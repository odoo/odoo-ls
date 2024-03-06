use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, Position, Range};
use rustpython_parser::ast::{Expr, Ranged, Stmt};
use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::Constant::Str;

use crate::constants::*;
use crate::core::file_mgr::FileInfo;
use crate::core::import_resolver::find_module;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::constants::EXTENSION_NAME;
use std::path::PathBuf;


#[derive(Debug)]
pub struct ModuleSymbol {
    root_path: String,
    loaded: bool,
    module_name: String,
    pub dir_name: String,
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
        let mut manifest_file_info = (*manifest_file_info).borrow_mut();
        if manifest_file_info.ast.is_none() {
            return None;
        }
        let diags = module._load_manifest(&manifest_file_info);
        if odoo.modules.contains_key(&module.dir_name) {
            //TODO: handle multiple modules with the same name
        }
        manifest_file_info.replace_diagnostics(crate::constants::BuildSteps::SYNTAX, diags);
        manifest_file_info.publish_diagnostics(odoo);
        drop(manifest_file_info);
        Some(module)
    }

    pub fn load_module_info(symbol: &mut Symbol, odoo: &mut SyncOdoo) {
        let module = symbol._module.as_ref().expect("Module must be set to call load_module_info");
        if module.loaded {
            return;
        }
        //let loaded = Vec::new();
        drop(module);
        ModuleSymbol::_load_depends(symbol, odoo);
    }

    /* Load manifest to identify the module characteristics.
    Returns list of od diagnostics to publish in manifest file. */
    fn _load_manifest(&mut self, file_info: &FileInfo) -> Vec<Diagnostic> {
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
        for (index, key) in dict.keys.iter().enumerate() {
            match key {
                Some(key) => {
                    let value = dict.values.get(index).unwrap();
                    match key {
                        Expr::Constant(key_const) => {
                            match &key_const.value {
                                Str(key_str) => {
                                    if key_str == "name" {
                                        if !value.is_constant_expr() || !value.as_constant_expr().unwrap().value.is_str() {
                                            res.push(self._create_diagnostic_for_manifest_key(file_info, "The name of the module should be a string", &key.range()));
                                        } else {
                                            self.module_name = value.as_constant_expr().unwrap().value.as_str().unwrap().to_string();
                                        }
                                    } else if key_str == "depends" {
                                        if !value.is_list_expr() {
                                            res.push(self._create_diagnostic_for_manifest_key(file_info, "The depends value should be a list", &key.range()));
                                        } else {
                                            for depend in value.as_list_expr().unwrap().elts.iter() {
                                                if !depend.is_constant_expr() || !depend.as_constant_expr().unwrap().value.is_str() {
                                                    res.push(self._create_diagnostic_for_manifest_key(file_info, "The depends key should be a list of strings", &depend.range()));
                                                } else {
                                                    let depend_value = depend.as_constant_expr().unwrap().value.as_str().unwrap().to_string();
                                                    if depend_value == self.dir_name {
                                                        res.push(self._create_diagnostic_for_manifest_key(file_info, "A module cannot depends on itself", &depend.range()));
                                                    } else {
                                                        self.depends.push(depend_value);
                                                    }
                                                }
                                            }
                                        }
                                    } else if key_str == "data" {
                                        if !value.is_list_expr() {
                                            res.push(self._create_diagnostic_for_manifest_key(file_info, "The data value should be a list", &key.range()));
                                        } else {
                                            for data in value.as_list_expr().unwrap().elts.iter() {
                                                if !data.is_constant_expr() || !data.as_constant_expr().unwrap().value.is_str() {
                                                    res.push(self._create_diagnostic_for_manifest_key(file_info, "The data key should be a list of strings", &data.range()));
                                                } else {
                                                    self.data.push(data.as_constant_expr().unwrap().value.as_str().unwrap().to_string());
                                                }
                                            }
                                        }
                                    } else if key_str == "active" {
                                        res.push(Diagnostic::new(
                                            file_info.text_range_to_range(&key.range()).unwrap(),
                                            Some(DiagnosticSeverity::WARNING),
                                            None,
                                            Some(EXTENSION_NAME.to_string()),
                                            "The active key is deprecated".to_string(),
                                            None,
                                            Some(vec![DiagnosticTag::DEPRECATED]),
                                        ))
                                    }
                                },
                                _ => {
                                    res.push(self._create_diagnostic_for_manifest_key(file_info, "Manifest keys should be strings", &key.range()));
                                }
                            }
                        }
                        _ => {
                            res.push(self._create_diagnostic_for_manifest_key(file_info, "Manifest keys should be strings", &key.range()));
                        }
                    }
                },
                None => {
                    res.push(Diagnostic::new(
                        Range::new(Position::new(0, 0), Position::new(0, 1)),
                        Some(DiagnosticSeverity::ERROR),
                        None,
                        Some(EXTENSION_NAME.to_string()),
                        "Do not use dict unpacking to build your manifest".to_string(),
                        None,
                        None,
                    ));
                    return res;
                }
            }
        }
        res
    }

    fn _create_diagnostic_for_manifest_key(&self, file_info: &FileInfo, text: &str, range: &TextRange) -> Diagnostic {
        return Diagnostic::new(
            file_info.text_range_to_range(range).unwrap(),
            Some(DiagnosticSeverity::ERROR),
            None,
            Some(EXTENSION_NAME.to_string()),
            text.to_string(),
            None,
            None,
        )
    }

    /* ensure that all modules indicates in the module dependencies are well loaded.
    Returns list of diagnostics to publish in manifest file */
    fn _load_depends(symbol: &mut Symbol, odoo: &mut SyncOdoo) {
        let module = symbol._module.as_ref().expect("Module must be set to call _load_depends");
        let diagnostics: Vec<Diagnostic> = vec![];
        let loaded: Vec<String> = vec![];
        for depend in module.depends.clone().iter() {
            //TODO: raise an error on dependency cycle
            if !odoo.modules.contains_key(depend) {
                let module = find_module(odoo, depend);
            } else {
                symbol.add_dependency(&mut odoo.modules.get(depend).unwrap().upgrade().unwrap().borrow_mut(), BuildSteps::ARCH, BuildSteps::ARCH)
            }
        }
    }

}