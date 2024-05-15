use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::vec;

use ruff_text_size::TextRange;
use ruff_python_ast::{Alias, Expr, Identifier, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use weak_table::traits::WeakElement;
use std::path::PathBuf;

use crate::constants::*;
use crate::core::import_resolver::resolve_import_stmt;
use crate::core::odoo::SyncOdoo;
use crate::core::file_mgr::FileMgr;
use crate::core::symbol::Symbol;
use crate::core::evaluation::Evaluation;
use crate::core::python_utils;
use crate::S;

use super::config::DiagMissingImportsMode;
use super::import_resolver::ImportResult;
use super::python_arch_eval_hooks::PythonArchEvalHooks;


#[derive(Debug, Clone)]
pub struct PythonArchEval {
    sym_stack: Vec<Rc<RefCell<Symbol>>>,
    diagnostics: Vec<Diagnostic>,
    safe_import: Vec<bool>,
    ast_indexes: Vec<u16>,
}

impl PythonArchEval {
    pub fn new(symbol: Rc<RefCell<Symbol>>) -> PythonArchEval {
        PythonArchEval {
            sym_stack: vec![symbol],
            diagnostics: Vec::new(),
            safe_import: vec![false],
            ast_indexes: vec![],
        }
    }

    pub fn eval_arch(&mut self, odoo: &mut SyncOdoo) {
        //println!("eval arch");
        let mut symbol = self.sym_stack.first().unwrap().borrow_mut();
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
        let file_info_rc = odoo.get_file_mgr().borrow_mut().get_file_info(path.as_str());
        let file_info = (*file_info_rc).borrow();
        if file_info.ast.is_some() {
            for (index, stmt) in file_info.ast.as_ref().unwrap().iter().enumerate() {
                self.ast_indexes.push(index as u16);
                self.visit_stmt(odoo, stmt);
                self.ast_indexes.pop();
            }
        }
        drop(file_info);
        let mut file_info = (*file_info_rc).borrow_mut();
        file_info.replace_diagnostics(BuildSteps::ARCH_EVAL, self.diagnostics.clone());
        PythonArchEvalHooks::on_file_eval(odoo, self.sym_stack.first().unwrap().clone());
        let mut symbol = self.sym_stack.first().unwrap().borrow_mut();
        symbol.arch_eval_status = BuildStatus::DONE;
        if symbol.is_external {
            for sym in symbol.all_symbols(None, false) {
                sym.borrow_mut().ast_indexes = None;
            }
            drop(file_info);
            odoo.get_file_mgr().borrow_mut().delete_path(odoo, &path);
        } else {
            drop(symbol);
            odoo.add_to_init_odoo(self.sym_stack.first().unwrap().clone());
        }
    }

    fn visit_stmt(&mut self, odoo: &mut SyncOdoo, stmt: &Stmt) {
        match stmt {
            Stmt::Import(import_stmt) => {
                self.eval_local_symbols_from_import_stmt(odoo, None, &import_stmt.names, None, &import_stmt.range)
            },
            Stmt::ImportFrom(import_from_stmt) => {
                self.eval_local_symbols_from_import_stmt(odoo, import_from_stmt.module.as_ref(), &import_from_stmt.names, import_from_stmt.level, &import_from_stmt.range)
            },
            Stmt::ClassDef(class_stmt) => {
                self.visit_class_def(odoo, class_stmt, stmt);
            },
            Stmt::FunctionDef(func_stmt) => {
                self.visit_func_def(odoo, func_stmt);
            },
            Stmt::AnnAssign(ann_assign_stmt) => {
                self._visit_ann_assign(odoo, ann_assign_stmt);
            },
            Stmt::Assign(assign_stmt) => {
                self._visit_assign(odoo, assign_stmt);
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

    fn eval_local_symbols_from_import_stmt(&mut self, odoo: &mut SyncOdoo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], level: Option<u32>, range: &TextRange) {
        if name_aliases.len() == 1 && name_aliases[0].name.to_string() == "*" {
            return;
        }
        let import_results: Vec<ImportResult> = resolve_import_stmt(
            odoo,
            &self.sym_stack.first().unwrap(),
            &self.sym_stack.last().unwrap(),
            from_stmt,
            name_aliases,
            level,
            range);

        for _import_result in import_results.iter() {
            let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&_import_result.name, &_import_result.range);
            if variable.is_none() {
                continue;
            }
            if _import_result.found {
                //resolve the symbol and build necessary evaluations
                let (mut _sym, mut instance): (Weak<RefCell<Symbol>>, bool) = Symbol::follow_ref(_import_result.symbol.clone(), odoo, &mut None, false, &mut self.diagnostics);
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
                            (_sym, instance) = Symbol::follow_ref(_import_result.symbol.clone(), odoo, &mut None, false, &mut self.diagnostics);
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
                    self.sym_stack.first().unwrap().borrow_mut().not_found_paths.push((BuildSteps::ARCH_EVAL, file_tree.clone()));
                    odoo.not_found_symbols.insert(self.sym_stack.first().unwrap().clone());
                    if self._match_diag_config(odoo, &_import_result.symbol) {
                        self.diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(_import_result.range.start().to_u32(), 0), Position::new(_import_result.range.end().to_u32(), 0)),
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
                    self.sym_stack.first().unwrap().borrow_mut().not_found_paths.push((BuildSteps::ARCH_EVAL, file_tree.clone()));
                    odoo.not_found_symbols.insert(self.sym_stack.first().unwrap().clone());
                    if self._match_diag_config(odoo, &_import_result.symbol) {
                        self.diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(_import_result.range.start().to_u32(), 0), Position::new(_import_result.range.end().to_u32(), 0)),
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

    fn _visit_ann_assign(&mut self, odoo: &mut SyncOdoo, ann_assign_stmt: &StmtAnnAssign) {
        let assigns = match ann_assign_stmt.value.as_ref() {
            Some(value) => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), Some(value)),
            None => python_utils::unpack_assign(&vec![*ann_assign_stmt.target.clone()], Some(&ann_assign_stmt.annotation), None)
        };
        for assign in assigns.iter() { //should only be one
            let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&assign.target.id.to_string(), &assign.target.range);
            if let Some(variable) = variable {
                let parent = variable.borrow().parent.as_ref().unwrap().upgrade().unwrap().clone();
                if assign.annotation.is_some() {
                    let (eval, diags) = Evaluation::eval_from_ast(odoo, &assign.annotation.as_ref().unwrap(), parent, &ann_assign_stmt.range);
                    variable.borrow_mut().evaluation = eval;
                    self.diagnostics.extend(diags);
                } else if assign.value.is_some() {
                    let (eval, diags) = Evaluation::eval_from_ast(odoo, &assign.value.as_ref().unwrap(), parent, &ann_assign_stmt.range);
                    variable.borrow_mut().evaluation = eval;
                    self.diagnostics.extend(diags);
                } else {
                    panic!("either value or annotation should exists");
                }
                let mut v_mut = variable.borrow_mut();
                let mut sym = None;
                if let Some(eval) = &v_mut.evaluation {
                    if !eval.symbol.symbol.is_expired() {
                        sym = Some(eval.symbol.symbol.upgrade().unwrap());
                    }
                }
                if let Some(sym) = sym {
                    if sym.borrow().sym_type != SymType::CONSTANT && sym.borrow().parent.is_some() {
                        v_mut.add_dependency(&mut sym.borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
                    }
                }
            } else {
                println!("Symbol not found");
            }
        }
    }

    fn _visit_assign(&mut self, odoo: &mut SyncOdoo, assign_stmt: &StmtAssign) {
        let assigns = python_utils::unpack_assign(&assign_stmt.targets, None, Some(&assign_stmt.value));
        for assign in assigns.iter() {
            let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&assign.target.id.to_string(), &assign.target.range);
            if let Some(variable) = variable {
                let parent = variable.borrow().parent.as_ref().unwrap().upgrade().unwrap().clone();
                let (eval, diags) = Evaluation::eval_from_ast(odoo, &assign.value.as_ref().unwrap(), parent, &assign_stmt.range);
                variable.borrow_mut().evaluation = eval;
                self.diagnostics.extend(diags);
                let mut v_mut = variable.borrow_mut();
                let mut sym = None;
                if let Some(eval) = &v_mut.evaluation {
                    if !eval.symbol.symbol.is_expired() {
                        sym = Some(eval.symbol.symbol.upgrade().unwrap());
                    }
                }
                if let Some(sym) = sym {
                    if sym.borrow().sym_type != SymType::CONSTANT && sym.borrow().parent.is_some() {
                        v_mut.add_dependency(&mut sym.borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
                    }
                }
            } else {
                println!("Symbol not found");
            }
        }
    }

    fn create_diagnostic_base_not_found(&mut self, odoo: &mut SyncOdoo, symbol: &mut Symbol, var_name: &str, range: &TextRange) {
        let tree = symbol.get_tree();
        let tree = vec![tree.0.clone(), vec![var_name.to_string()]].concat();
        symbol.not_found_paths.push((BuildSteps::ARCH_EVAL, tree.clone()));
        odoo.not_found_symbols.insert(symbol.get_rc().unwrap());
        self.diagnostics.push(Diagnostic::new(
            Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
            Some(DiagnosticSeverity::WARNING),
            None,
            Some(EXTENSION_NAME.to_string()),
            format!("{} not found", tree.join(".")),
            None,
            None,
        ));
    }

    fn load_base_classes(&mut self, odoo: &mut SyncOdoo, symbol: &Rc<RefCell<Symbol>>, class_stmt: &StmtClassDef) {
        for base in class_stmt.bases() {
            let (full_base, range) = PythonArchEval::extract_base_name(base);
            if range.is_none() {
                continue;
            }
            let range = range.unwrap();
            let elements = full_base.split(".").collect::<Vec<&str>>();
            let parent = symbol.borrow().parent.as_ref().unwrap().upgrade().unwrap();
            let iter_element = Symbol::infer_name(odoo, &parent, &elements.first().unwrap().to_string(), Some(class_stmt.range));
            if iter_element.is_none() {
                let mut parent = parent.borrow_mut();
                self.create_diagnostic_base_not_found(odoo, &mut parent, elements[0], range);
                continue;
            }
            let iter_element = iter_element.unwrap();
            let mut iter_element = Symbol::follow_ref(iter_element, odoo, &mut None, false, &mut self.diagnostics).0;
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
                let next_iter_element = iter_up.borrow().get_member_symbol(odoo, &base_element.to_string(), None, false, true, false, &mut self.diagnostics);
                if next_iter_element.len() == 0 {
                    found = false;
                    break;
                }
                let iter_element_rc = next_iter_element.first().unwrap();
                iter_element = Symbol::follow_ref(iter_element_rc.clone(), odoo, &mut None, false, &mut self.diagnostics).0;
            }
            if compiled {
                continue;
            }
            if !found {
                self.create_diagnostic_base_not_found(odoo, &mut (*previous_element.upgrade().unwrap()).borrow_mut(), last_element, range);
                continue
            }
            if (*iter_element.upgrade().unwrap()).borrow().sym_type != SymType::CLASS {
                self.diagnostics.push(Diagnostic::new(
                    Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
                    Some(DiagnosticSeverity::WARNING),
                    None,
                    Some(EXTENSION_NAME.to_string()),
                    format!("Base class {} is not a class", elements.join(".")),
                    None,
                    None,
                ));
            } else {
                symbol.borrow_mut().add_dependency(&mut iter_element.upgrade().unwrap().borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
                symbol.borrow_mut()._class.as_mut().unwrap().bases.insert(iter_element.upgrade().unwrap());
            }
        }
    }

    fn extract_base_name(base: &Expr) -> (String, Option<&TextRange>) {
        match base {
            Expr::Name(name) => {
                return (name.id.to_string(), Some(&name.range));
            },
            Expr::Attribute(attr) => {
                let (mut name, range) = PythonArchEval::extract_base_name(&attr.value);
                name = name + "." + &attr.attr.to_string();
                return (name, range);
            },
            _ => {(S!(""), None)}
        }
    }

    fn visit_class_def(&mut self, odoo: &mut SyncOdoo, class_stmt: &StmtClassDef, stmt: &Stmt) {
        let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&class_stmt.name.to_string(), &class_stmt.range);
        if variable.is_none() {
            panic!("Class not found");
        }
        variable.as_ref().unwrap().borrow_mut().ast_indexes = Some(self.ast_indexes.clone());
        self.load_base_classes(odoo, variable.as_ref().unwrap(), class_stmt);
        self.sym_stack.push(variable.unwrap());
        for (index, stmt) in class_stmt.body.iter().enumerate() {
            self.ast_indexes.push(index as u16);
            self.visit_stmt(odoo, stmt);
            self.ast_indexes.pop();
        }
        self.sym_stack.pop();
    }

    fn visit_func_def(&mut self, odoo: &mut SyncOdoo, func_stmt: &StmtFunctionDef) {
        let variable = self.sym_stack.last().unwrap().borrow_mut().get_positioned_symbol(&func_stmt.name.to_string(), &func_stmt.range);
        if variable.is_none() {
            panic!("Function symbol not found");
        }
        let variable = variable.unwrap();
        variable.borrow_mut().ast_indexes = Some(self.ast_indexes.clone());
        {
            let inner_func = &variable.borrow()._function;
            if !inner_func.as_ref().unwrap().is_static {
                if self.sym_stack.last().unwrap().borrow().sym_type == SymType::CLASS {
                    if func_stmt.parameters.args.len() == 0 || variable.borrow().local_symbols.len() == 0 {
                        // self.diagnostics.push(Diagnostic::new(
                        //     FileMgr::textRange_to_temporary_Range(&func_stmt.range),
                        //     Some(DiagnosticSeverity::ERROR),
                        //     None,
                        //     None,
                        //     S!("Non-static method should have at least one parameter"),
                        //     None,
                        //     None
                        // ))
                    } else {
                        let var = variable.borrow();
                        let first_param = var.local_symbols.first().unwrap();
                        first_param.borrow_mut().evaluation = Some(Evaluation::eval_from_symbol(self.sym_stack.last().unwrap()));
                        first_param.borrow_mut().evaluation.as_mut().unwrap().symbol.instance = true;
                    }
                }
            }
        }
        self.sym_stack.push(variable);
        for (index, stmt) in func_stmt.body.iter().enumerate() {
            //we don't want to evaluate functions here, but in validator. We must only assign ast indexes
            match stmt {
                Stmt::FunctionDef(f) => {
                    self.ast_indexes.push(index as u16);
                    self.visit_func_def(odoo, f);
                    self.ast_indexes.pop();
                },
                Stmt::ClassDef(c) => {
                    self.ast_indexes.push(index as u16);
                    self.visit_class_def(odoo, c, stmt);
                    self.ast_indexes.pop();
                }
                _ => {}
            }
        }
        self.sym_stack.pop();
    }
}