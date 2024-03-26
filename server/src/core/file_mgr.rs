use rustpython_parser::{Parse, ast, text_size::TextRange};
use tower_lsp::lsp_types::{Diagnostic, Position, Range};
use std::{borrow::BorrowMut, collections::HashMap, fs};
use crate::core::odoo::SyncOdoo;
use crate::core::messages::{Msg, MsgDiagnostic};
use anyhow::Result;
use url::Url;
use std::rc::Rc;
use std::cell::RefCell;
use crate::constants::*;

#[derive(Debug)]
pub struct FileInfo {
    pub ast: Option<Vec<ast::Stmt>>,
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
            diagnostics: HashMap::with_capacity(5),
        }
    }
    pub fn update(&mut self, odoo: &SyncOdoo, uri: &str, content: Option<String>, version: Option<i32>) {
        // update the file info with the given information.
        // uri: indicates the path of the file
        // content: if content is given, it will be used to update the ast and text_rope, if not, the loading will be from the disk
        // version: if the version is provided, the file_info wil be updated only if the new version is higher.
        // -100 can be given as version number to indicates that the file has not been opened yet, and that we have to load it ourself
        // See https://github.com/Microsoft/language-server-protocol/issues/177
        if let Some(version) = version {
            if version == -100 {
                self.version = 1;
            } else {
                if version <= self.version {
                    return;
                }
                self.version = version;
            }
        }
        if let Some(content) = content {
            self._build_ast(&content);
        } else {
            match fs::read_to_string(uri) {
                Ok(content) => self._build_ast(&content),
                Err(_) => odoo.msg_sender.blocking_send(Msg::LOG_ERROR(format!("Failed to read file {}", uri))).expect("error sending log message"),
            };
        }
    }

    pub fn _build_ast(&mut self, content: &str) {
        self.ast = Some(ast::Suite::parse_without_path(&content).unwrap()); //TODO handle errors
        self.text_rope = Some(ropey::Rope::from(content));
        self.replace_diagnostics(BuildSteps::SYNTAX, vec![]);
    }

    pub fn replace_diagnostics(&mut self, step: BuildSteps, diagnostics: Vec<Diagnostic>) {
        self.need_push = true;
        self.diagnostics.insert(step, diagnostics);
    }

    pub fn publish_diagnostics(&mut self, odoo: &mut SyncOdoo) {
        if self.need_push {
            let mut all_diagnostics = Vec::new();

            for diagnostics in self.diagnostics.values() {
                all_diagnostics.extend(diagnostics.clone());
            }
            let _ = odoo.msg_sender.blocking_send(Msg::DIAGNOSTIC(MsgDiagnostic{
                uri: url::Url::parse(&format!("file://{}", self.uri)).expect("Failed to parse manifest uri"),
                diags: all_diagnostics,
                version: Some(self.version),
            }));
            self.need_push = false;
        }
    }

    pub fn byte_position_to_position(&self, offset: usize) -> Option<Position> {
        let rope = self.text_rope.as_ref()?;
        let line = rope.try_char_to_line(offset).ok()?;
        let first_char_of_line = rope.try_line_to_char(line).ok()?;
        let column = offset - first_char_of_line;
        Some(Position::new(line as u32, column as u32))
    }

    pub fn text_range_to_range(&self, range: &TextRange) -> Option<Range> {
        let start = self.byte_position_to_position(range.start().to_usize())?;
        let end = self.byte_position_to_position(range.end().to_usize())?;
        Some(Range::new(start, end))
    }
}

#[derive(Debug)]
pub struct FileMgr {
    pub files: HashMap<String, Rc<RefCell<FileInfo>>>
}

impl FileMgr {

    pub fn new() -> Self {
        Self {
            files: HashMap::new()
        }
    }

    pub fn get_file_info(&mut self, syncOdoo: &mut SyncOdoo, uri: &str, content: Option<String>, version: Option<i32>) -> Rc<RefCell<FileInfo>> {
        let file_info = self.files.entry(uri.to_string()).or_insert_with(|| Rc::new(RefCell::new(FileInfo::new(uri.to_string()))));
        let return_info = file_info.clone();
        let mut file_info_mut = (*return_info).borrow_mut();
        file_info_mut.update(syncOdoo, uri, content, version);
        drop(file_info_mut);
        return_info
    }

    pub fn delete_path(&mut self, odoo: &mut SyncOdoo, uri: String) {
        let to_del = self.files.remove(&uri);
        if let Some(to_del) = to_del {
            let mut to_del = (*to_del).borrow_mut();
            to_del.replace_diagnostics(BuildSteps::SYNTAX, vec![]);
            to_del.replace_diagnostics(BuildSteps::ARCH, vec![]);
            to_del.replace_diagnostics(BuildSteps::ARCH_EVAL, vec![]);
            to_del.replace_diagnostics(BuildSteps::ODOO, vec![]);
            to_del.replace_diagnostics(BuildSteps::VALIDATION, vec![]);
            to_del.publish_diagnostics(odoo)
        }
    }

    // fn pathname2uri(s: &str) -> String {
    //     let mut path = s.replace("\\", "/");
    //     path = percent_encode(path.as_bytes(), PATH_SEGMENT_ENCODE_SET).to_string();
    //     if cfg!(target_os = "windows") {
    //         path = format!("file:///{}", path);
    //     } else {
    //         path = format!("file://{}", path);
    //     }
    //     path
    // }
}
