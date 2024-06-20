use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range};
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashSet;

use crate::constants::*;
use crate::core::file_mgr::FileInfo;
use crate::core::import_resolver::find_module;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::constants::EXTENSION_NAME;
use crate::S;
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;


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
        println!("building new module: {:?}", dir_path);
        if dir_path.components().last().unwrap().as_os_str().to_str().unwrap() == "base" {
            module.depends.clear();
        }
        module.dir_name = dir_path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string();
        let manifest_path = dir_path.join("__manifest__.py");
        if !manifest_path.exists() {
            return None;
        }
        let manifest_file_info = odoo.get_file_mgr().borrow_mut().update_file_info(odoo, manifest_path.as_os_str().to_str().unwrap(), None, None, false);
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
        println!("End building new module: {:?}", dir_path);
        Some(module)
    }

    pub fn load_module_info(symbol: Rc<RefCell<Symbol>>, odoo: &mut SyncOdoo, odoo_addons: Rc<RefCell<Symbol>>) -> Vec<String> {
        {
            let _symbol = symbol.borrow();
            let module = _symbol._module.as_ref().expect("Module must be set to call load_module_info");
            if module.loaded {
                return vec![];
            }
        }
        let (mut diagnostics, mut loaded) = ModuleSymbol::_load_depends(&mut (*symbol).borrow_mut(), odoo, odoo_addons);
        diagnostics.append(&mut ModuleSymbol::_load_data(symbol.clone(), odoo));
        diagnostics.append(&mut ModuleSymbol::_load_arch(symbol.clone(), odoo));
        {
            let mut _symbol = symbol.borrow_mut();
            let module = _symbol._module.as_mut().expect("Module must be set to call load_module_info");
            module.loaded = true;
            loaded.push(module.dir_name.clone());
            let manifest_path = PathBuf::from(module.root_path.clone()).join("__manifest__.py");
            let manifest_file_info = odoo.get_file_mgr().borrow_mut().get_file_info(&S!(manifest_path.as_os_str().to_str().unwrap())).expect("file not found in cache").clone();
            let mut manifest_file_info = (*manifest_file_info).borrow_mut();
            manifest_file_info.replace_diagnostics(crate::constants::BuildSteps::ARCH, diagnostics);
            manifest_file_info.publish_diagnostics(odoo);
        }
        loaded
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
                Some(NumberOrString::String(S!("OLS30201"))),
                Some(EXTENSION_NAME.to_string()),
                "A manifest should only contains one dictionnary".to_string(),
                None,
                None,
            ));
            return res;
        }
        let dict = &ast[0].as_expr_stmt().unwrap().value.clone().dict_expr().unwrap();
        for (index, key) in dict.iter_keys().enumerate() {
            match key {
                Some(key) => {
                    let value = &dict.items.get(index).unwrap().value;
                    match key {
                        Expr::StringLiteral(key_literal) => {
                            let key_str = key_literal.value.to_string();
                            if key_str == "name" {
                                if !value.is_string_literal_expr() {
                                    res.push(self._create_diagnostic_for_manifest_key("The name of the module should be a string", S!("OLS30203"), &key_literal.range));
                                } else {
                                    self.module_name = value.as_string_literal_expr().unwrap().value.to_string();
                                }
                            } else if key_str == "depends" {
                                if !value.is_list_expr() {
                                    res.push(self._create_diagnostic_for_manifest_key("The depends value should be a list", S!("OLS30204"), &key_literal.range));
                                } else {
                                    for depend in value.as_list_expr().unwrap().elts.iter() {
                                        if !depend.is_string_literal_expr() {
                                            res.push(self._create_diagnostic_for_manifest_key("The depends key should be a list of strings", S!("OLS30205"), &depend.range()));
                                        } else {
                                            let depend_value = depend.as_string_literal_expr().unwrap().value.to_string();
                                            if depend_value == self.dir_name {
                                                res.push(self._create_diagnostic_for_manifest_key("A module cannot depends on itself", S!("OLS30206"), &depend.range()));
                                            } else {
                                                self.depends.push(depend_value);
                                            }
                                        }
                                    }
                                }
                            } else if key_str == "data" {
                                if !value.is_list_expr() {
                                    res.push(self._create_diagnostic_for_manifest_key("The data value should be a list", S!("OLS30207"), &key_literal.range));
                                } else {
                                    for data in value.as_list_expr().unwrap().elts.iter() {
                                        if !data.is_literal_expr() {
                                            res.push(self._create_diagnostic_for_manifest_key("The data key should be a list of strings", S!("OLS30208"), &data.range()));
                                        } else {
                                            self.data.push(data.as_string_literal_expr().unwrap().value.to_string());
                                        }
                                    }
                                }
                            } else if key_str == "active" {
                                res.push(Diagnostic::new(
                                    Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                                    Some(DiagnosticSeverity::WARNING),
                                    Some(NumberOrString::String(S!("OLS20201"))),
                                    Some(EXTENSION_NAME.to_string()),
                                    "The active key is deprecated".to_string(),
                                    None,
                                    Some(vec![DiagnosticTag::DEPRECATED]),
                                ))
                            } else {
                                //res.push(self._create_diagnostic_for_manifest_key("Manifest keys should be strings", &key.range()));
                            }
                        }
                        _ => {
                            res.push(self._create_diagnostic_for_manifest_key("Manifest keys should be strings", S!("OLS30209"), &key.range()));
                        }
                    }
                },
                None => {
                    res.push(Diagnostic::new(
                        Range::new(Position::new(0, 0), Position::new(0, 1)),
                        Some(DiagnosticSeverity::ERROR),
                        Some(NumberOrString::String(S!("OLS30302"))),
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

    fn _create_diagnostic_for_manifest_key(&self, text: &str, code: String, range: &TextRange) -> Diagnostic {
        return Diagnostic::new(
            Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
            Some(DiagnosticSeverity::ERROR),
            Some(NumberOrString::String(code)),
            Some(EXTENSION_NAME.to_string()),
            text.to_string(),
            None,
            None,
        )
    }

    /* ensure that all modules indicates in the module dependencies are well loaded.
    Returns list of diagnostics to publish in manifest file */
    fn _load_depends(symbol: &mut Symbol, odoo: &mut SyncOdoo, odoo_addons: Rc<RefCell<Symbol>>) -> (Vec<Diagnostic>, Vec<String>) {
        let module = symbol._module.as_ref().expect("Module must be set to call _load_depends");
        let mut diagnostics: Vec<Diagnostic> = vec![];
        let mut loaded: Vec<String> = vec![];
        for depend in module.depends.clone().iter() {
            //TODO: raise an error on dependency cycle
            if !odoo.modules.contains_key(depend) {
                let module = find_module(odoo, odoo_addons.clone(), depend);
                if module.is_none() {
                    odoo.not_found_symbols.insert(symbol.weak_self.as_ref().unwrap().upgrade().expect("The symbol must be in the tree"));
                    symbol.not_found_paths.push((BuildSteps::ARCH, vec![S!("odoo"), S!("addons"), depend.clone()]));
                    diagnostics.push(Diagnostic::new(
                        Range::new(Position::new(0, 0), Position::new(0, 1)),
                        Some(DiagnosticSeverity::ERROR),
                        Some(NumberOrString::String(S!("OLS30210"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("Module {} depends on {} which is not found. Please review your addons paths", symbol.name, depend),
                        None,
                        None,
                    ))
                } else {
                    loaded.push(depend.clone());
                    let module = module.unwrap();
                    let mut module = (*module).borrow_mut();
                    symbol.add_dependency(&mut module, BuildSteps::ARCH, BuildSteps::ARCH);
                }
            } else {
                let module = odoo.modules.get(depend).unwrap().upgrade().unwrap();
                let mut module = (*module).borrow_mut();
                symbol.add_dependency(&mut module, BuildSteps::ARCH, BuildSteps::ARCH)
            }
        }
        (diagnostics, loaded)
    }

    fn _load_data(_symbol: Rc<RefCell<Symbol>>, _odoo: &mut SyncOdoo) -> Vec<Diagnostic> {
        vec![]
    }

    fn _load_arch(symbol: Rc<RefCell<Symbol>>, odoo: &mut SyncOdoo) -> Vec<Diagnostic> {
        let root_path = (*symbol).borrow()._module.as_ref().expect("Module must be set to call _load_depends").root_path.clone();
        let tests_path = PathBuf::from(root_path).join("tests");
        if tests_path.exists() {
            let _arc_symbol = Symbol::create_from_path(odoo, &tests_path, symbol, false);
            if _arc_symbol.is_some() {
                let _arc_symbol = _arc_symbol.unwrap();
                odoo.add_to_rebuild_arch(_arc_symbol);
            }
        }
        vec![]
    }

    pub fn is_in_deps(odoo: &mut SyncOdoo, symbol: &Rc<RefCell<Symbol>>, dir_name: &String, acc: &mut Option<HashSet<String>>) -> bool {
        if symbol.borrow()._module.as_ref().unwrap().dir_name == *dir_name || symbol.borrow()._module.as_ref().unwrap().depends.contains(dir_name) {
            return true;
        }
        if acc.is_none() {
            *acc = Some(HashSet::new());
        }
        for dep in symbol.borrow()._module.as_ref().unwrap().depends.iter() {
            if acc.as_ref().unwrap().contains(dep) {
                continue;
            }
            let dep_module = odoo.modules.get(dep);
            if let Some(dep_module) = dep_module {
                let dep_module = dep_module.upgrade();
                if dep_module.is_none() {
                    continue;
                }
                if ModuleSymbol::is_in_deps(odoo, dep_module.as_ref().unwrap(), dir_name, acc) {
                    return true;
                }
                acc.as_mut().unwrap().insert(dep_module.as_ref().unwrap().borrow()._module.as_ref().unwrap().dir_name.clone());
            }
        }
        false
    }

}