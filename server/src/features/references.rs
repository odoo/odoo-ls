use std::rc::Weak;
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use lsp_types::{Location, Range};
use ruff_python_ast::{Alias, Expr, Identifier, Stmt, StmtAnnAssign, StmtAssert, StmtAssign, StmtAugAssign, StmtClassDef, StmtIf, StmtMatch, StmtRaise, StmtReturn, StmtTry, StmtTypeAlias, StmtWith};
use ruff_text_size::{Ranged, TextRange, TextSize};
use weak_table::PtrWeakHashSet;

use crate::{S, Sy};
use crate::constants::OYarn;
use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr};
use crate::core::odoo::SyncOdoo;
use crate::features::goto_utils::{GotoRequest, GotoSourceType, GotoUtils};
use crate::{constants::SymType, core::{file_mgr::{FileInfo, FileMgr}, symbols::symbol::Symbol}, features::xml_ast_utils::{XmlAstResult, XmlAstUtils}, threads::SessionInfo, utils::PathSanitizer};

pub struct ReferenceFeature {

}

impl ReferenceFeature {
    /// Basic find all references implementation
    /// Same-file references only, local vars, params, methods
    /// TODO: Cross-file references within module
    /// TODO: All files within workspace
    /// TODO: Odoo specific (XML field refs, string-based model refs)
    pub fn get_references(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Vec<Location>> {
        //We want to search for references of the definition, and not the current symbol. Let's use definition feature for that
        SyncOdoo::process_rebuilds(session, false);
        let def_sources = GotoUtils::get_symbols(session, GotoRequest::Definition, file_symbol, file_info, line, character);

        let mut locations = Vec::new();
        for definition in def_sources.iter() {
            match &definition.source {
                GotoSourceType::Symbol(target_symbol) => {
                    let mut to_check: PtrWeakHashSet<Weak<RefCell<Symbol>>> = PtrWeakHashSet::new();

                    to_check.insert(file_symbol.clone());

                    //take arch and arch_eval dependents
                    if !file_symbol.borrow().dependents().is_empty() { // file could be out of workspace
                        for dep in file_symbol.borrow().dependents()[0].iter().take(2) {
                            if let Some(dep_set) = dep {
                                for dep_symbol_rc in dep_set.iter() {
                                    to_check.insert(dep_symbol_rc.clone());
                                }
                            }
                        }
                    }
                    //If the symbol is a model, browse model dependents too
                    if target_symbol.borrow().typ() == SymType::CLASS {
                        if let Some(model_data) = target_symbol.borrow().as_class_sym()._model.as_ref() {
                            if let Some(model) = session.sync_odoo.models.get(&model_data.name).cloned() {
                                to_check.extend(model.borrow().dependents.clone());
                                for symbol in model.borrow().all_symbols(session, None, false) {
                                    to_check.insert(symbol.0);
                                }
                            }
                        }
                    }
                    let mut files_to_check: PtrWeakHashSet<Weak<RefCell<Symbol>>> = PtrWeakHashSet::new(); //only iter on files
                    for sym in to_check.iter() {
                        let Some(Some(file)) = sym.borrow().get_file().map(|x| x.upgrade()) else { //to be sure we are on a file
                            continue;
                        };
                        files_to_check.insert(file.clone());
                    }
                    for file in files_to_check.iter() {
                        let Some(dep_file_info) = session.sync_odoo.get_file_mgr().borrow().get_file_info(&file.borrow().paths()[0]) else {
                            continue;
                        };
                        locations.extend(ReferenceFeature::references_in_file(session, &file, &dep_file_info, &target_symbol));
                    }
                },
                GotoSourceType::Module(m) => {
                    let module_name = Sy!(m.borrow().name().clone());
                    let modules = session.sync_odoo.modules.clone();
                    for (_module_name, module) in modules.iter() {
                        if let Some(module) = module.upgrade() {
                            if module.borrow().name() == &module_name {
                                locations.extend(ReferenceFeature::find_name_in_manifest(session, module.clone()));
                            }
                            if module.borrow().as_module_package().depends.iter().any(|(dep, _range)| dep == &module_name) {
                                locations.extend(ReferenceFeature::find_depend_in_manifest(session, module.clone(), &S!(module_name.clone())));
                            }
                        }
                    }
                },
                GotoSourceType::OdooData(data) => {

                }
            }
        }

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    fn references_in_file(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, target_symbol_rc: &Rc<RefCell<Symbol>>) -> Vec<Location> {
        let file_info_ast = file_info.borrow().file_info_ast.clone();
        let mut visitor = ReferenceVisitor {
            sym_stack: vec![],
        };
        session.sync_odoo.evaluation_locations = vec![];
        session.sync_odoo.evaluation_search = Some(target_symbol_rc.clone());
        visitor.browse_file(session, file_symbol, file_info_ast.borrow().get_stmts().as_ref().unwrap());
        session.sync_odoo.evaluation_search = None;
        std::mem::take(&mut session.sync_odoo.evaluation_locations)
    }

    //the name inside the manifest will be different, so we only search for the key in this module and return the location with the range
    fn find_name_in_manifest(session: &mut SessionInfo, module: Rc<RefCell<Symbol>>) -> Vec<Location> {
        let mut locations = Vec::new();
        let manifest = PathBuf::from(module.borrow().paths()[0].clone()).join("__manifest__.py").sanitize();
        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest).clone();
        if let Some(file_info) = file_info {
            let file_info_ast = file_info.borrow().file_info_ast.clone();
            let file_info_ast = file_info_ast.borrow();
            if let Some(stmts) = file_info_ast.get_stmts() {
                if stmts.len() != 1 {
                    return locations;
                }
                let Stmt::Expr(e) = &stmts[0] else {return locations;};
                let Expr::Dict(d) = &*e.value else {return locations;};
                for dict_item in d.items.iter() {
                    let Some(key) = dict_item.key.as_ref() else {continue;};
                    let value = &dict_item.value;
                    let Expr::StringLiteral(key_s) = key else {continue;};
                    let Expr::StringLiteral(value_s) = value else {continue;};
                    if key_s.value.to_str() == "name" {
                        let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &manifest, &value_s.range());
                        locations.push(Location {
                            uri: FileMgr::pathname2uri(&manifest),
                            range,
                        });
                    }
                }
            }
        }
        locations
    }

