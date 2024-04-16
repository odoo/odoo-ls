use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::{ptr, vec};

use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::{Alias, Expr, Identifier, Int, Ranged, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};
use weak_table::traits::WeakElement;
use std::path::PathBuf;

use crate::constants::*;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::file_mgr::FileInfo;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::core::evaluation::Evaluation;
use crate::core::python_utils;
use crate::S;

use super::config::DiagMissingImportsMode;
use super::import_resolver::ImportResult;
use super::python_arch_eval_hooks::PythonArchEvalHooks;


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
        if symbol.arch_eval_status != BuildStatus::PENDING {
            return;
        }
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
                self.visit_stmt(odoo, stmt, &file_info);
            }
        }
        drop(file_info);
        let mut file_info = (*file_info_rc).borrow_mut();
        file_info.replace_diagnostics(BuildSteps::ARCH_EVAL, self.diagnostics.clone());
        PythonArchEvalHooks::on_file_eval(odoo, self.symbol.clone());
        let mut symbol = self.symbol.borrow_mut();
        symbol.arch_eval_status = BuildStatus::DONE;
        if symbol.is_external {
            for sym in symbol.all_symbols(None, false) {
                sym.borrow_mut().ast_ptr = ptr::null();
            }
            odoo.get_file_mgr().borrow_mut().delete_path(odoo, &path);
        } else {
            odoo.add_to_init_odoo(self.symbol.clone());
        }
    }

    fn visit_stmt(&mut self, odoo: &mut SyncOdoo, stmt: &Stmt, file_info: &FileInfo) {
        match stmt {
            Stmt::Import(import_stmt) => {
                self.eval_local_symbols_from_import_stmt(odoo, &file_info, None, &import_stmt.names, None, &import_stmt.range)
            },
            Stmt::ImportFrom(import_from_stmt) => {
                self.eval_local_symbols_from_import_stmt(odoo, &file_info, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level.as_ref(), &import_from_stmt.range)
            },
            Stmt::ClassDef(class_stmt) => {
                self.visit_class_def(odoo, &file_info, class_stmt, stmt);
            },
            Stmt::FunctionDef(func_stmt) => {
                self.visit_func_def(func_stmt, stmt);
            }
            _ => {}
        }
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
        if from_stmt.is_some() && from_stmt.unwrap().to_string() == "_weakref" {
            println!("here");
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
                let (mut _sym, mut instance): (Weak<RefCell<Symbol>>, bool) = Symbol::follow_ref(_import_result.symbol.clone(), odoo, &mut None, false);
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
                            (_sym, instance) = Symbol::follow_ref(_import_result.symbol.clone(), odoo, &mut None, false);
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

    fn create_diagnostic_base_not_found(&mut self, odoo: &mut SyncOdoo, file_info: &FileInfo, symbol: &mut Symbol, var_name: &str, range: &TextRange) {
        let tree = symbol.get_tree();
        let tree = vec![tree.0.clone(), vec![var_name.to_string()]].concat();
        symbol.not_found_paths.push((BuildSteps::ARCH_EVAL, tree.clone()));
        odoo.not_found_symbols.insert(symbol.get_rc().unwrap());
        let range = file_info.text_range_to_range(range).unwrap();
        self.diagnostics.push(Diagnostic::new(
            range,
            Some(DiagnosticSeverity::WARNING),
            None,
            Some(EXTENSION_NAME.to_string()),
            format!("{} not found", tree.join(".")),
            None,
            None,
        ));
    }

    fn load_base_classes(&mut self, odoo: &mut SyncOdoo, file_info: &FileInfo, symbol: Rc<RefCell<Symbol>>, class_stmt: &StmtClassDef) {
        for base in class_stmt.bases.iter() {
            let full_base = PythonArchEval::extract_base_name(base);
            if full_base.len() == 0 {
                continue;
            }
            let elements = full_base.split(".").collect::<Vec<&str>>();
            let parent = symbol.borrow().parent.as_ref().unwrap().upgrade().unwrap();
            let mut parent = parent.borrow_mut();
            let iter_element = parent.infer_name(odoo, elements.first().unwrap().to_string(), Some(class_stmt.range));
            if iter_element.is_none() {
                self.create_diagnostic_base_not_found(odoo, file_info, &mut parent, elements[0], &base.range());
                continue;
            }
            drop(parent);
            let iter_element = iter_element.unwrap();
            let mut iter_element = Symbol::follow_ref(iter_element, odoo, &mut None, false).0;
            let mut previous_element = iter_element.clone();
            let mut found: bool = true;
            let mut compiled: bool = false;
            let mut last_element = elements.first().unwrap();
            for base_element in elements.iter().skip(1) {
                last_element = base_element;
                let iter_up = iter_element.upgrade().unwrap();
                if iter_up.borrow().sym_type == SymType::COMPILED {
                    compiled = true;
                }
                previous_element = iter_element.clone();
                let next_iter_element = iter_up.borrow().get_member_symbol(odoo, &base_element.to_string(), None, false, true, false);
                if next_iter_element.len() == 0 {
                    found = false;
                    break;
                }
                let iter_element_rc = next_iter_element.first().unwrap();
                iter_element = Symbol::follow_ref(iter_element_rc.clone(), odoo, &mut None, false).0;
            }
            if compiled {
                continue;
            }
            if !found {
                self.create_diagnostic_base_not_found(odoo, file_info, &mut (*previous_element.upgrade().unwrap()).borrow_mut(), last_element, &base.range());
                continue
            }
            if (*iter_element.upgrade().unwrap()).borrow().sym_type != SymType::CLASS {
                let range = file_info.text_range_to_range(&base.range()).unwrap();
                self.diagnostics.push(Diagnostic::new(
                    range,
                    Some(DiagnosticSeverity::WARNING),
                    None,
                    Some(EXTENSION_NAME.to_string()),
                    format!("Base class {} is not a class", elements.join(".")),
                    None,
                    None,
                ));
            }
        }
    }

    fn extract_base_name(base: &Expr) -> String {
        match base {
            Expr::Name(name) => {
                return name.id.to_string();
            },
            Expr::Attribute(attr) => {
                return PythonArchEval::extract_base_name(&attr.value) + "." + &attr.attr.to_string();
            },
            _ => {S!("")}
        }
    }

    fn visit_class_def(&mut self, odoo: &mut SyncOdoo, file_info: &FileInfo, class_stmt: &StmtClassDef, stmt: &Stmt) {
        let variable = self.symbol.borrow_mut().get_positioned_symbol(&class_stmt.name.to_string(), &class_stmt.range);
        if variable.is_none() {
            return;
        }
        variable.as_ref().unwrap().borrow_mut().ast_ptr = stmt as *const Stmt;
        self.load_base_classes(odoo, file_info, variable.unwrap(), class_stmt);
        for stmt in class_stmt.body.iter() {
            self.visit_stmt(odoo, stmt, file_info);
        }
    }

    fn visit_func_def(&mut self, func_stmt: &StmtFunctionDef,  stmt: &Stmt) {
        let variable = self.symbol.borrow_mut().get_positioned_symbol(&func_stmt.name.to_string(), &func_stmt.range);
        variable.as_ref().unwrap().borrow_mut().ast_ptr = stmt as *const Stmt;
    }
}