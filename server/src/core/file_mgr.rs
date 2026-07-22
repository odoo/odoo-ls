use oxc::{allocator::Allocator, diagnostics::OxcDiagnostic, parser::Parser, semantic::SemanticBuilder, span::SourceType};
use oxc_linter::{ConfigStore, ConfigStoreBuilder, ContextSubHost, ExternalPluginStore, LintOptions, ModuleRecord};
use oxc::syntax::module_record::ExportExportName;
use ruff_python_ast::{ModModule, PySourceType, Stmt, token::{Token, TokenKind}};
use ruff_python_parser::Parsed;
use lsp_types::{Diagnostic, DiagnosticSeverity, MessageType, NumberOrString, Position, PublishDiagnosticsParams, Range, TextDocumentContentChangeEvent, Uri};
use lsp_types::notification::{Notification, PublishDiagnostics};
use ruff_source_file::{LineIndex, OneIndexed, PositionEncoding, SourceLocation};
use rustc_hash::FxHasher;
use tracing::{error, warn};
use std::path::Path;
use crate::core::js_arch_builder::{JsDeclaration, JsExportKind};
use crate::core::js_arch_builder::{ComponentDescriptor, JsTemplateRef};
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, OnceLock};
use std::{fs};
use crate::utils::{HashMap, HashSet};
use crate::core::{config::{DiagnosticFilter, DiagnosticFilterPathType}, js_arch_builder, js_utils};
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

#[derive(Debug, PartialEq, Clone)]
pub enum NoqaInfo {
    None,
    All,
    Codes(Vec<String>),
}

pub fn combine_noqa_info(noqas: &[NoqaInfo]) -> NoqaInfo {
    let mut codes = HashSet::default();
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
struct NoqaScan {
    pub blocs: HashMap<u32, NoqaInfo>,
    pub lines: HashMap<u32, NoqaInfo>,
    pub test_comments: Vec<(u32, Vec<String>)>,
}


/// Select the ruff [`PySourceType`] for a path by extension: `.pyi` → stub,
/// `.ipynb` → notebook, everything else → regular Python.
pub fn python_source_type(path: &str) -> PySourceType {
    if path.ends_with(".pyi") {
        PySourceType::Stub
    } else if path.ends_with(".ipynb") {
        PySourceType::Ipynb
    } else {
        PySourceType::Python
    }
}

/// Parsed product of a Python source: its indexed AST plus the noqa/`# OLS`
/// directives scanned from its comments. See [`parse_python`].
#[derive(Debug)]
pub struct ParsedPython {
    pub indexed_module: Arc<IndexedModule>,
    pub noqas_blocs: HashMap<u32, NoqaInfo>,
    pub noqas_lines: HashMap<u32, NoqaInfo>,
    pub diag_test_comments: Vec<(u32, Vec<String>)>,
}

/// Parse `text_document` as Python and, unless `is_external`, scan its comments
/// for noqa/`# OLS` directives.
/// Shared by the build thread ([`FileInfo::_build_ast`]) and the pre-parse
/// workers ([`crate::core::pre_parser`])
pub fn parse_python(
    text_document: &TextDocument,
    source_type: PySourceType,
    encoding: PositionEncoding,
    test_mode: bool,
    skip_noqa_scan: bool,
) -> ParsedPython {
    let parsed = ruff_python_parser::parse_unchecked_source(text_document.contents(), source_type);
    // External files skip the noqa scan: their diagnostics are never published.
    let (noqas_blocs, noqas_lines, diag_test_comments) = if skip_noqa_scan {
        (HashMap::default(), HashMap::default(), Vec::new())
    } else {
        let scan = FileInfo::scan_noqa(&parsed, text_document.contents(), text_document, encoding, test_mode);
        (scan.blocs, scan.lines, scan.test_comments)
    };
    ParsedPython {
        indexed_module: IndexedModule::new(parsed),
        noqas_blocs,
        noqas_lines,
        diag_test_comments,
    }
}

/// Parsed product of a JS source: the OWL data extracted from it, the module
/// specifiers it imports, and its OXC diagnostics, already in LSP form. See [`parse_js`].
#[derive(Debug, Default)]
pub struct ParsedJs {
    pub template_refs: Vec<JsTemplateRef>,
    pub component_descriptors: Vec<ComponentDescriptor>,
    /// Named declarations, for workspace symbols.
    pub decls: Vec<JsDeclaration>,
    pub imports: Vec<String>,
    pub reexports: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Stack size for JS parsing threads, sized for OXC recursive descent on minified libs.
pub const JS_PARSE_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Parse `contents` as JS: OXC parse + semantic analysis + linter, plus the OWL
/// template refs and component descriptors [`js_arch_builder::visit_file`] extracts.
/// Vendored libs (`/static/lib/`) stop after the parse — see below.
/// Shared by the build thread ([`FileInfo::build_js_ast`]) and the pre-parse workers
/// ([`crate::core::pre_parser`]).
///
/// Everything session-dependent is left to the caller — see [`FileInfo::apply_parsed_js`].
pub fn parse_js(contents: &str, path: &str) -> ParsedJs {
    // Recursive-descent parse/semantic can overflow the default stack on deeply-nested
    // ASTs (e.g. minified JS). Run on a dedicated thread with a known-large stack.
    // Callers already on such a stack (pre-parse workers) skip this and call parse_js_inner.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(JS_PARSE_STACK_SIZE)
            .spawn_scoped(scope, || parse_js_inner(contents, path))
            .expect("failed to spawn JS parsing thread")
            .join()
            .unwrap_or_default()
    })
}

