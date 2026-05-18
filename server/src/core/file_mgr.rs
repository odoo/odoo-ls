use ruff_python_ast::{ModModule, PySourceType, Stmt, token::{Token, TokenKind}};
use ruff_python_parser::Parsed;
use lsp_types::{Diagnostic, DiagnosticSeverity, MessageType, NumberOrString, Position, PublishDiagnosticsParams, Range, TextDocumentContentChangeEvent, Uri};
use lsp_types::notification::{Notification, PublishDiagnostics};
use ruff_source_file::{OneIndexed, PositionEncoding, SourceLocation};
use tracing::{error, warn};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, OnceLock};
use std::{collections::HashMap, fs};
use crate::core::config::{DiagnosticFilter, DiagnosticFilterPathType};
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode, DiagnosticSetting};
use crate::core::text_document::TextDocument;
use crate::features::node_index_ast::IndexedModule;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer;
use std::rc::Rc;
use std::cell::RefCell;
use crate::S;
use crate::constants::*;
use ruff_text_size::{Ranged, TextRange};

use super::odoo::SyncOdoo;

// Global static for legacy UNC path detection
pub static LEGACY_UNC_PATHS: OnceLock<AtomicBool> = OnceLock::new();

pub fn legacy_unc_paths() -> &'static AtomicBool {
    LEGACY_UNC_PATHS.get_or_init(|| AtomicBool::new(false))
}