    fn find_depend_in_manifest(session: &mut SessionInfo, module: Rc<RefCell<Symbol>>, module_name: &String) -> Vec<Location> {
        let mut locations = Vec::new();
        let manifest = PathBuf::from(module.borrow().paths()[0].clone()).join("__manifest__.py").sanitize();
        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest).clone();
        if let Some(file_info) = file_info {
            let file_info_ast = file_info.borrow().file_info_ast.clone();
            let file_info_ast = file_info_ast.borrow();
            if let Some(stmts) = file_info_ast.get_stmts() {
                if stmts.len() != 1 {
                    return locations;
                }
                let Stmt::Expr(e) = &stmts[0] else {return locations;};
                let Expr::Dict(d) = &*e.value else {return locations;};
                for dict_item in d.items.iter() {
                    let Some(key) = dict_item.key.as_ref() else {continue;};
                    let value = &dict_item.value;
                    let Expr::StringLiteral(key_s) = key else {continue;};
                    let Expr::List(depends_d) = value else {continue;};
                    if key_s.value.to_str() == "depends" {
                        for value in depends_d.elts.iter() {
                            let Expr::StringLiteral(value_s) = value else {continue;};
                            if value_s.value.to_str() == module_name {
                                let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &manifest, &value_s.range());
                                locations.push(Location {
                                    uri: FileMgr::pathname2uri(&manifest),
                                    range,
                                });
                            }
                        }
                    }
                }
            }
        }
        locations
    }
}

struct ReferenceVisitor {
    sym_stack: Vec<Rc<RefCell<Symbol>>>,
}

impl ReferenceVisitor {

    pub fn browse_file(&mut self, session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, stmts: &Vec<Stmt>) {
        self.sym_stack.push(file_symbol.clone());
        self.visit_vec_stmt(session, stmts);
    }