/// The body of [`parse_js`], without the stack-headroom thread. Only call from a
/// thread that already has at least [`JS_PARSE_STACK_SIZE`] of stack.
pub fn parse_js_inner(contents: &str, path: &str) -> ParsedJs {
    let os_path = std::path::Path::new(path);
    let source_type = SourceType::from_path(os_path).unwrap_or_default();
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, contents, source_type).parse();
    let mut diags: Vec<OxcDiagnostic> = ret.errors;
    let parser_module_record = ret.module_record;

    let (imports, reexports, exports) = FileInfo::collect_js_imports(&parser_module_record);

    let program = allocator.alloc(ret.program);

    // Collect template references, component descriptors and declarations before
    // semantic analysis
    let (template_refs, component_descriptors, decls) = js_arch_builder::visit_file(program, path, &exports);
    // Vendored libraries are kept out of workspace symbols for the same reason they
    // are kept out of OXC diagnostics: they are not the user's code, and many are minified.
    let is_lib = path.contains("/static/lib/");
    let decls = if is_lib { vec![] } else { decls };

    // Semantic analysis and the linter exist only to produce diagnostics, and
    // a vendored lib's are dropped. They are also the biggest sources we parse,
    // so stop here for them.
    if is_lib {
        return ParsedJs { template_refs, component_descriptors, decls, imports, reexports, diagnostics: vec![] };
    }

    let semantic_ret = SemanticBuilder::new()
        .with_cfg(true)
        .with_check_syntax_error(true)
        .build(program);
    diags.extend(semantic_ret.errors);
    let semantic = semantic_ret.semantic;

    // Build the linter module record and context
    let module_record = Arc::new(ModuleRecord::new(os_path, &parser_module_record, &semantic));
    let context_sub_host = ContextSubHost::new(semantic, module_record, 0);

    // Create a linter with default (correctness) rules and run it
    let mut external_plugin_store = ExternalPluginStore::default();
    let config = ConfigStoreBuilder::default()
        .build(&mut external_plugin_store)
        .expect("failed to build linter config");
    let config_store = ConfigStore::new(
        config,
        HashMap::default(),
        external_plugin_store,
    );
    let linter = oxc_linter::Linter::new(LintOptions::default(), config_store, None);
    let messages = linter.run(os_path, vec![context_sub_host], &allocator);
    diags.extend(messages.into_iter().map(|m| m.error));

    let uri = FileMgr::pathname2uri(path);
    let diagnostics = diags.iter().flat_map(
        |d| js_utils::oxc_diagnostic_to_lsp_diagnostic(d, &uri)
    ).collect();
    ParsedJs { template_refs, component_descriptors, decls, imports, reexports, diagnostics }
}

#[derive(Debug, Clone)]
pub struct PythonAst {
    pub indexed_module: Option<Arc<IndexedModule>>,
}

