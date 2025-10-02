use lsp_types::notification::{Notification, PublishDiagnostics};
use ropey::Rope;
use ruff_python_ast::{ModModule, PySourceType, Stmt};
use ruff_python_parser::{Parsed, Token, TokenKind};
use lsp_types::{Diagnostic, DiagnosticSeverity, MessageType, NumberOrString, Position, PublishDiagnosticsParams, Range, TextDocumentContentChangeEvent};
use tracing::{error, warn};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::{collections::HashMap, fs};
use crate::core::config::{DiagnosticFilter, DiagnosticFilterPathType};
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode, DiagnosticSetting};
use crate::features::node_index_ast::IndexedModule;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer;
use std::rc::Rc;
use std::cell::RefCell;
use crate::S;
use crate::constants::*;
use ruff_text_size::{Ranged, TextRange};

use super::odoo::SyncOdoo;

#[derive(Debug, PartialEq, Clone)]
pub enum NoqaInfo {
    None,
    All,
    Codes(Vec<String>),
}

pub fn combine_noqa_info(noqas: &Vec<NoqaInfo>) -> NoqaInfo {
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

#[derive(Debug, Clone)]
pub enum AstType {
    Python,
    Xml,
    Csv
}

/* Structure that hold ast and rope for FileInfo. It allows Fileinfo to hold it with a Rc<RefCell<>> to allow mutability and build on-the-fly
 */
#[derive(Debug)]
pub struct FileInfoAst {
    pub text_hash: u64,
    pub text_rope: Option<ropey::Rope>,
    pub indexed_module: Option<Arc<IndexedModule>>,
    pub ast_type: AstType,
}

impl FileInfoAst {
    pub fn get_stmts(&self) -> Option<&Vec<Stmt>> {
        self.indexed_module.as_ref().map(|module| &module.parsed.syntax().body)
    }
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
                text_rope: None,
                indexed_module: None,
                ast_type: AstType::Python,
            })),
            diagnostics: HashMap::new(),
            noqas_blocs: HashMap::new(),
            noqas_lines: HashMap::new(),
            diagnostic_filters: Vec::new(),
        }
    }
    pub fn update(&mut self, session: &mut SessionInfo, uri: &str, content: Option<&Vec<TextDocumentContentChangeEvent>>, version: Option<i32>, in_workspace: bool, force: bool) -> bool {
        // update the file info with the given information.
        // uri: indicates the path of the file
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
            for change in content.iter() {
                self.apply_change(change);
            }
        } else {
            match fs::read_to_string(uri) {
                Ok(content) => {
                    self.file_info_ast.borrow_mut().text_rope = Some(ropey::Rope::from(content.as_str()));
                },
                Err(e) => {
                    session.log_message(MessageType::ERROR, format!("Failed to read file {}, with error {}", uri, e));
                    return false;
                },
            };
        }
        let mut hasher = DefaultHasher::new();
        self.file_info_ast.borrow_mut().text_rope.clone().unwrap().hash(&mut hasher);
        let old_hash = self.file_info_ast.borrow().text_hash;
        self.file_info_ast.borrow_mut().text_hash = hasher.finish();
        if old_hash == self.file_info_ast.borrow().text_hash {
            return false;
        }
        self._build_ast(session, in_workspace);
        true
    }

    pub fn _build_ast(&mut self, session: &mut SessionInfo, in_workspace: bool) {
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
        let content = &fia.text_rope.as_ref().unwrap().slice(..);
        let source = content.to_string(); //cast to string to get a version with all changes
        drop(fia);
        let mut python_source_type = PySourceType::Python;
        if self.uri.ends_with(".pyi") {
            python_source_type = PySourceType::Stub;
        } else if self.uri.ends_with(".ipynb") {
            python_source_type = PySourceType::Ipynb;
        }
        let parsed_module = ruff_python_parser::parse_unchecked_source(source.as_str(), python_source_type);
        if in_workspace {
            self.noqas_blocs.clear();
            self.noqas_lines.clear();
            self.extract_tokens(&parsed_module, &source);
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

    /* if ast has been set to none to lower memory usage, try to reload it */
    pub fn prepare_ast(&mut self, session: &mut SessionInfo) {
        if self.file_info_ast.borrow_mut().text_rope.is_none() { //can already be set in xml files
            match fs::read_to_string(&self.uri) {
                Ok(content) => {
                    self.file_info_ast.borrow_mut().text_rope = Some(ropey::Rope::from(content.as_str()));
                },
                Err(_) => {
                    return;
                },
            };
        }
        let mut hasher = DefaultHasher::new();
        self.file_info_ast.borrow().text_rope.clone().unwrap().hash(&mut hasher);
        self.file_info_ast.borrow_mut().text_hash = hasher.finish();
        self._build_ast(session, session.sync_odoo.get_file_mgr().borrow().is_in_workspace(&self.uri));
    }

    pub fn extract_tokens(&mut self, parsed_module: &Parsed<ModModule>, source: &String) {
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
                            let char = self.file_info_ast.borrow().text_rope.as_ref().unwrap().try_byte_to_char(token.start().to_usize()).expect("unable to get char from bytes");
                            let line = self.file_info_ast.borrow().text_rope.as_ref().unwrap().try_char_to_line(char).ok().expect("unable to get line from char");
                            if let Some(previous_token) = previous_token {
                                let previous_token_char = self.file_info_ast.borrow().text_rope.as_ref().unwrap().try_byte_to_char(previous_token.start().to_usize()).expect("unable to get char from bytes");
                                let previous_token_line = self.file_info_ast.borrow().text_rope.as_ref().unwrap().try_char_to_line(previous_token_char).ok().expect("unable to get line from char");
                                if previous_token_line == line {
                                    self.noqas_lines.insert(line as u32, noqa_to_add.unwrap());
                                    noqa_to_add = None;
                                    continue;
                                }
                            }
                            if is_first_expr {
                                self.add_noqa_bloc(0, noqa_to_add.unwrap());
                                noqa_to_add = None;
                            }
                        }
                    }
                },
                TokenKind::Class | TokenKind::Def => {
                    if noqa_to_add.is_some() {
                        self.add_noqa_bloc(token.range().start().to_u32(), noqa_to_add.unwrap());
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
    }

    fn add_noqa_bloc(&mut self, index: u32, noqa_to_add: NoqaInfo) {
        if let Some(noqa_bloc) = self.noqas_blocs.remove(&index) {
            self.noqas_blocs.insert(index, combine_noqa_info(&vec![noqa_bloc, noqa_to_add]));
        } else {
            self.noqas_blocs.insert(index, noqa_to_add.clone());
        }
    }

    pub fn replace_diagnostics(&mut self, step: BuildSteps, diagnostics: Vec<Diagnostic>) {
        self.need_push = true;
        self.diagnostics.insert(step, diagnostics);
    }

    pub fn update_validation_diagnostics(&mut self, diagnostics: HashMap<BuildSteps, Vec<Diagnostic>>) {
        self.need_push = true;
        for (key, value) in diagnostics.iter() {
            self.diagnostics.entry(*key).or_insert_with(|| vec![]).extend(value.clone());
        }
    }

    fn update_range(&self, mut diagnostic: Diagnostic) -> Diagnostic {
        diagnostic.range.start = self.offset_to_position(diagnostic.range.start.line as usize);
        diagnostic.range.end = self.offset_to_position(diagnostic.range.end.line as usize);
        diagnostic
    }
    pub fn update_diagnostic_filters(&mut self, session: &SessionInfo) {
        self.diagnostic_filters = session.sync_odoo.config.diagnostic_filters.iter().cloned().filter(|filter| {
            match filter.path_type {
                DiagnosticFilterPathType::IN => {
                    filter.paths.iter().any(|p| p.matches(&self.uri))
                }
                DiagnosticFilterPathType::NOT_IN => {
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
                let updated = self.update_range(d.clone());
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

    pub fn offset_to_position_with_rope(rope: &Rope, offset: usize) -> Position {
        let char = rope.try_byte_to_char(offset).expect("unable to get char from bytes");
        let line = rope.try_char_to_line(char).ok().expect("unable to get line from char");
        let first_char_of_line = rope.try_line_to_char(line).expect("unable to get char from line");
        let column = char - first_char_of_line;
        Position::new(line as u32, column as u32)
    }

    pub fn offset_to_position(&self, offset: usize) -> Position {
        FileInfo::offset_to_position_with_rope(self.file_info_ast.borrow().text_rope.as_ref().expect("no rope provided"), offset)
    }

    pub fn text_range_to_range(&self, range: &TextRange) -> Range {
        Range {
            start: self.offset_to_position(range.start().to_usize()),
            end: self.offset_to_position(range.end().to_usize())
        }
    }

    pub fn std_range_to_range(&self, range: &std::ops::Range<usize>) -> Range {
        Range {
            start: self.offset_to_position(range.start),
            end: self.offset_to_position(range.end)
        }
    }

    pub fn position_to_offset_with_rope(rope: &Rope, line: u32, char: u32) -> usize {
        let line_char = rope.try_line_to_char(line as usize).expect("unable to get char from line");
        rope.try_char_to_byte(line_char + char as usize).expect("unable to get byte from char")
    }

    pub fn position_to_offset(&self, line: u32, char: u32) -> usize {
        FileInfo::position_to_offset_with_rope(self.file_info_ast.borrow().text_rope.as_ref().expect("no rope provided"), line, char)
    }

    fn apply_change(&mut self, change: &TextDocumentContentChangeEvent) {
        if change.range.is_none() {
            self.file_info_ast.borrow_mut().text_rope = Some(ropey::Rope::from_str(&change.text));
            return;
        }
        let start_idx = self.file_info_ast.borrow().text_rope.as_ref().unwrap().try_line_to_char(change.range.unwrap().start.line as usize).expect("Unable to get char position of line");
        let start_idx = start_idx + change.range.unwrap().start.character as usize;
        let end_idx = self.file_info_ast.borrow().text_rope.as_ref().unwrap().try_line_to_char(change.range.unwrap().end.line as usize).expect("Unable to get char position of line");
        let end_idx = end_idx + change.range.unwrap().end.character as usize;
        self.file_info_ast.borrow_mut().text_rope.as_mut().unwrap().remove(start_idx .. end_idx);
        self.file_info_ast.borrow_mut().text_rope.as_mut().unwrap().insert(start_idx, &change.text);
    }
}

#[derive(Debug)]
pub struct FileMgr {
    pub files: HashMap<String, Rc<RefCell<FileInfo>>>,
    workspace_folders: HashMap<String, String>,
    has_repeated_workspace_folders: bool,
}

impl FileMgr {

    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            workspace_folders: HashMap::new(),
            has_repeated_workspace_folders: false,
        }
    }

    #[allow(non_snake_case)]
    pub fn textRange_to_temporary_Range(range: &TextRange) -> Range {
        Range::new(
            Position::new(range.start().to_u32(), 0),
            Position::new(range.end().to_u32(), 0))
    }

    pub fn get_file_info(&self, path: &String) -> Option<Rc<RefCell<FileInfo>>> {
        self.files.get(path).cloned()
    }

    pub fn text_range_to_range(&self, session: &mut SessionInfo, path: &String, range: &TextRange) -> Range {
        let file = self.files.get(path);
        if let Some(file) = file {
            if file.borrow().file_info_ast.borrow().text_rope.is_none() {
                file.borrow_mut().prepare_ast(session);
            }
            return file.borrow().text_range_to_range(range);
        }
        //file not in cache, let's load rope on the fly
        match fs::read_to_string(path) {
            Ok(content) => {
                let rope = ropey::Rope::from(content.as_str());
                return Range {
                    start: FileInfo::offset_to_position_with_rope(&rope, range.start().to_usize()),
                    end: FileInfo::offset_to_position_with_rope(&rope, range.end().to_usize())
                };
            },
            Err(_) => session.log_message(MessageType::ERROR, format!("Failed to read file {}", path))
        };
        Range::default()
    }
    

    pub fn std_range_to_range(&self, session: &mut SessionInfo, path: &String, range: &std::ops::Range<usize>) -> Range {
        let file = self.files.get(path);
        if let Some(file) = file {
            if file.borrow().file_info_ast.borrow().text_rope.is_none() {
                file.borrow_mut().prepare_ast(session);
            }
            return file.borrow().std_range_to_range(range);
        }
        //file not in cache, let's load rope on the fly
        match fs::read_to_string(path) {
            Ok(content) => {
                let rope = ropey::Rope::from(content.as_str());
                return Range {
                    start: FileInfo::offset_to_position_with_rope(&rope, range.start),
                    end: FileInfo::offset_to_position_with_rope(&rope, range.end)
                };
            },
            Err(_) => session.log_message(MessageType::ERROR, format!("Failed to read file {}", path))
        };
        Range::default()
    }

    pub fn update_file_info(&mut self, session: &mut SessionInfo, uri: &str, content: Option<&Vec<TextDocumentContentChangeEvent>>, version: Option<i32>, force: bool) -> (bool, Rc<RefCell<FileInfo>>) {
        let file_info = self.files.entry(uri.to_string()).or_insert_with(|| {
            let mut file_info = FileInfo::new(uri.to_string());
            file_info.update_diagnostic_filters(session);
            Rc::new(RefCell::new(file_info))
        });
        let return_info = file_info.clone();
        //Do not modify the file if a version is not given but the file is opened
        let mut updated: bool = false;
        if (version.is_some() && version.unwrap() != -100) || !file_info.borrow().opened || force {
            let mut file_info_mut = (*return_info).borrow_mut();
            updated = file_info_mut.update(session, uri, content, version, self.is_in_workspace(uri), force);
            drop(file_info_mut);
        }
        (updated, return_info)
    }

    pub fn update_all_file_diagnostic_filters(&mut self, session: &SessionInfo) {
        for file_info in self.files.values() {
            file_info.borrow_mut().update_diagnostic_filters(session);
        }
    }

    pub fn delete_path(session: &mut SessionInfo, uri: &String) {
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

    pub fn add_workspace_folder(&mut self, name: String, path: String) {
        if self.workspace_folders.contains_key(&name) {
            warn!("Workspace folder with name {} already exists", name);
            self.has_repeated_workspace_folders = true;
        }
        let sanitized = PathBuf::from(path).sanitize();
        self.workspace_folders.insert(name, sanitized);
    }

    pub fn remove_workspace_folder(&mut self, name: String) {
        self.workspace_folders.remove(&name);
    }

    pub fn has_repeated_workspace_folders(&self) -> bool {
        self.has_repeated_workspace_folders
    }

    pub fn get_workspace_folders(&self) -> &HashMap<String, String> {
        &self.workspace_folders
    }

    pub fn is_in_workspace(&self, path: &str) -> bool {
        for p in self.workspace_folders.values() {
            if path.starts_with(p) {
                return true;
            }
        }
        false
    }

    pub fn pathname2uri(s: &String) -> lsp_types::Uri {
        let mut slash = "";
        if cfg!(windows) {
            slash = "/";
        }
        let pre_uri = match url::Url::parse(&format!("file://{}{}", slash, s)) {
            Ok(pre_uri) => pre_uri,
            Err(err) => panic!("unable to transform pathname to uri: {s}, {}", err)
        };
        match lsp_types::Uri::from_str(pre_uri.as_str()) {
            Ok(url) => url,
            Err(err) => panic!("unable to transform pathname to uri: {s}, {}", err)
        }
    }

    pub fn uri2pathname(s: &str) -> String {
        if let Ok(url) = url::Url::parse(s) {
            if let Ok(url) = url.to_file_path() {
                return url.sanitize();
            }
        }
        error!("Unable to extract path from uri: {s}");
        S!(s)
    }
}

