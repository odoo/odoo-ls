use ruff_python_ast::{Stmt, StmtTry, Identifier, Alias, Int};
use ruff_text_size::TextRange;
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range, Position};
use crate::constants::*;
use crate::core::symbol::Symbol;
use crate::core::odoo::SyncOdoo;
use crate::core::import_resolver::{resolve_import_stmt};
use crate::core::symbols::module_symbol::ModuleSymbol;

use super::file_mgr::{FileInfo};

#[derive(Debug)]
pub struct PythonValidator {
    symbol: Rc<RefCell<Symbol>>,
    diagnostics: Vec<Diagnostic>,
    safe_imports: Vec<bool>,
    current_module: Option<Rc<RefCell<Symbol>>>
}

/* PythonValidator operate on a single Symbol. Unlike other steps, it can be done on any symbol containing code (file, function, class. Not variable, namespace).
It will validate this node and run a validator on all subsymbol and dependencies.
It will try to inference the return type of functions if it is not annotated;
To achieve that, it will keep a pointer to the corresponding ast node. This pointer (unsafe in rust) is valid as long asµ
the file_info.ast is not modified during the process, which should never occur. */
impl PythonValidator {
    pub fn new(symbol: Rc<RefCell<Symbol>>) -> Self {
        Self {
            symbol,
            diagnostics: Vec::new(),
            safe_imports: vec![false],
            current_module: None,
        }
    }

    fn get_file_info(&mut self, odoo: &mut SyncOdoo) -> Rc<RefCell<FileInfo>> {
        let file_symbol = self.symbol.borrow().get_in_parents(&vec![SymType::FILE, SymType::PACKAGE], true).unwrap().upgrade().unwrap();
        let file_symbol = file_symbol.borrow();
        let mut path = file_symbol.paths[0].clone();
        if file_symbol.sym_type == SymType::PACKAGE {
            path = PathBuf::from(path).join("__init__.py").as_os_str().to_str().unwrap().to_owned() + file_symbol.i_ext.as_str();
        }
        let file_info_rc = odoo.get_file_mgr().borrow_mut().get_file_info(odoo, path.as_str());
        file_info_rc
    }

