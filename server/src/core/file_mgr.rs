use rustpython_parser::{Parse, ast};
use tower_lsp::lsp_types::Diagnostic;
use std::{collections::HashMap, fs};
use tower_lsp::Client;
use url::Url;
use crate::constants::*;

pub struct FileInfo {
    pub ast: Option<Vec<ast::Stmt>>,
    version: i32,
    pub uri: String,
    need_push: bool,
    diagnostics: HashMap<BuildSteps, Vec<Diagnostic>>,
}

impl FileInfo {
    fn new(uri: String) -> Self {
        Self {
            ast: None,
            version: 0,
            uri,
            need_push: false,
            diagnostics: HashMap::with_capacity(5),
        }
    }

    pub fn build_ast(&mut self, path: &str, content: &str) -> &mut Self{
        if ! content.is_empty() {
            self.ast = Some(ast::Suite::parse(content, "<embedded>").unwrap()) //TODO handle errors
        } else {
            let python_code = match fs::read_to_string(path) {
                Ok(content) => content,
                Err(_) => String::new(),
            };
            if ! python_code.is_empty() {
                self.ast = Some(ast::Suite::parse(&python_code, path).unwrap()) //TODO handle errors
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
}


pub struct FileMgr {
    pub files: HashMap<String, FileInfo>
}

impl FileMgr {

    pub fn new() -> Self {
        Self {
            files: HashMap::new()
        }
    }

    pub fn get_file_info(&mut self, uri: &str) -> &mut FileInfo {
        self.files.entry(uri.to_string()).or_insert_with(|| FileInfo::new(uri.to_string()))
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
