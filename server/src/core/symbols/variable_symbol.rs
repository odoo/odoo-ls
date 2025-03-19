use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
use ruff_python_ast::{Alias, Identifier};
use ruff_text_size::TextRange;

use crate::{constants::{flatten_tree, BuildStatus, BuildSteps, BUILT_IN_LIBS, EXTENSION_NAME}, core::{config::DiagMissingImportsMode, entry_point::EntryPoint, evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak}, import_resolver::{resolve_import_stmt, ImportResult}, odoo::SyncOdoo, python_arch_eval::PythonArchEval}, threads::SessionInfo, S};
use std::{cell::RefCell, rc::{Rc, Weak}};

use super::symbol::Symbol;

#[derive(Debug)]
pub struct ImportInformation {
    pub from: Option<Identifier>,
    pub alias: Alias,
    pub level: Option<u32>,
    pub import_step: BuildSteps,
}

#[derive(Debug)]
pub struct VariableSymbol {
    pub name: String,
    pub is_external: bool,
    pub doc_string: Option<String>,
    pub ast_indexes: Vec<u16>, //list of index to reach the corresponding ast node from file ast
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub is_import_variable: bool,
    pub import_information: Option<ImportInformation>,
    pub is_parameter: bool,
    pub evaluations: Vec<Evaluation>, //Vec, because sometimes a single allocation can be ambiguous, like ''' a = "5" if X else 5 '''
    pub range: TextRange,
}

impl VariableSymbol {

    pub fn new(name: String, range: TextRange, is_external: bool) -> Self {
        Self {
            name,
            is_external,
            doc_string: None,
            ast_indexes: vec![],
            weak_self: None,
            parent: None,
            range,
            is_import_variable: false,
            import_information: None,
            is_parameter: false,
            evaluations: vec![],
        }
    }

    pub fn is_type_alias(&self) -> bool {
        //TODO it does not use get_symbol call, and only evaluate "sym" from EvaluationSymbol
        return self.evaluations.len() >= 1 && self.evaluations.iter().all(|x| !x.symbol.is_instance().unwrap_or(true)) && !self.is_import_variable;
    }
    
    /* If this variable has been evaluated to a relational field, return the main symbol of the comodel */
    pub fn get_relational_model(&self, session: &mut SessionInfo, from_module: Option<Rc<RefCell<Symbol>>>) -> Vec<Rc<RefCell<Symbol>>> {
        for eval in self.evaluations.iter() {
            let symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let eval_weaks = Symbol::follow_ref(&symbol, session, &mut None, true, false, None, &mut vec![]);
            for eval_weak in eval_weaks.iter() {
                if let Some(symbol) = eval_weak.upgrade_weak() {
                    if ["Many2one", "One2many", "Many2many"].contains(&symbol.borrow().name().as_str()) {
                        let Some(comodel) = eval_weak.as_weak().context.get("comodel") else {
                            continue;
                        };
                        let Some(model) = session.sync_odoo.models.get(&comodel.as_string()).cloned() else {
                            continue;
                        };
                        return model.borrow().get_main_symbols(session, from_module);
                    }
                }
            }
        }
        vec![]
    }
    
