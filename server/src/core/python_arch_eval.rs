use std::rc::{Rc, Weak};
use std::cell::RefCell;

use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::{Identifier, Stmt, Alias, Int, StmtAnnAssign, StmtAssign};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, Position, Range};
use std::path::PathBuf;

use crate::constants::*;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::file_mgr::FileInfo;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::core::evaluation::Evaluation;
use crate::core::python_utils;

use super::config::DiagMissingImportsMode;
use super::import_resolver::ImportResult;
use super::symbol;


#[derive(Debug, Clone)]
pub struct PythonArchEval {
    symbol: Rc<RefCell<Symbol>>,
    diagnostics: Vec<Diagnostic>,
    safe_import: Vec<bool>,
}

impl PythonArchEval {
    pub fn new(symbol: Rc<RefCell<Symbol>>) -> PythonArchEval {
        PythonArchEval {
            symbol: symbol,
            diagnostics: Vec::new(),
            safe_import: vec![false],
        }
    }

    pub fn eval_arch(&mut self, odoo: &mut SyncOdoo) {
        //println!("eval arch");
        let mut symbol = self.symbol.borrow_mut();
        symbol.arch_eval_status = BuildStatus::IN_PROGRESS;
        if symbol.paths.len() != 1 {
            panic!()
        }
        let mut path = symbol.paths[0].clone();
        //println!("eval path: {}", path);
        if symbol.sym_type == SymType::PACKAGE {
            path = PathBuf::from(path).join("__init__.py").as_os_str().to_str().unwrap().to_owned() + symbol.i_ext.as_str();
        }
        drop(symbol);
        let file_info_rc = odoo.get_file_mgr().borrow_mut().get_file_info(odoo, path.as_str(), None, None); //create ast
        let file_info = (*file_info_rc).borrow();
        if file_info.ast.is_some() {
            for stmt in file_info.ast.as_ref().unwrap() {
                match stmt {
                    //TODO move import logic from ast visiting to symbol analyzing
                    Stmt::Import(import_stmt) => {
                        self.eval_local_symbols_from_import_stmt(odoo, &file_info, None, &import_stmt.names, None, &import_stmt.range)
                    },
                    Stmt::ImportFrom(import_from_stmt) => {
                        self.eval_local_symbols_from_import_stmt(odoo, &file_info, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level.as_ref(), &import_from_stmt.range)
                    },
                    _ => {}
                }
            }
        }
        drop(file_info);
        let mut file_info = (*file_info_rc).borrow_mut();
        file_info.replace_diagnostics(BuildSteps::ARCH_EVAL, self.diagnostics.clone());
        //TODO remove that temporary publish
        file_info.publish_diagnostics(odoo);
        let mut symbol = self.symbol.borrow_mut();
        symbol.arch_eval_status = BuildStatus::DONE;
        //TODO odoo.add_to_rebuild_odoo(Arc::downgrade(&self.sym_stack[0]));
    }

    fn _match_diag_config(&self, odoo: &mut SyncOdoo, symbol: &Rc<RefCell<Symbol>>) -> bool {
        let import_diag_level = &odoo.config.diag_missing_imports;
        if *import_diag_level == DiagMissingImportsMode::None {
            return false
        }
        if *import_diag_level == DiagMissingImportsMode::All {
            return true
        }
        if *import_diag_level == DiagMissingImportsMode::OnlyOdoo {
            let tree = symbol.borrow().get_tree();
            if tree.0.len() > 0 && tree.0[0] == "odoo" {
                return true;
            }
        }
        false
    }