/// Best-effort hint to the kernel that we're about to read every file under
/// `root` whose extension is in `exts`. Walks the tree once and fires
/// `posix_fadvise(WILLNEED)` on each match so the data lands in the page cache
/// before the build pipeline reads it. Errors are silently ignored — purely an
/// optimization. No-op on non-Linux platforms.
pub fn prefetch_dir(root: &std::path::Path, exts: &[&str]) {
    #[cfg(target_os = "linux")]
    {
        use std::fs::File;
        use std::os::fd::AsRawFd;
        use nix::fcntl::{posix_fadvise, PosixFadviseAdvice};

        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else { continue };
                if file_type.is_dir() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with('.') || name == "__pycache__" || name == "static" || name == "node_modules" {
                        continue;
                    }
                    stack.push(entry.path());
                } else if file_type.is_file() {
                    let path = entry.path();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if exts.contains(&ext) {
                        if let Ok(file) = File::open(&path) {
                            let _ = posix_fadvise(file.as_raw_fd(), 0, 0, PosixFadviseAdvice::POSIX_FADV_WILLNEED);
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (root, exts);
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum NoqaInfo {
    None,
    All,
    Codes(Vec<String>),
}

pub fn combine_noqa_info(noqas: &[NoqaInfo]) -> NoqaInfo {
    let mut codes = HashSet::new();
    for noqa in noqas.iter() {
        match noqa {
            NoqaInfo::None => {},
            NoqaInfo::All => {
                return NoqaInfo::All;
            }
            NoqaInfo::Codes(c) => {
                codes.extend(c.iter().cloned());
            }
        }
    }
    NoqaInfo::Codes(codes.iter().cloned().collect())
}

/// Result of scanning a parsed module's comment tokens. See [`scan_noqa`].
pub(crate) struct NoqaScan {
    pub blocs: HashMap<u32, NoqaInfo>,
    pub lines: HashMap<u32, NoqaInfo>,
    pub test_comments: Vec<(u32, Vec<String>)>,
}

/// Scan a parsed module's comment tokens for `noqa` directives and (in test mode)
/// `# OLS` test-expectation comments.
///
/// This is a pure function of the parse result + source text, so it can run on a
/// [`crate::core::pre_parser`] worker thread as well as on the build thread. The
/// build thread reaches it through [`FileInfo::extract_tokens`].
pub(crate) fn scan_noqa(
    parsed_module: &Parsed<ModModule>,
    source: &str,
    text_document: &TextDocument,
    encoding: PositionEncoding,
    parse_test_comments: bool,
) -> NoqaScan {
    fn add_noqa_bloc(blocs: &mut HashMap<u32, NoqaInfo>, index: u32, noqa: NoqaInfo) {
        if let Some(existing) = blocs.remove(&index) {
            blocs.insert(index, combine_noqa_info(&[existing, noqa]));
        } else {
            blocs.insert(index, noqa);
        }
    }
    let mut blocs: HashMap<u32, NoqaInfo> = HashMap::new();
    let mut lines: HashMap<u32, NoqaInfo> = HashMap::new();
    let mut test_comments: Vec<(u32, Vec<String>)> = Vec::new();
    let mut is_first_expr: bool = true;
    let mut noqa_to_add = None;
    let mut previous_token: Option<&Token> = None;
    for token in parsed_module.tokens().iter() {
        match token.kind() {
            TokenKind::Comment => {
                let text = &source[token.range()];
                if text.starts_with("#noqa") || text.starts_with("# noqa") || text.starts_with("# odools: noqa") {
                    let after_noqa = text.split("noqa").skip(1).next();
                    if let Some(after_noqa) = after_noqa {
                        let mut codes = vec![];
                        for code in after_noqa.split(|c: char| c == ',' || c.is_whitespace() || c == ':') {
                            let code = code.trim();
                            if code.len() > 0 {
                                codes.push(code.to_string());
                            }
                        }
                        if codes.len() > 0 {
                            noqa_to_add = Some(NoqaInfo::Codes(codes));
                        } else {
                            noqa_to_add = Some(NoqaInfo::All);
                        }
                        let source_location = text_document.index().source_location(token.start(), text_document.contents(), encoding);
                        if let Some(previous_token) = previous_token {
                            let prev_location = text_document.index().source_location(previous_token.start(), text_document.contents(), encoding);
                            if prev_location.line == source_location.line {
                                lines.insert(source_location.line.to_zero_indexed() as u32, noqa_to_add.unwrap());
                                noqa_to_add = None;
                                continue;
                            }
                        }
                        if is_first_expr {
                            add_noqa_bloc(&mut blocs, 0, noqa_to_add.unwrap());
                            noqa_to_add = None;
                        }
                    }
                }
                if parse_test_comments {
                    if text.starts_with("#OLS") || text.starts_with("# OLS") {
                        let codes = text.split(",").map(|s| s.trim().trim_start_matches('#').trim().to_string()).collect::<Vec<String>>();
                        let source_location = text_document.index().source_location(token.start(), text_document.contents(), encoding);
                        test_comments.push((source_location.line.to_zero_indexed() as u32, codes));
                    }
                }
            },
            TokenKind::Class | TokenKind::Def => {
                if noqa_to_add.is_some() {
                    add_noqa_bloc(&mut blocs, token.range().start().to_u32(), noqa_to_add.unwrap());
                    noqa_to_add = None;
                }
            }
            TokenKind::NonLogicalNewline => {}
            _ => {
                is_first_expr = false
            }
        }
        previous_token = Some(token);
    }
    NoqaScan { blocs, lines, test_comments }
}

#[derive(Debug, Clone)]
pub enum AstType {
    Python,
    Xml,
    Csv
}

/* Structure that hold ast and text_document for FileInfo. It allows Fileinfo to hold it with a Rc<RefCell<>> to allow mutability and build on-the-fly
 */
#[derive(Debug)]
pub struct FileInfoAst {
    pub text_hash: u64,
    pub text_document: Option<TextDocument>,
    pub indexed_module: Option<Arc<IndexedModule>>,
    pub ast_type: AstType,
}

impl FileInfoAst {
    pub fn get_stmts(&self) -> Option<&Vec<Stmt>> {
        self.indexed_module.as_ref().map(|module| &module.parsed.syntax().body)
    }
}

/// A fully-prepared Python AST produced by a [`crate::core::pre_parser`] worker
/// thread, ready to be slotted into a [`FileInfo`] by the build thread without
/// re-reading or re-parsing the file. Built off the build thread to overlap the
/// (pure-CPU) ruff parse with the actual module build. See [`FileInfo::apply_prepared_ast`].
#[derive(Debug)]
pub struct PreparedAst {
    pub text_hash: u64,
    pub text_document: TextDocument,
    pub indexed_module: Arc<IndexedModule>,
    pub noqas_blocs: HashMap<u32, NoqaInfo>,
    pub noqas_lines: HashMap<u32, NoqaInfo>,
    pub diag_test_comments: Vec<(u32, Vec<String>)>,
}

#[derive(Debug)]
pub struct FileInfo {
    pub version: Option<i32>,
    pub uri: String,
    pub valid: bool, // indicates if the file contains syntax error or not
    pub opened: bool,
    need_push: bool,
    pub file_info_ast: Rc<RefCell<FileInfoAst>>,
    diagnostics: HashMap<BuildSteps, Vec<Diagnostic>>,
    pub noqas_blocs: HashMap<u32, NoqaInfo>,
    noqas_lines: HashMap<u32, NoqaInfo>,
    diagnostic_filters: Vec<DiagnosticFilter>,

    pub diag_test_comments: Vec<(u32, Vec<String>)>, //for tests: line and list of codes
}

impl FileInfo {
    fn new(uri: String) -> Self {
        Self {
            version: None,
            uri,
            valid: true,
            opened: false,
            need_push: false,
            file_info_ast: Rc::new(RefCell::new(FileInfoAst {
                text_hash: 0,
                text_document: None,
                indexed_module: None,
                ast_type: AstType::Python,
            })),
            diagnostics: HashMap::new(),
            noqas_blocs: HashMap::new(),
            noqas_lines: HashMap::new(),
            diagnostic_filters: Vec::new(),
            diag_test_comments: vec![],
        }
    }
    pub fn update(&mut self, session: &mut SessionInfo, path: &str, content: Option<&[TextDocumentContentChangeEvent]>, version: Option<i32>, is_external: bool, force: bool, is_untitled: bool) -> bool {
        // update the file info with the given information.
        // path: indicates the path of the file
        // content: if content is given, it will be used to update the ast and text_rope, if not, the loading will be from the disk
        // version: if the version is provided, the file_info wil be updated only if the new version is higher.
        // -100 can be given as version number to indicates that the file has not been opened yet, and that we have to load it ourself
        // See https://github.com/Microsoft/language-server-protocol/issues/177
        // Return true if the update has been done and not discarded
        match version {
            // -100, we set FileInfo to -100 if it was not opened yet. Otherwise, we do not change the version
            Some(-100) => if !self.opened {
                self.version = Some(-100);
            } else if !force { // If opened, with -100, we do not update
                return false;
            },
            // normal version number, we update if higher, and set to opened anyway
            Some(version) => {
                self.opened = true;
                if self.version.map(|v| version <= v).unwrap_or(false) && !force {
                    // If the version is not higher, we do not update the file
                    return false;
                }
                self.version = Some(version);
            }
            // no version provided, we update only if the file is not opened or on force
            None if self.version.is_some() && !force => return false,
            _ => {},
        }
        self.diagnostics.clear();
        if let Some(content) = content {
            // If we are in did open, we create a new text_document
            // I.E. we have one content change event with no range
            // See [`Odoo:handle_did_open`]
            if content.len() == 1 && content[0].range.is_none() {
                self.file_info_ast.borrow_mut().text_document = Some(TextDocument::new(content[0].text.clone(), self.version.expect("Expected version on file did Open")));
            } else {
                self.file_info_ast.borrow_mut().text_document.as_mut().unwrap().apply_changes(content, version.unwrap(), session.sync_odoo.encoding);
            }
        } else if is_untitled {
            session.log_message(MessageType::ERROR, format!("Attempt to update untitled file {}, without changes", path));
            return false;
        } else {
            // A pre-parser worker thread may have already read + parsed this file
            // ahead of the build. If so, slot the prepared AST straight in — no
            // disk read, no parse. Best-effort: a miss just falls through to the
            // inline path below. See `crate::core::pre_parser`.
            if let Some(cache) = session.sync_odoo.pre_parse_cache.clone() {
                if path.ends_with(".py") || path.ends_with(".pyi") {
                    if let Some(prepared) = cache.take(&PathBuf::from(path).sanitize()) {
                        if self.file_info_ast.borrow().text_hash == prepared.text_hash {
                            return false;
                        }
                        self.apply_prepared_ast(session, prepared);
                        return true;
                    }
                }
            }
            match fs::read_to_string(path) {
                Ok(content) => {
                    self.file_info_ast.borrow_mut().text_document = Some(TextDocument::new(content, self.version.unwrap_or(-1)));
                },
                Err(e) => {
                    session.log_message(MessageType::ERROR, format!("Failed to read file {}, with error {}", path, e));
                    return false;
                },
            };
        }
        let mut hasher = DefaultHasher::new();
        self.file_info_ast.borrow_mut().text_document.clone().unwrap().hash(&mut hasher);
        let old_hash = self.file_info_ast.borrow().text_hash;
        self.file_info_ast.borrow_mut().text_hash = hasher.finish();
        if old_hash == self.file_info_ast.borrow().text_hash {
            return false;
        }
        self._build_ast(session, is_external);
        true
    }

    pub fn _build_ast(&mut self, session: &mut SessionInfo, is_external: bool) {
        if self.uri.ends_with(".xml") {
            self.file_info_ast.borrow_mut().ast_type = AstType::Xml;
            return;
        }
        if self.uri.ends_with(".csv") {
            self.file_info_ast.borrow_mut().ast_type = AstType::Csv;
            return;
        }
        let mut diagnostics = vec![];
        let fia_rc = self.file_info_ast.clone();
        let fia = fia_rc.borrow_mut();
        let source = S!(fia.text_document.as_ref().unwrap().contents());
        drop(fia);
        let mut python_source_type = PySourceType::Python;
        if self.uri.ends_with(".pyi") {
            python_source_type = PySourceType::Stub;
        } else if self.uri.ends_with(".ipynb") {
            python_source_type = PySourceType::Ipynb;
        }
        let parsed_module = ruff_python_parser::parse_unchecked_source(source.as_str(), python_source_type);
        if !is_external {
            self.noqas_blocs.clear();
            self.noqas_lines.clear();
            self.extract_tokens(&parsed_module, &source, session.sync_odoo.encoding, session.sync_odoo.test_mode);
        }
        self.valid = true;
        for error in parsed_module.errors().iter() {
            self.valid = false;
            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS01000, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range{
                        start: Position::new(error.location.start().to_u32(), 0),
                        end: Position::new(error.location.end().to_u32(), 0)
                    },
                    message: error.error.to_string(),
                    ..diagnostic_base
                });
            }
        }
        self.file_info_ast.borrow_mut().indexed_module = Some(IndexedModule::new(parsed_module));
        self.replace_diagnostics(BuildSteps::SYNTAX, diagnostics);
    }

    /// Slot a [`PreparedAst`] produced by a background [`crate::core::pre_parser`]
    /// worker into this `FileInfo`, instead of reading and parsing the file inline.
    /// Mirrors the Python branch of [`Self::_build_ast`] (syntax diagnostics are
    /// rebuilt here because they depend on the session's noqa/severity config).
    fn apply_prepared_ast(&mut self, session: &mut SessionInfo, prepared: PreparedAst) {
        let mut diagnostics = vec![];
        self.valid = true;
        for error in prepared.indexed_module.parsed.errors().iter() {
            self.valid = false;
            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS01000, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range{
                        start: Position::new(error.location.start().to_u32(), 0),
                        end: Position::new(error.location.end().to_u32(), 0)
                    },
                    message: error.error.to_string(),
                    ..diagnostic_base
                });
            }
        }
        self.noqas_blocs = prepared.noqas_blocs;
        self.noqas_lines = prepared.noqas_lines;
        self.diag_test_comments = prepared.diag_test_comments;
        {
            let mut fia = self.file_info_ast.borrow_mut();
            fia.text_hash = prepared.text_hash;
            fia.text_document = Some(prepared.text_document);
            fia.indexed_module = Some(prepared.indexed_module);
            fia.ast_type = AstType::Python;
        }
        self.replace_diagnostics(BuildSteps::SYNTAX, diagnostics);
    }

    /* if ast has been set to none to lower memory usage, try to reload it */
    pub fn prepare_ast(&mut self, session: &mut SessionInfo) {
        if self.file_info_ast.borrow_mut().text_document.is_none() { //can already be set in xml files
            match fs::read_to_string(&self.uri) {
                Ok(content) => {
                    self.file_info_ast.borrow_mut().text_document = Some(TextDocument::new(content, self.version.unwrap_or(-1)));
                },
                Err(_) => {
                    return;
                },
            };
        }
        let mut hasher = DefaultHasher::new();
        self.file_info_ast.borrow().text_document.clone().unwrap().hash(&mut hasher);
        self.file_info_ast.borrow_mut().text_hash = hasher.finish();
        self._build_ast(session, session.sync_odoo.get_file_mgr().borrow().is_in_workspace(&self.uri));
    }

    fn extract_tokens(&mut self, parsed_module: &Parsed<ModModule>, source: &String, encoding: PositionEncoding, parse_test_comments: bool) {
        let scan = {
            let fia = self.file_info_ast.borrow();
            scan_noqa(parsed_module, source, fia.text_document.as_ref().unwrap(), encoding, parse_test_comments)
        };
        self.noqas_blocs.extend(scan.blocs);
        self.noqas_lines.extend(scan.lines);
        self.diag_test_comments.extend(scan.test_comments);
    }

    pub fn replace_diagnostics(&mut self, step: BuildSteps, diagnostics: Vec<Diagnostic>) {
        self.need_push = true;
        self.diagnostics.insert(step, diagnostics);
    }

    pub fn update_validation_diagnostics(&mut self, diagnostics: HashMap<BuildSteps, Vec<Diagnostic>>) {
        self.need_push = true;
        for (key, value) in diagnostics {
            self.diagnostics.entry(key).or_default().extend(value);
        }
    }

    fn update_range(&self, mut diagnostic: Diagnostic, encoding: PositionEncoding) -> Diagnostic {
        diagnostic.range.start = self.offset_to_position(diagnostic.range.start.line, encoding);
        diagnostic.range.end = self.offset_to_position(diagnostic.range.end.line, encoding);
        diagnostic
    }
    pub fn update_diagnostic_filters(&mut self, session: &SessionInfo) {
        self.diagnostic_filters = session.sync_odoo.config.diagnostic_filters.iter().cloned().filter(|filter| {
            match filter.path_type {
                DiagnosticFilterPathType::In => {
                    filter.paths.iter().any(|p| p.matches(&self.uri))
                }
                DiagnosticFilterPathType::NotIn => {
                    !filter.paths.iter().any(|p| p.matches(&self.uri))
                }
            }
        }).collect::<Vec<_>>();
    }

    pub fn publish_diagnostics(&mut self, session: &mut SessionInfo) {
        if self.need_push {
            let mut all_diagnostics = Vec::new();

            'diagnostics: for d in self.diagnostics.values().flatten() {
                //check noqa lines
                let updated = self.update_range(d.clone(), session.sync_odoo.encoding);
                let updated_line = updated.range.start.line;
                if let Some(noqa_line) = self.noqas_lines.get(&updated_line) {
                    match noqa_line {
                        NoqaInfo::None => {},
                        NoqaInfo::All => {
                            continue;
                        }
                        NoqaInfo::Codes(codes) => {
                            match &updated.code {
                                None => {continue;},
                                Some(NumberOrString::Number(n)) => {
                                    if codes.contains(&n.to_string()) {
                                        continue;
                                    }
                                },
                                Some(NumberOrString::String(s)) => {
                                    if codes.contains(&s) {
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
                for filter in self.diagnostic_filters.iter() {
                    if !filter.codes.is_empty(){
                        // we pass the filter if we do not have code, or does it not match the filter
                        let Some(updated_code) = &updated.code else {
                            continue;
                        };
                        let updated_code = match updated_code {
                            NumberOrString::Number(n) => n.to_string(),
                            NumberOrString::String(s) => s.clone(),
                        };
                        if !filter.codes.iter().any(|re| re.is_match(&updated_code)) {
                            continue;
                        }
                    }
                    if !filter.types.is_empty() {
                        // we pass the filter if we do not have severity, or does it not match the filter
                        let Some(severity) = &updated.severity else {
                            continue;
                        };
                        if !filter.types.iter().any(|t| match (t, severity) {
                            (DiagnosticSetting::Error, &DiagnosticSeverity::ERROR)
                            | (DiagnosticSetting::Warning, &DiagnosticSeverity::WARNING)
                            | (DiagnosticSetting::Info, &DiagnosticSeverity::INFORMATION)
                            | (DiagnosticSetting::Hint, &DiagnosticSeverity::HINT) => true,
                            _ => false,
                        }) {
                            continue;
                        }
                    }
                    continue 'diagnostics;
                }
                all_diagnostics.push(updated);
            }
            session.send_notification::<PublishDiagnosticsParams>(PublishDiagnostics::METHOD, PublishDiagnosticsParams{
                uri: FileMgr::pathname2uri(&self.uri),
                diagnostics: all_diagnostics,
                version: self.version,
            });
            self.need_push = false;
        }
    }

    fn offset_to_position_with_text_document(text_document: &TextDocument, offset: u32, encoding: PositionEncoding) -> Position {
        let location = text_document.index().source_location(offset.into(), text_document.contents(), encoding);
        let line = u32::try_from(location.line.to_zero_indexed()).expect("row usize fits in u32");
        let character = u32::try_from(location.character_offset.to_zero_indexed())
            .expect("character usize fits in u32");
        Position::new(line, character)
    }

    fn try_offset_to_position_with_text_document(text_document: &TextDocument, offset: u32, encoding: PositionEncoding) -> Option<Position> {
        let location = text_document.index().source_location(offset.into(), text_document.contents(), encoding);
        let line = u32::try_from(location.line.to_zero_indexed()).ok()?;
        let character = u32::try_from(location.character_offset.to_zero_indexed()).ok()?;
        Some(Position::new(line, character))
    }

    pub fn offset_to_position(&self, offset: u32, encoding: PositionEncoding) -> Position {
        FileInfo::offset_to_position_with_text_document(self.file_info_ast.borrow().text_document.as_ref().expect("no text_document provided"), offset, encoding)
    }

    fn try_offset_to_position(&self, offset: u32, encoding: PositionEncoding) -> Option<Position> {
        FileInfo::try_offset_to_position_with_text_document(self.file_info_ast.borrow().text_document.as_ref()?, offset, encoding)
    }

    pub fn text_range_to_range(&self, range: &TextRange, encoding: PositionEncoding) -> Range {
        Range {
            start: self.offset_to_position(range.start().to_usize() as u32, encoding),
            end: self.offset_to_position(range.end().to_usize() as u32, encoding)
        }
    }

    pub fn try_text_range_to_range(&self, range: &TextRange, encoding: PositionEncoding) -> Option<Range> {
        Some(Range {
            start: self.try_offset_to_position(range.start().to_usize() as u32, encoding)?,
            end: self.try_offset_to_position(range.end().to_usize() as u32, encoding)?
        })
    }

    pub fn std_range_to_range(&self, range: &std::ops::Range<usize>, encoding: PositionEncoding) -> Range {
        Range {
            start: self.offset_to_position(range.start as u32, encoding),
            end: self.offset_to_position(range.end as u32, encoding)
        }
    }

    fn position_to_offset_with_text_document(text_document: &TextDocument, line: u32, char: u32, encoding: PositionEncoding) -> usize {
        let position = SourceLocation {
            line: OneIndexed::from_zero_indexed(line as usize),
            character_offset: OneIndexed::from_zero_indexed(char as usize),
        };
        text_document.index().offset(position, text_document.contents(), encoding).into()
    }

    pub fn position_to_offset(&self, line: u32, char: u32, encoding: PositionEncoding) -> usize {
        FileInfo::position_to_offset_with_text_document(self.file_info_ast.borrow().text_document.as_ref().expect("no text_document provided"), line, char, encoding)
    }
}

#[derive(Debug)]
pub struct FileMgr {
    pub files: HashMap<String, Rc<RefCell<FileInfo>>>,
    untitled_files: HashMap<String, Rc<RefCell<FileInfo>>>, // key: untitled URI or unique name
    workspace_folders: HashSet<(String, Uri)>,
}

impl FileMgr {

    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            untitled_files: HashMap::new(),
            workspace_folders: HashSet::new(),
        }
    }

    #[allow(non_snake_case)]
    pub fn textRange_to_temporary_Range(range: &TextRange) -> Range {
        Range::new(
            Position::new(range.start().to_u32(), 0),
            Position::new(range.end().to_u32(), 0))
    }

    pub fn get_file_info(&self, path: &str) -> Option<Rc<RefCell<FileInfo>>> {
        if Self::is_untitled(path) {
            self.untitled_files.get(path).cloned()
        } else {
            self.files.get(path).cloned()
        }
    }

    pub fn text_range_to_range(&self, session: &mut SessionInfo, path: &String, range: &TextRange) -> Range {
        let file = if Self::is_untitled(path) {
            self.untitled_files.get(path)
        } else {
            self.files.get(path)
        };
        if let Some(file) = file {
            if file.borrow().file_info_ast.borrow().text_document.is_none() {
                file.borrow_mut().prepare_ast(session);
            }
            return file.borrow().text_range_to_range(range, session.sync_odoo.encoding);
        }
        // For untitled, never try to read from disk
        if Self::is_untitled(path) {
            session.log_message(MessageType::ERROR, format!("Untitled file {} not found in memory", path));
            return Range::default();
        }
        //file not in cache, let's load text_document on the fly
        match fs::read_to_string(path) {
            Ok(content) => {
                let text_document = TextDocument::new(content, -1);
                return Range {
                    start: FileInfo::offset_to_position_with_text_document(&text_document, range.start().into(), session.sync_odoo.encoding),
                    end: FileInfo::offset_to_position_with_text_document(&text_document, range.end().into(), session.sync_odoo.encoding)
                };
            },
            Err(_) => session.log_message(MessageType::ERROR, format!("Failed to read file {}", path))
        };
        Range::default()
    }


    pub fn std_range_to_range(&self, session: &mut SessionInfo, path: &str, range: &std::ops::Range<usize>) -> Range {
        let file = if Self::is_untitled(path) {
            self.untitled_files.get(path)
        } else {
            self.files.get(path)
        };
        if let Some(file) = file {
            if file.borrow().file_info_ast.borrow().text_document.is_none() {
                file.borrow_mut().prepare_ast(session);
            }
            return file.borrow().std_range_to_range(range, session.sync_odoo.encoding);
        }
        // For untitled, never try to read from disk
        if Self::is_untitled(path) {
            session.log_message(MessageType::ERROR, format!("Untitled file {} not found in memory", path));
            return Range::default();
        }
        //file not in cache, let's load text_document on the fly
        match fs::read_to_string(path) {
            Ok(content) => {
                let text_document = TextDocument::new(content, -1);
                return Range {
                    start: FileInfo::offset_to_position_with_text_document(&text_document, range.start as u32, session.sync_odoo.encoding),
                    end: FileInfo::offset_to_position_with_text_document(&text_document, range.end as u32, session.sync_odoo.encoding)
                };
            },
            Err(_) => session.log_message(MessageType::ERROR, format!("Failed to read file {}", path))
        };
        Range::default()
    }

    /// Returns true if the path/uri is an untitled (in-memory) file.
    /// by convention, untitled files start with "untitled:".
    pub fn is_untitled(path: &str) -> bool {
        path.starts_with("untitled:")
    }

    pub fn update_file_info(&mut self, session: &mut SessionInfo, uri: &str, content: Option<&[TextDocumentContentChangeEvent]>, version: Option<i32>, force: bool) -> (bool, Rc<RefCell<FileInfo>>) {
        let is_untitled = Self::is_untitled(uri);
        let entry = if is_untitled {
            self.untitled_files.entry(uri.to_string())
        } else {
            self.files.entry(uri.to_string())
        };
        let file_info = entry.or_insert_with(|| {
            let mut file_info = FileInfo::new(uri.to_string());
            file_info.update_diagnostic_filters(session);
            Rc::new(RefCell::new(file_info))
        });
        let return_info = file_info.clone();
        //Do not modify the file if a version is not given but the file is opened
        let mut updated: bool = false;
        if (version.is_some() && version.unwrap() != -100) || !file_info.borrow().opened || force {
            let mut file_info_mut = (*return_info).borrow_mut();
            let ep_mgr = session.sync_odoo.entry_point_mgr.borrow();
            let is_part_of_ep = ep_mgr.iter_all_but_public().any(|entry| uri.starts_with(&entry.borrow().path));
            drop(ep_mgr);
            updated = file_info_mut.update(session, uri, content, version, !is_part_of_ep, force, is_untitled);
            drop(file_info_mut);
        }
        (updated, return_info)
    }

    pub fn update_all_file_diagnostic_filters(&mut self, session: &SessionInfo) {
        for file_info in self.files.values() {
            file_info.borrow_mut().update_diagnostic_filters(session);
        }
    }

    pub fn delete_path(session: &mut SessionInfo, uri: &str) {
        //delete all files that are the uri or in subdirectory
        let matching_keys: Vec<String> = session.sync_odoo.get_file_mgr().borrow_mut().files.keys().filter(|k| PathBuf::from(k).starts_with(uri)).cloned().collect();
        for key in matching_keys {
            let to_del = session.sync_odoo.get_file_mgr().borrow_mut().files.remove(&key);
            if let Some(to_del) = to_del {
                if SyncOdoo::is_in_workspace_or_entry(session, uri) {
                    let mut to_del = (*to_del).borrow_mut();
                    to_del.replace_diagnostics(BuildSteps::SYNTAX, vec![]);
                    to_del.replace_diagnostics(BuildSteps::ARCH, vec![]);
                    to_del.replace_diagnostics(BuildSteps::ARCH_EVAL, vec![]);
                    to_del.replace_diagnostics(BuildSteps::VALIDATION, vec![]);
                    to_del.publish_diagnostics(session)
                }
            }
        }
    }

    pub fn clear(session: &mut SessionInfo) {
        let file_mgr = session.sync_odoo.get_file_mgr();
        let file_mgr = file_mgr.borrow();
        for file in file_mgr.files.values().clone() {
            if !file_mgr.is_in_workspace(&file.borrow().uri) {
                continue;
            }
            let mut found = false;
            for entry in session.sync_odoo.entry_point_mgr.borrow().custom_entry_points.iter() {
                let entry = entry.borrow();
                if &file.borrow().uri == &entry.path {
                    found = true;
                    break;
                }
            }
            if !found {
                continue;
            }
            let mut to_del = file.borrow_mut();
            to_del.replace_diagnostics(BuildSteps::SYNTAX, vec![]);
            to_del.replace_diagnostics(BuildSteps::ARCH, vec![]);
            to_del.replace_diagnostics(BuildSteps::ARCH_EVAL, vec![]);
            to_del.replace_diagnostics(BuildSteps::VALIDATION, vec![]);
            to_del.publish_diagnostics(session)
        }
        drop(file_mgr);
        session.sync_odoo.get_file_mgr().borrow_mut().files.clear();
    }

    /// Add workspace folder by name and uri
    /// Same format as received from the client
    pub fn add_workspace_folder(&mut self, name: String, uri: Uri) {
        self.workspace_folders.insert((name, uri));
    }

    /// Remove workspace folder by name and uri
    /// Same format as received from the client
    pub fn remove_workspace_folder(&mut self, name: String, uri: Uri) {
        self.workspace_folders.remove(&(name, uri));
    }

    pub fn get_workspace_folders(&self) -> &HashSet<(String, Uri)> {
        &self.workspace_folders
    }

    /// Get workspace folders with sanitized path strings instead of URIs
    pub fn get_processed_workspace_folders(&self) -> HashSet<(String, String)> {
        self.workspace_folders.iter().map(|(name, uri)| {
            (name.clone(), FileMgr::uri2pathname(uri.as_str()))
        }).collect()
    }

    /// Get a map of workspace folder name to sanitized path string
    /// of only unique workspace names, repeated names are skipped
    pub fn get_unique_workspace_folders(&self) -> HashMap<String, String> {
        let mut visited_names= HashSet::new();
        let mut unique_folders = HashMap::new();
        for (name, uri) in self.workspace_folders.iter() {
            if visited_names.insert(name.clone()) {
                unique_folders.insert(name.clone(), FileMgr::uri2pathname(uri.as_str()));
            } else {
                unique_folders.remove(name);
                warn!("Workspace folder name '{}' is not unique, skipping it for unique workspace folder retrieval", name);
            }
        }
        unique_folders
    }

    pub fn is_in_workspace(&self, path: &str) -> bool {
        self.workspace_folders.iter().any(|(_, uri)| path.starts_with(&FileMgr::uri2pathname(uri.as_str())))
    }

    pub fn pathname2uri(s: &str) -> lsp_types::Uri {
        Self::try_pathname2uri(s).unwrap_or_else(|err| panic!("unable to transform pathname to uri: {s}, {}", err))
    }

    pub fn try_pathname2uri(s: &str) -> Result<lsp_types::Uri, anyhow::Error> {
        let pre_uri = if s.starts_with("untitled:"){
            s.to_string()
        } else {
            let mut slash = "";
            if cfg!(windows) {
                slash = "/";
            }
            // If the path starts with \\\\, we want to remove it and also set slash to empty string
            // Such that we have file://wsl.localhost/<path> for example
            // For normal paths we do want file:///C:/...
            // For some editors like PyCharm they use the legacy windows UNC urls so we have file:////wsl.localhost/<path>
            let (replaced, unc) = if s.starts_with("\\\\") {
                slash = "";
                (s.replacen("\\\\", "", 1), true)
            } else {
                (s.to_string(), false)
            };
            // Use legacy UNC flag to determine if we need four slashes
            match url::Url::parse(&format!("file://{}{}", slash, replaced)) {
                Ok(pre_uri) => {
                    if unc && legacy_unc_paths().load(Ordering::Relaxed){
                        pre_uri.to_string().replace("file://", "file:////")
                    } else {
                        pre_uri.to_string()
                    }
                },
                Err(err) => return Err(err.into())
            }
        };
        lsp_types::Uri::from_str(&pre_uri).map_err(|err| err.into())
    }

    pub fn uri2pathname(s: &str) -> String {
        // Detect legacy UNC path (file:////)
        if s.starts_with("file:////") {
            legacy_unc_paths().store(true, Ordering::Relaxed);
        }
        let str_repr = s.replace("file:////", "file://");
        match url::Url::parse(&str_repr) {
            Ok(url) => {
                match url.to_file_path() {
                    Ok(path) => path.sanitize(),
                    Err(_) => {
                        error!("Unable to convert url to file path: {s}");
                        S!(s)
                    }
                }
            },
            Err(err) => {
                error!("Unable to parse uri: {s}, {}", err);
                S!(s)
            }
        }
    }
}