    fn find_stmt_from_ast<'a>(ast: &'a Vec<Stmt>, indexes: &Vec<u16>) -> &'a Stmt {
        let mut stmt = ast.get(indexes[0] as usize).expect("index not found in ast");
        for i in indexes.iter().skip(1) {
            match stmt {
                Stmt::ClassDef(c) => {
                    stmt = c.body.get(*i as usize).expect("index not found in ast");
                },
                Stmt::FunctionDef(f) => {
                    stmt = f.body.get(*i as usize).expect("index not found in ast");
                }
                _ => {}
            }
        }
        stmt
    }

    /* Validate the symbol. The dependencies must be done before any validation. */
    pub fn validate(&mut self, odoo: &mut SyncOdoo) {
        let mut symbol = self.symbol.borrow_mut();
        self.current_module = symbol.get_module_sym();
        if symbol.validation_status != BuildStatus::PENDING {
            return;
        }
        symbol.validation_status = BuildStatus::IN_PROGRESS;
        let sym_type = symbol.sym_type.clone();
        drop(symbol);
        match sym_type {
            SymType::FILE | SymType::PACKAGE => {
                let file_info_rc = self.get_file_info(odoo).clone();
                let file_info = file_info_rc.borrow();
                if file_info.ast.is_some() {
                    self.validate_body(odoo, file_info.ast.as_ref().unwrap());
                }
                drop(file_info);
                let mut file_info = file_info_rc.borrow_mut();
                file_info.replace_diagnostics(BuildSteps::VALIDATION, self.diagnostics.clone());
            },
            SymType::CLASS | SymType::FUNCTION => {
                let file_info_rc = self.get_file_info(odoo).clone();
                let file_info = file_info_rc.borrow();
                if file_info.ast.is_some() {
                    let stmt = PythonValidator::find_stmt_from_ast(file_info.ast.as_ref().unwrap(), self.symbol.borrow().ast_indexes.as_ref().expect("this node should contains an index vector"));
                    let body = match stmt {
                        Stmt::FunctionDef(s) => {
                            &s.body
                        },
                        Stmt::ClassDef(s) => {
                            &s.body
                        }
                        _ => {panic!("Wrong statement in validation ast extraction {} ", sym_type)}
                    };
                    self.validate_body(odoo, body);
                } else {
                    println!("no ast found on file info");
                }
                
            },
            _ => {panic!("Only File, function or class can be validated")}
        }
        let mut symbol = self.symbol.borrow_mut();
        symbol.validation_status = BuildStatus::DONE;
        if vec![SymType::FILE, SymType::PACKAGE].contains(&symbol.sym_type) {
            if !symbol.in_workspace {
                odoo.get_file_mgr().borrow_mut().delete_path(odoo, &symbol.paths[0].to_string());
            } else {
                drop(symbol);
                match sym_type {
                    SymType::FILE | SymType::PACKAGE => {
                        let file_info = self.get_file_info(odoo);
                        let mut file_info = file_info.borrow_mut();
                        file_info.publish_diagnostics(odoo);
                    },
                    _ => {}
                }
            }
        }
    }

    fn validate_body(&mut self, odoo: &mut SyncOdoo, vec_ast: &Vec<Stmt>) {
        for stmt in vec_ast.iter() {
            match stmt {
                Stmt::FunctionDef(f) => {
                    let sym = self.symbol.borrow().get_positioned_symbol(&f.name.to_string(), &f.range);
                    if let Some(sym) = sym {
                        let val_status = sym.borrow().validation_status.clone();
                        if val_status == BuildStatus::PENDING {
                            let mut v = PythonValidator::new(sym.clone());
                            v.validate(odoo);
                        } else if val_status == BuildStatus::IN_PROGRESS {
                            panic!("cyclic validation detected... Aborting");
                        }
                        self.diagnostics.append(&mut sym.borrow_mut()._function.as_mut().unwrap().diagnostics);
                    } else {
                        //TODO panic!("symbol not found.");
                    }
                },
                Stmt::ClassDef(c) => {
                    let sym = self.symbol.borrow().get_positioned_symbol(&c.name.to_string(), &c.range);
                    if let Some(sym) = sym {
                        let val_status = sym.borrow().validation_status.clone();
                        if val_status == BuildStatus::PENDING {
                            //TODO not possible, as we do not release the file_info of the file validator.
                            let mut v = PythonValidator::new(sym.clone());
                            v.validate(odoo);
                        } else if val_status == BuildStatus::IN_PROGRESS {
                            panic!("cyclic validation detected... Aborting");
                        }
                        self.diagnostics.append(&mut sym.borrow_mut()._class.as_mut().unwrap().diagnostics);
                    } else {
                        //TODO panic!("symbol not found.");
                    }
                },
                Stmt::Try(t) => {
                    self.visit_try(odoo, t);
                },
                Stmt::Import(i) => {
                    self._resolve_import(odoo, None, &i.names, None, &i.range);
                },
                Stmt::ImportFrom(i) => {
                    self._resolve_import(odoo, i.module.as_ref(), &i.names, i.level, &i.range);
                }
                _ => {
                    println!("Stmt not handled");
                }
            }
        }
    }

    fn visit_try(&mut self, odoo: &mut SyncOdoo, node: &StmtTry) {
        let mut safe = false;
        for handler in node.handlers.iter() {
            let handler = handler.as_except_handler().unwrap();
            if let Some(type_) = &handler.type_ {
                if type_.is_name_expr() && type_.as_name_expr().unwrap().id.to_string() == "ImportError" {
                    safe = true;
                }
            }
        }
        self.safe_imports.push(safe);
        self.validate_body(odoo, &node.body);
        self.safe_imports.pop();
    }

    fn _resolve_import(&mut self, odoo: &mut SyncOdoo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: Option<u32>, range: &TextRange) {
        let file_symbol = self.symbol.borrow().get_in_parents(&vec![SymType::FILE, SymType::PACKAGE], true);
        let file_symbol = file_symbol.expect("file symbol not found").upgrade().expect("unable to upgrade file symbol");
        let import_results = resolve_import_stmt(
            odoo,
            &file_symbol,
            &self.symbol,
            from_stmt,
            name_aliases,
            level,
            range);
        for import_result in import_results.iter() {
            if import_result.found && self.current_module.is_some() {
                let module = import_result.symbol.borrow().get_module_sym();
                if let Some(module) = module {
                    if !ModuleSymbol::is_in_deps(odoo, &self.current_module.as_ref().unwrap(), &module.borrow()._module.as_ref().unwrap().dir_name, &mut None) && !self.safe_imports.last().unwrap() {
                        self.diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(import_result.range.start().to_u32(), 0), Position::new(import_result.range.end().to_u32(), 0)),
                            Some(DiagnosticSeverity::ERROR),
                            None,
                            Some(EXTENSION_NAME.to_string()),
                            format!("{} is not in the dependencies of the module", module.borrow()._module.as_ref().unwrap().dir_name),
                            None,
                            None,
                        ))
                    }
                }
            }
        }
    }
}