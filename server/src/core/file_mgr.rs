use ropey::Rope;
use ruff_python_ast::Mod;
use ruff_python_parser::Mode;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, TextDocumentContentChangeEvent};
use std::{collections::HashMap, fs};
use crate::core::odoo::SyncOdoo;
use crate::core::messages::{Msg, MsgDiagnostic};
use std::rc::Rc;
use std::cell::RefCell;
use crate::S;
use crate::constants::*;
use ruff_text_size::TextRange;

#[derive(Debug)]
pub struct FileInfo {
    pub ast: Option<Vec<ruff_python_ast::Stmt>>,
    pub version: i32,
    pub uri: String,
    need_push: bool,
    text_rope: Option<ropey::Rope>,
    diagnostics: HashMap<BuildSteps, Vec<Diagnostic>>,
}

impl FileInfo {
    fn new(uri: String) -> Self {
        Self {
            ast: None,
            version: 0,
            uri,
            need_push: false,
            text_rope: None,
            diagnostics: HashMap::new(),
        }
    }
    pub fn update(&mut self, odoo: &SyncOdoo, uri: &str, content: Option<&Vec<TextDocumentContentChangeEvent>>, version: Option<i32>, force: bool) {
        // update the file info with the given information.
        // uri: indicates the path of the file
        // content: if content is given, it will be used to update the ast and text_rope, if not, the loading will be from the disk
        // version: if the version is provided, the file_info wil be updated only if the new version is higher.
        // -100 can be given as version number to indicates that the file has not been opened yet, and that we have to load it ourself
        // See https://github.com/Microsoft/language-server-protocol/issues/177
        // Return true if the update has been done and not discarded
        if let Some(version) = version {
            if version == -100 {
                self.version = 1;
            } else {
                if version <= self.version && !force {
                    return;
                }
                self.version = version;
            }
        } else if self.version != 0 && !force {
            return;
        }
        self.diagnostics.clear();
        if let Some(content) = content {
            for change in content.iter() {
                self.apply_change(change);
            }
            self._build_ast();
        } else {
            match fs::read_to_string(uri) {
                Ok(content) => {
                    self.text_rope = Some(ropey::Rope::from(content.as_str()));
                    self._build_ast()
                },
                Err(_) => odoo.msg_sender.send(Msg::LOG_ERROR(format!("Failed to read file {}", uri))),
            };
        }
    }
//"/home/odoo/Documents/odoo-servers/test_odoo/odoo/odoo/addons/base/__manifest__.py"
    pub fn _build_ast(&mut self) {
        //let ast = ast::Suite::parse(&content, content_path);
        let mut diagnostics = vec![];
        let content = &self.text_rope.as_ref().unwrap().slice(..);
        let source = content.to_string(); //cast to string to get a version with all changes
        let ast = ruff_python_parser::parse(source.as_str(), Mode::Module);
        match ast {
            Ok(module) => {
                match module {
                    Mod::Expression(_expr) => {
                        println!("[Warning] No support for expression-file only");
                        self.ast = None
                    },
                    Mod::Module(module) => {
                        self.ast = Some(module.body);
                    }
                }
            },
            Err(err) => {
                self.ast = None;
                diagnostics.push(Diagnostic::new(
                    Range{ start: Position::new(err.location.start().to_u32(), 0),
                        end: Position::new(err.location.end().to_u32(), 0)},
                    Some(DiagnosticSeverity::ERROR),
                    Some(NumberOrString::String(S!("OLS30001"))),
                    None,
                    err.error.to_string(),
                    None,
                    None));
            }
        };
        self.replace_diagnostics(BuildSteps::SYNTAX, diagnostics);
    }

    pub fn replace_diagnostics(&mut self, step: BuildSteps, diagnostics: Vec<Diagnostic>) {
        self.need_push = true;
        self.diagnostics.insert(step, diagnostics);
    }

    fn update_range(&self, mut diagnostic: Diagnostic) -> Diagnostic {
        diagnostic.range.start = self.offset_to_position(diagnostic.range.start.line as usize);
        diagnostic.range.end = self.offset_to_position(diagnostic.range.end.line as usize);
        diagnostic
    }