    fn eval_local_symbols_from_import_stmt(&mut self, odoo: &mut SyncOdoo, file_info: &FileInfo, from_stmt: Option<&Identifier>, name_aliases: &[Alias<TextRange>], level: Option<&Int>, range: &TextRange) {
        if name_aliases.len() == 1 && name_aliases[0].name.to_string() == "*" {
            return;
        }
        let import_results: Vec<ImportResult> = resolve_import_stmt(
            odoo,
            &self.symbol,
            &self.symbol,
            from_stmt,
            name_aliases,
            level,
            range);
        
        for _import_result in import_results.iter() {
            let variable = self.symbol.borrow_mut().get_positioned_symbol(&_import_result.name, &_import_result.range);
            if variable.is_none() {
                continue;
            }
            if _import_result.found {
                //resolve the symbol and build necessary evaluations
                let (mut _sym, mut instance): (Weak<RefCell<Symbol>>, bool) = Symbol::follow_ref(_import_result.symbol.clone(), odoo, false);
                let mut old_ref: Option<Weak<RefCell<Symbol>>> = None;
                let mut arc_sym = _sym.upgrade().unwrap();
                let mut sym = arc_sym.borrow_mut();
                while sym.evaluation.is_none() && (old_ref.is_none() || !Rc::ptr_eq(&arc_sym, &old_ref.as_ref().unwrap().upgrade().unwrap())) {
                    old_ref = Some(_sym.clone());
                    let file_sym = sym.get_in_parents(&vec![SymType::FILE, SymType::PACKAGE], true);
                    drop(sym);
                    if file_sym.is_some() {
                        let arc_file_sym = file_sym.as_ref().unwrap().upgrade().unwrap();
                        if arc_file_sym.borrow_mut().arch_eval_status == BuildStatus::PENDING && odoo.is_in_rebuild(&arc_file_sym, BuildSteps::ARCH_EVAL) {
                            odoo.remove_from_rebuild_arch_eval(&arc_file_sym);
                            let mut builder = PythonArchEval::new(arc_file_sym);
                            builder.eval_arch(odoo);
                            (_sym, instance) = Symbol::follow_ref(_import_result.symbol.clone(), odoo, false);
                            arc_sym = _sym.upgrade().unwrap();
                            sym = arc_sym.borrow_mut();
                        } else {
                            sym = arc_sym.borrow_mut();
                        }
                    } else {
                        sym = arc_sym.borrow_mut();
                    }
                }
                drop(sym);
                if !Rc::ptr_eq(&arc_sym, &variable.as_ref().unwrap()) { //anti-loop. We want to be sure we are not evaluating to the same sym
                    variable.as_ref().unwrap().borrow_mut().evaluation = Some(Evaluation::eval_from_symbol(&_import_result.symbol));
                    variable.as_ref().unwrap().borrow_mut().add_dependency(&mut _import_result.symbol.borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
                } else {
                    let mut file_tree = vec![_import_result.file_tree.0.clone(), _import_result.file_tree.1.clone()].concat();
                    file_tree.extend(_import_result.name.split(".").map(str::to_string));
                    self.symbol.borrow_mut().not_found_paths.push((BuildSteps::ARCH_EVAL, file_tree.clone()));
                    odoo.not_found_symbols.insert(self.symbol.clone());
                    if self._match_diag_config(odoo, &_import_result.symbol) {
                        let range = file_info.text_range_to_range(&_import_result.range).unwrap();
                        self.diagnostics.push(Diagnostic::new(
                            range,
                            Some(DiagnosticSeverity::WARNING),
                            None,
                            Some(EXTENSION_NAME.to_string()),
                            format!("{} not found", file_tree.clone().join(".")),
                            None,
                            None,
                        ));
                    }
                }

            } else {
                let mut file_tree = vec![_import_result.file_tree.0.clone(), _import_result.file_tree.1.clone()].concat();
                file_tree.extend(_import_result.name.split(".").map(str::to_string));
                if BUILT_IN_LIBS.contains(&file_tree[0].as_str()) {
                    continue;
                }
                if !self.safe_import.last().unwrap() {
                    self.symbol.borrow_mut().not_found_paths.push((BuildSteps::ARCH_EVAL, file_tree.clone()));
                    odoo.not_found_symbols.insert(self.symbol.clone());
                    if self._match_diag_config(odoo, &_import_result.symbol) {
                        let range = file_info.text_range_to_range(&_import_result.range).unwrap();
                        self.diagnostics.push(Diagnostic::new(
                            range,
                            Some(DiagnosticSeverity::WARNING),
                            None,
                            Some(EXTENSION_NAME.to_string()),
                            format!("{} not found", file_tree.clone().join(".")),
                            None,
                            None,
                        ));
                    }
                }
            }
        }
    }

    fn _visit_ann_assign(&self, odoo: &mut SyncOdoo, ann_assign_stmt: &StmtAnnAssign) {
        let assigns = match ann_assign_stmt.value.as_ref() {
            Some(value) => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), Some(value)),
            None => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), None)
        };
        for assign in assigns.iter() { //should only be one
            let mut variable = Symbol::new(assign.target.id.to_string(), SymType::VARIABLE);
            variable.range = Some(assign.target.range.clone());
            variable.evaluation = None;
            self.symbol.borrow_mut().add_symbol(odoo, variable);
        }
    }

    fn _visit_assign(&self, odoo: &mut SyncOdoo, assign_stmt: &StmtAssign) {
        //TODO
    }
}