impl Default for PythonAst {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonAst {
    pub fn new() -> Self {
        Self {
            indexed_module: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct JsAst {
    /// Positions of OWL `static template = "some.xml_id"` string literals found in this JS file.
    /// Each entry is (byte range of the string content, xml_id value, enclosing class name).
    /// The range is converted to LSP coordinates by consumers.
    pub js_template_refs: Vec<JsTemplateRef>,
    /// Component descriptors extracted from OXC analysis of this JS file.
    pub js_component_descriptors: Vec<ComponentDescriptor>,
    /// Named declarations of this JS file, for workspace symbols.
    pub js_decls: Vec<JsDeclaration>,
    /// Every module specifier this JS file imports from, verbatim as written (incl.
    /// bare `import "x"` and `export … from`). Sorted and deduplicated.
    pub js_imports: Vec<String>,
    /// The subset of [`Self::js_imports`] reached through a re-export — tracked apart as
    /// one of the two type-propagating edges of `core::js_import_graph`.
    pub js_reexports: Vec<String>,
}

impl Default for JsAst {
    fn default() -> Self {
        Self::new()
    }
}

impl JsAst {
    pub fn new() -> Self {
        Self {
            js_template_refs: Vec::new(),
            js_component_descriptors: Vec::new(),
            js_decls: Vec::new(),
            js_imports: Vec::new(),
            js_reexports: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Ast {
    PythonAst(PythonAst),
    XmlAst,
    CsvAst,
    JsAst(JsAst),
}

impl Ast {
    pub fn as_py_ast(&self) -> &PythonAst {
        match self {
            Ast::PythonAst(py_ast) => py_ast,
            _ => panic!("Expected PythonAst, found {:?}", self),
        }
    }
    pub fn as_py_ast_mut(&mut self) -> &mut PythonAst {
        match self {
            Ast::PythonAst(py_ast) => py_ast,
            _ => panic!("Expected PythonAst, found {:?}", self),
        }
    }
    pub fn as_js_ast(&self) -> &JsAst {
        match self {
            Ast::JsAst(js_ast) => js_ast,
            _ => panic!("Expected JsAst, found {:?}", self),
        }
    }
    pub fn as_js_ast_mut(&mut self) -> &mut JsAst {
        match self {
            Ast::JsAst(js_ast) => js_ast,
            _ => panic!("Expected JsAst, found {:?}", self),
        }
    }
    pub fn is_built(&self) -> bool {
        match self {
            Ast::PythonAst(py_ast) => py_ast.indexed_module.is_some(),
            Ast::JsAst(js_ast) => !js_ast.js_template_refs.is_empty() || !js_ast.js_component_descriptors.is_empty(),
            Ast::XmlAst | Ast::CsvAst => true,
        }
    }
}

/* Structure that hold ast and text_document for FileInfo. It allows Fileinfo to hold it with a Rc<RefCell<>> to allow mutability and build on-the-fly
 */
#[derive(Debug)]
pub struct FileInfoAst {
    pub text_hash: u64,
    pub text_document: Option<TextDocument>,
    pub ast: Ast,
}

impl FileInfoAst {
    pub fn get_stmts(&self) -> Option<&[Stmt]> {
        match &self.ast {
            Ast::PythonAst(python_ast) => {
                python_ast.indexed_module.as_ref().map(|module| module.parsed.syntax().body.as_slice())
            },
            _ => None
        }
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
    diagnostics: HashMap<DiagnosticSource, Vec<Diagnostic>>,
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
                ast: Ast::PythonAst(PythonAst::new()),
            })),
            diagnostics: HashMap::default(),
            noqas_blocs: HashMap::default(),
            noqas_lines: HashMap::default(),
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
            // A pre-parser worker thread may have already read (and for Python
            // also parsed) this file ahead of the build. If so, slot the
            // prepared payload straight in — no disk read. See
            // `crate::core::pre_parser`.
            let sanitized_path = Path::new(path).sanitize_cow();
            if let Some(preloaded) = SyncOdoo::take_preloaded(session, &sanitized_path) {
                // no need to gate on hash change: pre-parser only runs on first build
                self.apply_preloaded(session, preloaded);
                return true;
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
        let old_hash = self.file_info_ast.borrow().text_hash;
        let new_hash = hash_text_document(self.file_info_ast.borrow().text_document.as_ref().unwrap());
        if old_hash == new_hash {
            return false;
        }
        self.file_info_ast.borrow_mut().text_hash = new_hash;
        self.diagnostics.clear();
        self._build_ast(session, is_external);
        true
    }

    pub fn _build_ast(&mut self, session: &mut SessionInfo, is_external: bool) {
        match Path::new(&self.uri).extension().and_then(|s| s.to_str()) {
            Some("xml") => {
                self.file_info_ast.borrow_mut().ast = Ast::XmlAst;
            }
            Some("csv") => {
                self.file_info_ast.borrow_mut().ast = Ast::CsvAst;
            }
            Some("js") | Some("ts") => {
                self.file_info_ast.borrow_mut().ast = Ast::JsAst(JsAst::new());
                self.build_js_ast(session, is_external);
            }
            _ => {
                self.build_python_ast(session, is_external);
            }
        }
    }

    fn build_python_ast(&mut self, session: &mut SessionInfo, is_external: bool) {
        let source_type = python_source_type(&self.uri);
        let parsed = {
            let fia = self.file_info_ast.borrow();
            parse_python(fia.text_document.as_ref().unwrap(), source_type, session.sync_odoo.encoding, session.sync_odoo.test_mode, is_external)
        };
        if !is_external {
            self.noqas_blocs = parsed.noqas_blocs;
            self.noqas_lines = parsed.noqas_lines;
            self.diag_test_comments.extend(parsed.diag_test_comments);
        }
        let (valid, diagnostics) = Self::syntax_diagnostics(session, &parsed.indexed_module.parsed);
        self.valid = valid;
        self.file_info_ast.borrow_mut().ast.as_py_ast_mut().indexed_module = Some(parsed.indexed_module);
        self.replace_diagnostics(DiagnosticSource::PY_SYNTAX, diagnostics);
    }

    /// Build the SYNTAX-step diagnostics (OLS01000) for a parsed Python module.
    /// Returns whether the module is syntactically valid along with the diagnostics.
    fn syntax_diagnostics(session: &SessionInfo, parsed: &Parsed<ModModule>) -> (bool, Vec<Diagnostic>) {
        let mut diagnostics = vec![];
        let mut valid = true;
        for error in parsed.errors().iter() {
            valid = false;
            if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS01000, &[]) {
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
        (valid, diagnostics)
    }

    /// Slot a [`PreloadedFile`] payload produced by a background [`crate::core::pre_parser`]
    /// worker into this `FileInfo`, instead of reading (and, for Python, parsing)
    /// the file inline. For Python this mirrors the Python branch of
    /// [`Self::_build_ast`] — syntax diagnostics are built here because they
    /// depend on the session's diagnostic config.
    fn apply_preloaded(&mut self, session: &mut SessionInfo, preloaded: PreloadedFile) {
        match preloaded {
            PreloadedFile::Python { text_hash, text_document, parsed } => {
                let (valid, diagnostics) = Self::syntax_diagnostics(session, &parsed.indexed_module.parsed);
                self.valid = valid;
                self.noqas_blocs = parsed.noqas_blocs;
                self.noqas_lines = parsed.noqas_lines;
                self.diag_test_comments = parsed.diag_test_comments;
                {
                    let mut fia = self.file_info_ast.borrow_mut();
                    fia.text_hash = text_hash;
                    fia.text_document = Some(text_document);
                    fia.ast = Ast::PythonAst(PythonAst { indexed_module: Some(parsed.indexed_module) });
                }
                self.replace_diagnostics(DiagnosticSource::PY_SYNTAX, diagnostics);
            }
            PreloadedFile::DataFile { text_hash, text_document, ast } => {
                let mut fia = self.file_info_ast.borrow_mut();
                fia.text_hash = text_hash;
                fia.text_document = Some(text_document);
                fia.ast = ast;
            }
            PreloadedFile::Js { text_hash, text_document, parsed } => {
                {
                    let mut fia = self.file_info_ast.borrow_mut();
                    fia.text_hash = text_hash;
                    fia.text_document = Some(text_document);
                    fia.ast = Ast::JsAst(JsAst::new());
                }
                self.apply_parsed_js(session, parsed);
            }
        }
    }

    fn build_js_ast(&mut self, session: &mut SessionInfo, _is_external: bool) {
        let parsed = {
            let fia = self.file_info_ast.borrow();
            parse_js(fia.text_document.as_ref().unwrap().contents(), &self.uri)
        };
        self.apply_parsed_js(session, parsed);
    }

    /// Store a [`ParsedJs`] on this `FileInfo` and feed the OWL maps with it. This is
    /// the session-dependent half of a JS build: it runs on the build thread whether
    /// the file was parsed inline ([`Self::build_js_ast`]) or by a pre-parse worker
    /// ([`Self::apply_preloaded`]), so `js_arch_builder::build` keeps seeing files in
    /// build order either way.
    ///
    /// Expects [`Ast::JsAst`] to be in place already.
    fn apply_parsed_js(&mut self, session: &mut SessionInfo, parsed: ParsedJs) {
        js_arch_builder::build(session, &parsed.template_refs, &parsed.component_descriptors);
        {
            let mut fia = self.file_info_ast.borrow_mut();
            let js_ast = fia.ast.as_js_ast_mut();
            js_ast.js_template_refs = parsed.template_refs;
            js_ast.js_component_descriptors = parsed.component_descriptors;
            js_ast.js_decls = parsed.decls;
            js_ast.js_imports = parsed.imports;
            js_ast.js_reexports = parsed.reexports;
        }
        self.replace_diagnostics(DiagnosticSource::JS_OXC, parsed.diagnostics); //OXC will use SYNTAX. others are reserved to tsserver
    }

    fn collect_js_imports(parser_module_record: &oxc::syntax::module_record::ModuleRecord) -> (Vec<String>, Vec<String>, HashMap<String, JsExportKind>) {
        let mut imports: Vec<String> = parser_module_record.requested_modules
            .keys()
            .map(|spec| spec.as_str().to_string())
            .collect();
        imports.sort();
        let mut reexports: Vec<String> = parser_module_record.indirect_export_entries
            .iter()
            .chain(parser_module_record.star_export_entries.iter())
            .filter_map(|entry| entry.module_request.as_ref())
            .map(|request| request.name.as_str().to_string())
            .collect();
        reexports.sort();
        reexports.dedup();

        // Local class name → how the module exports it. A renamed export
        // (`export { Foo as Bar }`) stays `None` so it falls back to a shim.
        let mut exports: HashMap<String, JsExportKind> = HashMap::default();
        for entry in parser_module_record.local_export_entries.iter() {
            let Some(local) = entry.local_name.name() else { continue };
            let kind = match &entry.export_name {
                ExportExportName::Default(_) => JsExportKind::Default,
                ExportExportName::Name(n) if n.name.as_str() == local.as_str() => {
                    JsExportKind::Named
                }
                _ => continue,
            };
            exports.insert(local.as_str().to_string(), kind);
        }
        (imports, reexports, exports)
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
        {
            let mut fia = self.file_info_ast.borrow_mut();
            fia.text_hash = hash_text_document(fia.text_document.as_ref().unwrap());
        }
        self._build_ast(session, session.sync_odoo.get_file_mgr().borrow().is_in_workspace(&self.uri));
    }

    /// Scan a parsed module's comment tokens for `noqa` directives and (in test mode)
    /// `# OLS` test-expectation comments.
    fn scan_noqa(
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
        let mut blocs: HashMap<u32, NoqaInfo> = HashMap::default();
        let mut lines: HashMap<u32, NoqaInfo> = HashMap::default();
        let mut test_comments: Vec<(u32, Vec<String>)> = Vec::new();
        let mut is_first_expr: bool = true;
        let mut noqa_to_add = None;
        let mut previous_token: Option<&Token> = None;
        for token in parsed_module.tokens().iter() {
            match token.kind() {
                TokenKind::Comment => {
                    let text = &source[token.range()];
                    if text.starts_with("#noqa") || text.starts_with("# noqa") || text.starts_with("# odools: noqa") {
                        let after_noqa = text.split("noqa").nth(1);
                        if let Some(after_noqa) = after_noqa {
                            let mut codes = vec![];
                            for code in after_noqa.split(|c: char| c == ',' || c.is_whitespace() || c == ':') {
                                let code = code.trim();
                                if !code.is_empty() {
                                    codes.push(code.to_string());
                                }
                            }
                            if !codes.is_empty() {
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
                    if parse_test_comments
                        && (text.starts_with("#OLS") || text.starts_with("# OLS")) {
                            let codes = text.split(",").map(|s| s.trim().trim_start_matches('#').trim().to_string()).collect::<Vec<String>>();
                            let source_location = text_document.index().source_location(token.start(), text_document.contents(), encoding);
                            test_comments.push((source_location.line.to_zero_indexed() as u32, codes));
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

    pub fn replace_diagnostics(&mut self, step: DiagnosticSource, diagnostics: Vec<Diagnostic>) {
        self.need_push = true;
        self.diagnostics.insert(step, diagnostics);
    }

    pub fn update_validation_diagnostics(&mut self, diagnostics: HashMap<DiagnosticSource, Vec<Diagnostic>>) {
        self.need_push = true;
        for (key, value) in diagnostics {
            self.diagnostics.entry(key).or_default().extend(value);
        }
    }

    fn update_range(&self, mut diagnostic: Diagnostic, encoding: PositionEncoding) -> Diagnostic {
        diagnostic.range.start = self.offset_to_position(diagnostic.range.start.line, encoding);
        diagnostic.range.end = self.offset_to_position(diagnostic.range.end.line, encoding);
        if let Some(ref mut related_information) = diagnostic.related_information {
            for related in related_information.iter_mut() {
                related.location.range.start = self.offset_to_position(related.location.range.start.line, encoding);
                related.location.range.end = self.offset_to_position(related.location.range.end.line, encoding);
            }
        }
        diagnostic
    }
    pub fn update_diagnostic_filters(&mut self, session: &SessionInfo) {
        self.diagnostic_filters = session.sync_odoo.config.diagnostic_filters().iter().filter(|filter| {
            match filter.path_type {
                DiagnosticFilterPathType::In => {
                    filter.paths.iter().any(|p| p.matches(&self.uri))
                }
                DiagnosticFilterPathType::NotIn => {
                    !filter.paths.iter().any(|p| p.matches(&self.uri))
                }
            }
        }).cloned().collect::<Vec<_>>();
    }

    pub fn publish_diagnostics(&mut self, session: &mut SessionInfo) {
        if self.need_push {
            let mut all_diagnostics = Vec::new();

            let is_js = matches!(self.file_info_ast.borrow().ast, Ast::JsAst(_));
            //We are checking ARCH as it contains Syntax diagnostics for tsserver
            let syntax_diags = self.diagnostics.get(&DiagnosticSource::JS_OXC);
            let has_syntax_diags = is_js && syntax_diags.map(|v| !v.is_empty()).unwrap_or(false);
            let diag_iter: Box<dyn Iterator<Item = &Diagnostic>> = if has_syntax_diags {
                // If there is syntax diagnostics, we only send the ones from OXC, to be less noisy
                Box::new(self.diagnostics.get(&DiagnosticSource::JS_OXC).unwrap().iter())
            } else {
                Box::new(self.diagnostics.values().flatten())
            };

            'diagnostics: for d in diag_iter {
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
                                    if codes.contains(s) {
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
                        if !filter.types.iter().any(|t| {
                            matches!(
                                (t, severity),
                                (DiagnosticSetting::Error, &DiagnosticSeverity::ERROR)
                                    | (DiagnosticSetting::Warning, &DiagnosticSeverity::WARNING)
                                    | (DiagnosticSetting::Info, &DiagnosticSeverity::INFORMATION)
                                    | (DiagnosticSetting::Hint, &DiagnosticSeverity::HINT)
                            )
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
        offset_to_position_with_line_index(text_document.index(), text_document.contents(), offset as usize, encoding)
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

    pub fn text_range_to_range(&self, range: TextRange, encoding: PositionEncoding) -> Range {
        Range {
            start: self.offset_to_position(range.start().to_usize() as u32, encoding),
            end: self.offset_to_position(range.end().to_usize() as u32, encoding)
        }
    }

    pub fn try_text_range_to_range(&self, range: TextRange, encoding: PositionEncoding) -> Option<Range> {
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
        position_to_offset_with_line_index(text_document.index(), text_document.contents(), line, char, encoding)
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

impl Default for FileMgr {
    fn default() -> Self {
        Self::new()
    }
}

impl FileMgr {

    pub fn new() -> Self {
        Self {
            files: HashMap::default(),
            untitled_files: HashMap::default(),
            workspace_folders: HashSet::default(),
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

    pub fn text_range_to_range(&self, session: &mut SessionInfo, path: &str, range: TextRange) -> Range {
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
        let matching_keys: Vec<String> = session.sync_odoo.get_file_mgr().borrow_mut().files.keys().filter(|&k| Path::new(k).starts_with(uri)).cloned().collect();
        for key in matching_keys {
            Self::delete_entry(session, &key, uri);
        }
    }

    /// Unlike `delete_path`, this function only deletes the file with the exact uri, and not files in subdirectories.
    pub fn delete_file_path(session: &mut SessionInfo, uri: &str) {
        Self::delete_entry(session, uri, uri);
    }

    /// Helper for delete_path and delete_file_path
    fn delete_entry(session: &mut SessionInfo, key: &str, uri: &str) {
        let to_del = session.sync_odoo.get_file_mgr().borrow_mut().files.remove(key);
        if let Some(to_del) = to_del
            && SyncOdoo::is_in_workspace_or_entry(session, uri) {
                let mut to_del = (*to_del).borrow_mut();
                to_del.diagnostics.clear();
                to_del.publish_diagnostics(session)
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
                if file.borrow().uri == entry.path {
                    found = true;
                    break;
                }
            }
            if !found {
                continue;
            }
            let mut to_del = file.borrow_mut();
            to_del.diagnostics.clear();
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
        let mut visited_names= HashSet::default();
        let mut unique_folders = HashMap::default();
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

    pub fn try_pathname2uri(s: &str) -> Result<lsp_types::Uri, String> {
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
                Err(err) => return Err(err.to_string())
            }
        };
        lsp_types::Uri::from_str(&pre_uri).map_err(|err| err.to_string())
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

/// A file pre-loaded by a background [`crate::core::pre_parser`] worker thread,
/// ready to be slotted into a [`FileInfo`] by the build thread without re-reading
/// or re-parsing it from disk. See [`FileInfo::apply_preloaded`].
#[derive(Debug)]
pub enum PreloadedFile {
    /// A fully-parsed Python source: the worker ran read + ruff parse + noqa scan
    /// off the build thread.
    /// The build only rebuilds syntax diagnostics (they depend on session config).
    Python {
        text_hash: u64,
        text_document: TextDocument,
        parsed: ParsedPython,
    },
    /// A pre-read XML/CSV data file (one of the paths listed in the manifest
    /// `data` list). Workers only run [`fs::read_to_string`] — no parsing; the
    /// build thread slots the contents straight in without touching the disk.
    DataFile {
        text_hash: u64,
        text_document: TextDocument,
        ast: Ast,
    },
    /// A fully-parsed JS asset: the worker ran read + OXC parse + semantic + lint off
    /// the build thread. The build thread only feeds the OWL maps with the result,
    /// which it must do in build order.
    Js {
        text_hash: u64,
        text_document: TextDocument,
        parsed: ParsedJs,
    },
}

pub fn hash_text_document(text: &TextDocument) -> u64 {
    let mut hasher = FxHasher::default();
    text.hash(&mut hasher);
    hasher.finish()
}

pub fn offset_to_position_with_line_index(
    index: &LineIndex,
    text: &str,
    offset: usize,
    encoding: PositionEncoding
) -> Position {
    // clamp offset to text length
    let offset = u32::try_from(offset.min(text.len())).expect("offset fits in u32");
    let location = index.source_location(offset.into(), text, encoding);
    let line = u32::try_from(location.line.to_zero_indexed()).expect("row usize fits in u32");
    let character = u32::try_from(location.character_offset.to_zero_indexed())
        .expect("character usize fits in u32");
    Position::new(line, character)
}

pub fn position_to_offset_with_line_index(
    index: &LineIndex,
    text: &str,
    line: u32,
    character: u32,
    encoding: PositionEncoding,
) -> usize {
    let position = SourceLocation {
        line: OneIndexed::from_zero_indexed(line as usize),
        character_offset: OneIndexed::from_zero_indexed(character as usize),
    };
    index.offset(position, text, encoding).into()
}
