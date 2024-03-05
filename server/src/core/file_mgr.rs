use rustpython_parser::{Parse, ast, text_size::TextRange};
use tower_lsp::lsp_types::{Diagnostic, Position, Range};
use std::{borrow::BorrowMut, collections::HashMap, fs};
use tower_lsp::Client;
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

    pub fn build_ast(&mut self, path: &str, content: &str) -> &mut Self{
        if ! content.is_empty() {
            self.ast = Some(ast::Suite::parse(content, "<embedded>").unwrap()); //TODO handle errors
            self.text_rope = Some(ropey::Rope::from(content));
        } else {
            let python_code = match fs::read_to_string(path) {
                Ok(content) => content,
                Err(_) => String::new(),
            };
            if ! python_code.is_empty() {
                self.ast = Some(ast::Suite::parse(&python_code, path).unwrap()); //TODO handle errors
                self.text_rope = Some(ropey::Rope::from(python_code));
            }
        }
        self.replace_diagnostics(BuildSteps::SYNTAX, vec![]);
        //TODO handle valueError, permissionError
        self
    }

    fn replace_diagnostics(&mut self, step: BuildSteps, diagnostics: Vec<Diagnostic>) {
        self.diagnostics.insert(step, diagnostics);
    }

    async fn publish_diagnostics(&self, client: &Client) {
        let mut all_diagnostics = Vec::new();

        for diagnostics in self.diagnostics.values() {
            all_diagnostics.extend(diagnostics.clone());
        }
        client.publish_diagnostics( Url::parse(self.uri.as_str()).unwrap(), all_diagnostics, Some(self.version)).await;
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

    pub fn get_file_info(&mut self, uri: &str) -> Rc<RefCell<FileInfo>> {
        let file_info = self.files.entry(uri.to_string()).or_insert_with(|| Rc::new(RefCell::new(FileInfo::new(uri.to_string()))));
        let return_info = file_info.clone();
        let mut file_info_mut = (*return_info).borrow_mut();
        match file_info_mut.ast {
            Some(_) => {},
            None => {
                file_info_mut.build_ast(uri, "");
            }
        }
        drop(file_info_mut);
        return_info
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