    pub fn visit_vec_stmt(&mut self, session: &mut SessionInfo, vec_ast: &Vec<Stmt>) {
        for stmt in vec_ast.iter() {
            match stmt {
                Stmt::FunctionDef(f) => {
                    let sym = self.sym_stack.last().unwrap().borrow().get_positioned_symbol(&OYarn::from(f.name.to_string()), &f.range);
                    if let Some(sym) = sym {
                        self.sym_stack.push(sym);
                        self.visit_vec_stmt(session, &f.body);
                        self.sym_stack.pop();
                    }
                },
                Stmt::ClassDef(c) => {
                    self.visit_class_def(session, c);
                },
                Stmt::Try(t) => {
                    self.visit_try(session, t);
                },
                Stmt::Import(i) => {
                    self._resolve_import(session, None, &i.names, None, &i.range);
                },
                Stmt::ImportFrom(i) => {
                    self._resolve_import(session, i.module.as_ref(), &i.names, Some(i.level), &i.range);
                },
                Stmt::Assign(a) => {
                    self.visit_assign(session, a);
                },
                Stmt::AnnAssign(a) => {
                    self.visit_ann_assign(session, a);
                },
                Stmt::Expr(e) => {
                    self.visit_expr(session, &e.value, &e.value.start());
                },
                Stmt::If(i) => {
                    self.visit_if(session, i);
                },
                Stmt::Break(_) => {},
                Stmt::Continue(_) => {},
                Stmt::Delete(d) => {
                    for target in d.targets.iter() {
                        self.visit_expr(session, target, &target.start());
                    }
                },
                Stmt::For(f) => {
                    self.visit_expr(session, &f.target, &f.target.start());
                    self.visit_vec_stmt(session, &f.body);
                    self.visit_vec_stmt(session, &f.orelse);
                },
                Stmt::While(w) => {
                    self.visit_expr(session, &w.test, &w.test.start());
                    self.visit_vec_stmt(session, &w.body);
                    self.visit_vec_stmt(session, &w.orelse);
                },
                Stmt::Return(stmt_return) => self.visit_return_stmt(session, stmt_return),
                Stmt::AugAssign(stmt_aug_assign) => self.visit_aug_assign(session, stmt_aug_assign),
                Stmt::TypeAlias(stmt_type_alias) => self.visit_type_alias(session, stmt_type_alias),
                Stmt::With(stmt_with) => self.visit_with(session, stmt_with),
                Stmt::Match(stmt_match) => self.visit_match(session, stmt_match),
                Stmt::Raise(stmt_raise) => self.visit_raise(session, stmt_raise),
                Stmt::Assert(stmt_assert) => self.visit_assert(session, stmt_assert),
                Stmt::Global(_) => {},
                Stmt::Nonlocal(_) => {},
                Stmt::Pass(_) => {},
                Stmt::IpyEscapeCommand(_) => {},
            }
        };
    }

