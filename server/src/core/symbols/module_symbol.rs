use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use rustpython_parser::ast::{Stmt};

use crate::core::file_mgr::FileInfo;
use crate::core::odoo::SyncOdoo;
use crate::constants::EXTENSION_NAME;
use std::path::PathBuf;


#[derive(Debug)]
pub struct ModuleSymbol {
    root_path: String,
    loaded: bool,
    module_name: String,
    dir_name: String,
    depends: Vec<String>,
    data: Vec<String>, // TODO
}

impl ModuleSymbol {

    pub fn new(odoo: &mut SyncOdoo, dir_path: &PathBuf) -> Option<Self> {
        let mut module = ModuleSymbol {
            root_path: dir_path.as_os_str().to_str().unwrap().to_string(),
            loaded: false,
            module_name: String::new(),
            dir_name: String::new(),
            depends: vec!("base".to_string()),
            data: Vec::new(),
        };
        module.dir_name = dir_path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string();
        let manifest_path = dir_path.join("__manifest__.py");
        if !manifest_path.exists() {
            return None;
        }
        let manifest_file_info = odoo.file_mgr.get_file_info(manifest_path.as_os_str().to_str().unwrap());
        let manifest_file_info = (*manifest_file_info).borrow();
        if manifest_file_info.ast.is_none() {
            return None;
        }
        module._load_manifest(&manifest_file_info);
        //TODO handle diagnostics
        Some(module)
    }

    pub fn load_module_info(&mut self) {
        if self.loaded {
            return;
        }
        //let loaded = Vec::new();
        self._load_depends();
    }

    /* Load manifest to identify the module characteristics.
    Returns list of od diagnostics to publish in manifest file. */
    fn _load_manifest(&self, file_info: &FileInfo) -> Vec<Diagnostic> {
        let mut res = vec![];
        let ast = file_info.ast.as_ref().unwrap();
        let mut is_manifest_valid = true;
        if ast.len() != 1 {is_manifest_valid = false;}
        match &ast[0] {
            Stmt::Expr(expr) => {
                if expr.value.is_dict_expr() {
                    //everything is fine, let's process it below
                } else {
                    is_manifest_valid = false;
                }
            },
            _ => {is_manifest_valid = false;}
        }
        if !is_manifest_valid {
            res.push(Diagnostic::new(
                Range::new(Position::new(0, 0), Position::new(0, 1)),
                Some(DiagnosticSeverity::ERROR),
                None,
                Some(EXTENSION_NAME.to_string()),
                "A manifest should only contains one dictionnary".to_string(),
                None,
                None,
            ));
            return res;
        }
        let dict = &ast[0].as_expr_stmt().unwrap().value.clone().dict_expr().unwrap();
        //TODO parse manifest and generate diagnostics
        res
    }

    /* ensure that all modules indicates in the module dependencies are well loaded.
    Returns list of diagnostics to publish in manifest file */
    fn _load_depends(&mut self) {

    }

}