    ///Follow the evaluations of sym_ref, evaluate files if needed, and return true if the end evaluation contains from_sym
    fn check_for_loop_evaluation(session: &mut SessionInfo, sym_ref: Rc<RefCell<Symbol>>, from_sym: &Rc<RefCell<Symbol>>, entry_point: &Rc<RefCell<EntryPoint>>, diagnostics: &mut Vec<Diagnostic>) -> bool {
        let sym_ref_cl = sym_ref.clone();
        let syms_followed = Symbol::follow_ref(&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
            Rc::downgrade(&sym_ref_cl), None, false
        )), session, &mut None, false, false, None, diagnostics);
        for sym in syms_followed.iter() {
            let sym = sym.upgrade_weak();
            if let Some(sym) = sym {
                if sym.borrow().evaluations().is_some() && sym.borrow().evaluations().unwrap().is_empty() {
                    let file_sym = sym_ref.borrow().get_file();
                    if file_sym.is_some() {
                        let rc_file_sym = file_sym.as_ref().unwrap().upgrade().unwrap();
                        if rc_file_sym.borrow_mut().build_status(BuildSteps::ARCH_EVAL) == BuildStatus::PENDING && session.sync_odoo.is_in_rebuild(&rc_file_sym, BuildSteps::ARCH_EVAL) {
                            session.sync_odoo.remove_from_rebuild_arch_eval(&rc_file_sym);
                            let mut builder = PythonArchEval::new(entry_point.clone(), rc_file_sym);
                            builder.eval_arch(session);
                            if VariableSymbol::check_for_loop_evaluation(session, sym_ref.clone(), from_sym, entry_point, diagnostics) {
                                return true;
                            }
                        }
                    }
                }
                if Rc::ptr_eq(&sym, &from_sym) {
                    return true;
                }
            }
        }
        false
    }

    fn _match_diag_config(odoo: &mut SyncOdoo, symbol: &Rc<RefCell<Symbol>>) -> bool {
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

    pub fn load_from_import_information(session: &mut SessionInfo, variable: Rc<RefCell<Symbol>>, file: &Rc<RefCell<Symbol>>, entry_point: &Rc<RefCell<EntryPoint>>, diagnostics: &mut Vec<Diagnostic>) {
        let Some(import_info) = variable.borrow_mut().as_variable_mut().import_information.take() else {
            return;
        };
        let import_results: Vec<ImportResult> = resolve_import_stmt(
            session,
            file,
            import_info.from.as_ref(),
            &[import_info.alias],
            import_info.level,
            &mut Some(diagnostics));

        for _import_result in import_results.iter() {
            if _import_result.found {
                let import_sym_ref = _import_result.symbol.clone();
                let has_loop = VariableSymbol::check_for_loop_evaluation(session, import_sym_ref, &variable, entry_point, diagnostics);
                if !has_loop { //anti-loop. We want to be sure we are not evaluating to the same sym
                    variable.borrow_mut().set_evaluations(vec![Evaluation::eval_from_symbol(&Rc::downgrade(&_import_result.symbol), None)]);
                    //let's not set dependencies as import_information are only set for files outside of workspace that doesn't have
                    //any file watcher and so can't be updated, only reloaded
                    // let file_of_import_symbol = _import_result.symbol.borrow().get_file();
                    // if let Some(import_file) = file_of_import_symbol {
                    //     let import_file = import_file.upgrade().unwrap();
                    //     if !Rc::ptr_eq(file, &import_file) {
                    //         file.borrow_mut().add_dependency(&mut import_file.borrow_mut(), self.current_step, BuildSteps::ARCH);
                    //     }
                    // }
                } else {
                    let mut file_tree = [_import_result.file_tree.0.clone(), _import_result.file_tree.1.clone()].concat();
                    file_tree.extend(_import_result.name.split(".").map(str::to_string));
                    file.borrow_mut().not_found_paths_mut().push((import_info.import_step, file_tree.clone()));
                    entry_point.borrow_mut().not_found_symbols.insert(file.clone());
                    if VariableSymbol::_match_diag_config(session.sync_odoo, &_import_result.symbol) {
                        diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(_import_result.range.start().to_u32(), 0), Position::new(_import_result.range.end().to_u32(), 0)),
                            Some(DiagnosticSeverity::WARNING),
                            Some(NumberOrString::String(S!("OLS20004"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("Failed to evaluate import {}", file_tree.clone().join(".")),
                            None,
                            None,
                        ));
                    }
                }

            } else {
                let mut file_tree = [_import_result.file_tree.0.clone(), _import_result.file_tree.1.clone()].concat();
                file_tree.extend(_import_result.name.split(".").map(str::to_string));
                if BUILT_IN_LIBS.contains(&file_tree[0].as_str()) {
                    continue;
                }
                file.borrow_mut().not_found_paths_mut().push((import_info.import_step, file_tree.clone()));
                entry_point.borrow_mut().not_found_symbols.insert(file.clone());
                //No need to set not found diagnostic, as we are not in workspace
            }
        }
    }

}