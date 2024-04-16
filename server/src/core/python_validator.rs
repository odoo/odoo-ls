use rustpython_parser::ast::{self, StmtClassDef};
use rustpython_parser::ast::Stmt;
use std::ptr;
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;
use tower_lsp::lsp_types::Diagnostic;
use crate::constants::{BuildStatus, DEBUG_MODE};
use crate::core::symbol::Symbol;
use crate::core::odoo::SyncOdoo;
use crate::constants::*;

use super::file_mgr::{FileInfo, FileMgr};
use super::symbols::{class_symbol, function_symbol};

#[derive(Debug)]
pub struct PythonValidator {
    symbol: Rc<RefCell<Symbol>>,
    diagnostics: Vec<Diagnostic>,
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
        }
    }

    fn get_file_info(&mut self, odoo: &mut SyncOdoo) -> Rc<RefCell<FileInfo>> {
        let symbol = self.symbol.borrow();
        let mut path = symbol.paths[0].clone();
        if symbol.sym_type == SymType::PACKAGE {
            path = PathBuf::from(path).join("__init__.py").as_os_str().to_str().unwrap().to_owned() + symbol.i_ext.as_str();
        }
        let file_info_rc = odoo.get_file_mgr().borrow_mut().get_file_info(odoo, path.as_str(), None, None); //create ast
        file_info_rc
    }

    /* Validate the symbol. The dependencies must be done before any validation. */
    pub fn validate(&mut self, odoo: &mut SyncOdoo) {
        let mut symbol = self.symbol.borrow_mut();
        symbol.validation_status = BuildStatus::IN_PROGRESS;
        if symbol.validation_status != BuildStatus::PENDING {
            return;
        }
        let sym_type = symbol.sym_type.clone();
        drop(symbol);
        match sym_type {
            SymType::FILE | SymType::PACKAGE => {
                let file_info = self.get_file_info(odoo);
                let mut file_info = file_info.borrow_mut();
                if file_info.ast.is_some() {
                    self.validate_body(odoo, file_info.ast.as_ref().unwrap());
                    file_info.replace_diagnostics(BuildSteps::ARCH_EVAL, self.diagnostics.clone());
                }
            },
            SymType::CLASS | SymType::FUNCTION => {
                let ref_symbol = self.symbol.clone(); //to make 'body' lives until the end
                let stmt = unsafe{&*ref_symbol.borrow().ast_ptr};
                let body = match stmt {
                    Stmt::FunctionDef(s) => {
                        &s.body
                    },
                    Stmt::ClassDef(s) => {
                        &s.body
                    }
                    _ => {panic!("Wrong statement in validation ast extraction")}
                };
                self.validate_body(odoo, body);
            },
            _ => {panic!("Only File, function or class can be validated")}
        }
        let mut symbol = self.symbol.borrow_mut();
        symbol.validation_status = BuildStatus::DONE;
        if symbol.in_workspace {
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
                        panic!("symbol not found.");
                    }
                },
                Stmt::ClassDef(c) => {
                    let sym = self.symbol.borrow().get_positioned_symbol(&c.name.to_string(), &c.range);
                    if let Some(sym) = sym {
                        let val_status = sym.borrow().validation_status.clone();
                        if val_status == BuildStatus::PENDING {
                            let mut v = PythonValidator::new(sym.clone());
                            v.validate(odoo);
                        } else if val_status == BuildStatus::IN_PROGRESS {
                            panic!("cyclic validation detected... Aborting");
                        }
                        self.diagnostics.append(&mut sym.borrow_mut()._class.as_mut().unwrap().diagnostics);
                    } else {
                        panic!("symbol not found.");
                    }
                },
                _ => {
                    println!("Stmt not handled");
                }
            }
        }
    }
}