    pub fn publish_diagnostics(&mut self, odoo: &SyncOdoo) {
        if self.need_push {
            let mut all_diagnostics = Vec::new();

            for diagnostics in self.diagnostics.values() {
                for d in diagnostics.iter() {
                    all_diagnostics.push(self.update_range(d.clone()));
                }
            }
            let _ = odoo.msg_sender.send(Msg::DIAGNOSTIC(MsgDiagnostic{
                uri: url::Url::parse(&format!("file://{}", self.uri)).expect("Failed to parse manifest uri"),
                diags: all_diagnostics,
                version: Some(self.version),
            }));
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
        let rope = self.text_rope.as_ref().expect("no rope provided");
        FileInfo::offset_to_position_with_rope(rope, offset)
    }

    pub fn position_to_offset_with_rope(rope: &Rope, line: u32, char: u32) -> usize {
        let line_char = rope.try_line_to_char(line as usize).expect("unable to get char from line");
        rope.try_char_to_byte(line_char + char as usize).expect("unable to get byte from char")
    }

    pub fn position_to_offset(&self, line: u32, char: u32) -> usize {
        let rope = self.text_rope.as_ref().expect("no rope provided");
        FileInfo::position_to_offset_with_rope(rope, line, char)
    }

    fn apply_change(&mut self, change: &TextDocumentContentChangeEvent) {
        //TODO a desync can occur if updates come while the initialization (did_open missing?)
        if change.range.is_none() {
            self.text_rope = Some(ropey::Rope::from_str(&change.text));
            return;
        }
        let start_idx = self.text_rope.as_ref().unwrap().try_line_to_char(change.range.unwrap().start.line as usize).expect("Unable to get char position of line");
        let start_idx = start_idx + change.range.unwrap().start.character as usize;
        let end_idx = self.text_rope.as_ref().unwrap().try_line_to_char(change.range.unwrap().end.line as usize).expect("Unable to get char position of line");
        let end_idx = end_idx + change.range.unwrap().end.character as usize;
        self.text_rope.as_mut().unwrap().remove(start_idx .. end_idx);
        self.text_rope.as_mut().unwrap().insert(start_idx, &change.text);
    }
}

#[derive(Debug)]
pub struct FileMgr {
    pub files: HashMap<String, Rc<RefCell<FileInfo>>>,
    workspace_folder: Vec<String>,
}

impl FileMgr {

    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            workspace_folder: vec![],
        }
    }

    pub fn textRange_to_temporary_Range(range: &TextRange) -> Range {
        Range::new(
            Position::new(range.start().to_u32(), 0),
            Position::new(range.end().to_u32(), 0))
    }

    pub fn get_file_info(&self, path: &String) -> Option<Rc<RefCell<FileInfo>>> {
        match self.files.get(path) {
            Some(rc) => Some(rc.clone()),
            None => None
        }
    }

    pub fn text_range_to_range(&mut self, odoo: &mut SyncOdoo, path: &String, range: &TextRange) -> Range {
        let file = self.files.get(path);
        if let Some(file) = file {
            return Range {
                start: file.borrow().offset_to_position(range.start().to_usize()),
                end: file.borrow().offset_to_position(range.end().to_usize())
            }
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
            Err(_) => odoo.msg_sender.send(Msg::LOG_ERROR(format!("Failed to read file {}", path)))
        };
        Range::default()
    }

    pub fn update_file_info(&mut self, sync_odoo: &mut SyncOdoo, uri: &str, content: Option<&Vec<TextDocumentContentChangeEvent>>, version: Option<i32>, force: bool) -> Rc<RefCell<FileInfo>> {
        let file_info = self.files.entry(uri.to_string()).or_insert_with(|| Rc::new(RefCell::new(FileInfo::new(uri.to_string()))));
        let return_info = file_info.clone();
        let mut file_info_mut = (*return_info).borrow_mut();
        file_info_mut.update(sync_odoo, uri, content, version, force);
        drop(file_info_mut);
        return_info
    }

    pub fn delete_path(&mut self, odoo: &SyncOdoo, uri: &String) {
        let to_del = self.files.remove(uri);
        if let Some(to_del) = to_del {
            if self.is_in_workspace(uri) {
                let mut to_del = (*to_del).borrow_mut();
                to_del.replace_diagnostics(BuildSteps::SYNTAX, vec![]);
                to_del.replace_diagnostics(BuildSteps::ARCH, vec![]);
                to_del.replace_diagnostics(BuildSteps::ARCH_EVAL, vec![]);
                to_del.replace_diagnostics(BuildSteps::ODOO, vec![]);
                to_del.replace_diagnostics(BuildSteps::VALIDATION, vec![]);
                to_del.publish_diagnostics(odoo)
            }
        }
    }

    pub fn add_workspace_folder(&mut self, path: String) {
        if !self.workspace_folder.contains(&path) {
            self.workspace_folder.push(path);
        }
    }

    pub fn remove_workspace_folder(&mut self, path: String) {
        let index = self.workspace_folder.iter().position(|x| *x == path).unwrap();
        self.workspace_folder.swap_remove(index);
    }

    pub fn is_in_workspace(&self, path: &str) -> bool {
        for p in self.workspace_folder.iter() {
            if path.starts_with(p) {
                return true;
            }
        }
        false
    }

    pub fn pathname2uri(s: &String) -> url::Url {
        if let Ok(url) = url::Url::from_file_path(s) {
            return url;
        }
        panic!("unable to transform pathname to uri: {s}")
    }

    pub fn uri2pathname(s: &str) -> String {
        if let Ok(url) = url::Url::parse(s) {
            if let Ok(url) = url.to_file_path() {
                return S!(url.to_str().unwrap());
            }
        }
        println!("Unable to extract path from uri: {s}");
        S!(s)
    }
}