    fn _resolve_import(&mut self, session: &mut SessionInfo, _from_stmt: Option<&Identifier>, name_aliases: &[Alias], _level: Option<u32>, _range: &TextRange) {
        let file_symbol = self.sym_stack[0].borrow().get_file();
        let file_symbol = file_symbol.expect("file symbol not found").upgrade().expect("unable to upgrade file symbol");
        let Some(eval_search) = session.sync_odoo.evaluation_search.clone() else {
            return;
        };
        for alias in name_aliases.iter() {
            if alias.name.id == "*" {
                continue;
            }
            let var_name = if alias.asname.is_none() {
                S!(alias.name.split(".").next().unwrap())
            } else {
                alias.asname.as_ref().unwrap().clone().to_string()
            };
            let variable = self.sym_stack.last().unwrap().borrow().get_positioned_symbol(&OYarn::from(var_name), &alias.range);
            if let Some(variable) = variable {
                for evaluation in variable.borrow().evaluations().as_ref().unwrap().iter() {
                    let eval_sym = evaluation.symbol.get_symbol(session, &mut None, &mut vec![], Some(file_symbol.clone()));
                    match eval_sym {
                        EvaluationSymbolPtr::WEAK(w) => {
                            if let Some(symbol) = w.weak.upgrade() {
                                if Rc::ptr_eq(&symbol, &eval_search) {
                                    let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &file_symbol.borrow().paths()[0], &alias.range);
                                    session.sync_odoo.evaluation_locations.push(Location {
                                        uri: FileMgr::pathname2uri(&file_symbol.borrow().paths()[0]),
                                        range: range,
                                    });
                                }
                            }
                        },
                        _ => {
                            panic!("Internal error: evaluated has invalid evaluationType");
                        }
                    }
                }
            }
        }
    }

    fn visit_class_def(&mut self, session: &mut SessionInfo, c: &StmtClassDef) {
        let sym = self.sym_stack.last().unwrap().borrow().get_positioned_symbol(&OYarn::from(c.name.to_string()), &c.range);
        if let Some(sym) = sym {
            self.sym_stack.push(sym);
            self.visit_vec_stmt(session, &c.body);
            self.sym_stack.pop();
        }
    }

    fn visit_expr(&mut self, session: &mut SessionInfo, expr: &Expr, max_infer: &TextSize) {
        Evaluation::eval_from_ast(session, expr, self.sym_stack.last().unwrap().clone(), max_infer, false, &mut vec![]);
    }

    fn visit_assert(&mut self, session: &mut SessionInfo, stmt_assert: &StmtAssert) {
        self.visit_expr(session, &stmt_assert.test, &stmt_assert.range.start());
        if let Some(msg) = stmt_assert.msg.as_ref() {
            self.visit_expr(session, msg, &stmt_assert.range.start());
        }
    }

    fn visit_raise(&mut self, session: &mut SessionInfo, stmt_raise: &StmtRaise) {
        if let Some(exc) = stmt_raise.exc.as_ref() {
            self.visit_expr(session, exc, &stmt_raise.range.start());
        }
    }

    fn visit_match(&mut self, session: &mut SessionInfo, stmt_match: &StmtMatch) {
        self.visit_expr(session, &stmt_match.subject, &stmt_match.range.start());
        for case in stmt_match.cases.iter() {
            if let Some(guard) = case.guard.as_ref() {
                self.visit_expr(session, guard, &case.pattern.start());
            }
            self.visit_vec_stmt(session, &case.body);
        }
    }

    fn visit_type_alias(&mut self, session: &mut SessionInfo, stmt_type_alias: &StmtTypeAlias) {
        self.visit_expr(session, &stmt_type_alias.value, &stmt_type_alias.range.start());
    }

    fn visit_aug_assign(&mut self, session: &mut SessionInfo, assign: &StmtAugAssign) {
        self.visit_expr(session, &assign.value, &assign.range.start());
    }

    fn visit_return_stmt(&mut self, session: &mut SessionInfo, stmt_return: &StmtReturn) {
        if let Some(value) = stmt_return.value.as_ref() {
            self.visit_expr(session, value, &stmt_return.range.start());
        }
    }

    fn visit_if(&mut self, session: &mut SessionInfo, node: &StmtIf) {
        self.visit_expr(session, &node.test, &node.test.start());
        self.visit_vec_stmt(session, &node.body);
        for elses in node.elif_else_clauses.iter() {
            if let  Some(test) = &elses.test {
                self.visit_expr(session, test, &test.start());
            }
            self.visit_vec_stmt(session, &elses.body);
        }
    }

    fn visit_with(&mut self, session: &mut SessionInfo, stmt_with: &StmtWith) {
        for item in stmt_with.items.iter() {
            self.visit_expr(session, &item.context_expr, &stmt_with.range.start());
        }
        self.visit_vec_stmt(session, &stmt_with.body);
    }

    fn visit_assign(&mut self, session: &mut SessionInfo, assign: &StmtAssign) {
        self.visit_expr(session, &assign.value, &assign.range.start());
    }

    fn visit_ann_assign(&mut self, session: &mut SessionInfo, assign: &StmtAnnAssign) {
        if let Some(value) = assign.value.as_ref() {
            self.visit_expr(session, value, &assign.range.start());
        }
    }

    fn visit_try(&mut self, session: &mut SessionInfo, node: &StmtTry) {
        //TODO handle handlers of try
        self.visit_vec_stmt(session, &node.body);
    }
}