use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use crate::constants::{BuildStatus, BuildSteps, SymType};
use crate::core::evaluation::{AnalyzeAstResult, Context, ContextValue, Evaluation, ExprOrIdent};
use crate::core::odoo::SyncOdoo;
use crate::core::import_resolver::{resolve_from_stmt, resolve_import_stmt};
use crate::core::symbols::symbol::Symbol;
use crate::core::file_mgr::{FileInfo, FileInfoAst};
use crate::threads::SessionInfo;
use crate::S;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt, walk_alias, walk_except_handler, walk_parameter, walk_keyword, walk_pattern_keyword, walk_type_param, walk_pattern};
use ruff_python_ast::{Alias, AtomicNodeIndex, ExceptHandler, Expr, ExprCall, Identifier, Keyword, Parameter, Pattern, PatternKeyword, Stmt, TypeParam};
use ruff_text_size::{Ranged, TextRange, TextSize, TextSlice};
use tracing::warn;

pub struct AstUtils {}

impl AstUtils {


    pub fn get_symbols<'a>(session: &mut SessionInfo, file_info_ast: &'a FileInfoAst, file_symbol: &Rc<RefCell<Symbol>>, offset: u32) -> (AnalyzeAstResult, Option<TextRange>, Option<ExprOrIdent<'a>>, Option<ExprCall>) {
        let mut expr: Option<ExprOrIdent<'a>> = None;
        let mut call_expr: Option<ExprCall> = None;
        for stmt in file_info_ast.get_stmts().unwrap().iter() {
            //we have to handle imports differently as symbols are not visible in file.
            if let Some((result, range)) = Self::get_symbol_in_import(session, file_symbol, offset, stmt) {
                return (result, range, None, None);
            }
            (expr, call_expr) = ExprFinderVisitor::find_expr_at(stmt, offset);
            if expr.is_some() {
                break;
            }
        }
        let Some(expr) = expr else {
            warn!("expr not found");
            return (AnalyzeAstResult::default(), None, None, None);
        };
        let (result, range) = Self::get_symbol_from_expr(session, file_symbol, &expr, offset);
        (result, range, Some(expr), call_expr)
    }

    pub fn get_symbol_from_expr<'a>(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, expr: &ExprOrIdent<'a>, offset: u32) -> (AnalyzeAstResult, Option<TextRange>) {
        let parent_symbol = Symbol::get_scope_symbol(file_symbol.clone(), offset, matches!(expr, ExprOrIdent::Parameter(_)));
        AstUtils::build_scope(session, &parent_symbol);
        let from_module;
        if let Some(module) = file_symbol.borrow().find_module() {
            from_module = ContextValue::MODULE(Rc::downgrade(&module));
        } else {
            from_module = ContextValue::BOOLEAN(false);
        }
        let mut context: Option<Context> = Some(HashMap::from([
            (S!("module"), from_module),
            (S!("range"), ContextValue::RANGE(expr.range()))
        ]));
        let analyse_ast_result: AnalyzeAstResult = Evaluation::analyze_ast(session, &expr, parent_symbol.clone(), &expr.range().end(), &mut context,false, &mut vec![]);
        (analyse_ast_result, Some(expr.range()))
    }

    pub fn flatten_expr(expr: &Expr) -> String {
        match expr {
            Expr::Name(n) => {
                n.id.to_string()
            },
            Expr::Attribute(a) => {
                AstUtils::flatten_expr(&a.value) + &a.attr
            },
            _ => {S!("//Unhandled//")}
        }
    }

    pub fn build_scope(session: &mut SessionInfo<'_>, scope: &Rc<RefCell<Symbol>>) {
        if scope.borrow().typ() == SymType::FUNCTION {
            let parent_func = scope.borrow().get_in_parents(&vec![SymType::FUNCTION], true);
            let scope_to_test = parent_func.and_then(|w| w.upgrade());
            let scope_to_test = scope_to_test.as_ref().unwrap_or(scope);
            if scope_to_test.borrow().as_func().arch_status == BuildStatus::PENDING {
                SyncOdoo::build_now(session, scope_to_test, BuildSteps::ARCH);
            }
            if scope_to_test.borrow().as_func().arch_eval_status == BuildStatus::PENDING {
                SyncOdoo::build_now(session, scope_to_test, BuildSteps::ARCH_EVAL);
            }
        }
    }

    fn get_symbol_in_import(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, offset: u32, stmt: &Stmt) -> Option<(AnalyzeAstResult, Option<TextRange>)> {
        match stmt {
            //for all imports, the idea will be to check if we are on the last name of the import (then it has been imported already and we can fallback on it),
            //or then take the full tree to the offset symbol and resolve_import on it as it was in a 'from' clause.
            Stmt::Import(stmt) => {
                for alias in stmt.names.iter() {
                    if alias.range().contains(TextSize::new(offset)) {
                        let mut is_last = false;
                        let (to_analyze, range) = if alias.name.range().contains(TextSize::new(offset)) {
                            let next_dot_offset = alias.name.id.as_str()[offset as usize - alias.name.range().start().to_usize()..].find(".");
                            if let Some(next_dot_offset) = next_dot_offset {
                                let end = offset as usize + next_dot_offset;
                                let text = &alias.name.id.as_str()[..end - alias.name.range().start().to_usize()];
                                let start_range = text.rfind(".").map(|p| p+1).unwrap_or(0) + alias.name.range().start().to_usize();
                                (text, TextRange::new(TextSize::new(start_range as u32), TextSize::new(end as u32)))
                            } else {
                                is_last = true;
                                (alias.name.id.as_str(), alias.name.range())
                            }
                        } else if alias.asname.is_some() && alias.asname.as_ref().unwrap().range().contains(TextSize::new(offset)) {
                            is_last = true;
                            (alias.asname.as_ref().unwrap().id.as_str(), alias.asname.as_ref().unwrap().range())
                        } else {
                            return None;
                        };
                        if !is_last {
                            //we import as a from_stmt, to refuse import of variables, as the import stmt is not complete
                            let to_analyze = Identifier { id: Name::new(to_analyze), range: TextRange::new(TextSize::new(0), TextSize::new(0)), node_index: AtomicNodeIndex::default() };
                            let (from_symbol, _fallback_sym, _file_tree) = resolve_from_stmt(session, file_symbol, Some(&to_analyze), 0);
                            if let Some(symbol) = from_symbol {
                                let result = AnalyzeAstResult {
                                    evaluations: vec![Evaluation::eval_from_symbol(&Rc::downgrade(&symbol), None)],
                                    diagnostics: vec![],
                                };
                                return Some((result, Some(range)));
                            }
                        } else {
                            let res = resolve_import_stmt(session, file_symbol, None, &[
                                Alias { //create a dummy alias with a asname to force full import
                                    name: Identifier { id: Name::new(to_analyze), range: TextRange::new(TextSize::new(0), TextSize::new(0)), node_index: AtomicNodeIndex::default() },
                                    asname: Some(Identifier { id: Name::new("fake_name"), range: alias.name.range().clone(), node_index: AtomicNodeIndex::default() }),
                                    range: alias.range(),
                                    node_index: AtomicNodeIndex::default()
                                }], 0, &mut None);
                            let res = res.into_iter().filter(|s| s.found).collect::<Vec<_>>();
                            if !res.is_empty() {
                                let result = AnalyzeAstResult {
                                    evaluations: res.iter().map(
                                        |s| Evaluation::eval_from_symbol(&Rc::downgrade(&s.symbol), None)
                                    ).collect(),
                                    diagnostics: vec![],
                                };
                                return Some((result, Some(range)));
                            }
                        }
                        return None;
                    }
                }
            },
            Stmt::ImportFrom(stmt) => {
                //only check module as names are already supported by default ast walking and name resolution
                if stmt.module.is_some() && stmt.module.as_ref().unwrap().range().contains(TextSize::new(offset)) {
                    let module = stmt.module.as_ref().unwrap();
                    let (to_analyze, range) = if module.range().contains(TextSize::new(offset)) {
                        let next_dot_offset = module.id.as_str()[offset as usize - module.range().start().to_usize()..].find(".");
                        if let Some(next_dot_offset) = next_dot_offset {
                            let end = offset as usize + next_dot_offset;
                            let text = &module.id.as_str()[..end - module.range().start().to_usize()];
                            let start_range = text.rfind(".").map(|p| p+1).unwrap_or(0) + module.range().start().to_usize();
                            (text, TextRange::new(TextSize::new(start_range as u32), TextSize::new(end as u32)))
                        } else {
                            (module.id.as_str(), module.range())
                        }
                    } else {
                        return None;
                    };
                    let to_analyze = Identifier { id: Name::new(to_analyze), range: TextRange::new(TextSize::new(0), TextSize::new(0)), node_index: AtomicNodeIndex::default() };
                    let (from_symbol, _fallback_sym, _file_tree) = resolve_from_stmt(session, file_symbol, Some(&to_analyze), 0);
                    if let Some(symbol) = from_symbol {
                        let result = AnalyzeAstResult {
                            evaluations: vec![Evaluation::eval_from_symbol(&Rc::downgrade(&symbol), None)],
                            diagnostics: vec![],
                        };
                        return Some((result, Some(range)));
                    }
                }
            },
            _ => {
                return None;
            }
        }
        None
    }
}


pub struct ExprFinderVisitor<'a> {
    offset: TextSize,
    expr: Option<ExprOrIdent<'a>>,
    last_call_expr: Option<&'a ExprCall>,
}

impl<'a> ExprFinderVisitor<'a> {
    /*
    Find expr from `stmt` at the given `offset`
    Returns: (expr, last_call_expr)
        expr: the expr being searched for
        last_call_expr: The last call expr preceding the expr we are searching for
     */
    pub fn find_expr_at(stmt: &'a Stmt, offset: u32) -> (Option<ExprOrIdent<'a>>, Option<ExprCall>) {
        let mut visitor = Self {
            offset: TextSize::new(offset),
            expr: None,
            last_call_expr: None
        };
        visitor.visit_stmt(stmt);
        (visitor.expr, visitor.last_call_expr.cloned())
    }

}

impl<'a> Visitor<'a> for ExprFinderVisitor<'a> {

    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.range().contains(self.offset) {
            if let Expr::Call(expr_call) = expr {
                if expr_call.arguments.range().contains(self.offset){
                    self.last_call_expr = Some(expr_call);
                }
            }
            walk_expr(self, expr);
            if self.expr.is_none() {
                self.expr = Some(ExprOrIdent::Expr(expr));
            }
        } else {
            walk_expr(self, expr);
        }
    }

    fn visit_alias(&mut self, alias: &'a Alias) {
        walk_alias(self, alias);
        if self.expr.is_none() {
            if alias.name.range().contains(self.offset) {
                self.expr = Some(ExprOrIdent::Ident(&alias.name));
            } else if let Some(ref asname) = alias.asname {
                if asname.range().contains(self.offset) {
                    self.expr = Some(ExprOrIdent::Ident(asname))
                }
            }
        }
    }

    fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler) {
        walk_except_handler(self, except_handler);
        if self.expr.is_none() {
            let ExceptHandler::ExceptHandler(ref handler) = *except_handler;
            if let Some(ref ident) = handler.name {
                if ident.clone().range().contains(self.offset) {
                    self.expr = Some(ExprOrIdent::Ident(ident));
                }
            }
        } else {
            walk_except_handler(self, except_handler);
        }
    }

    fn visit_parameter(&mut self, parameter: &'a Parameter) {
        walk_parameter(self, parameter);
        if self.expr.is_none() && parameter.name.range().contains(self.offset) {
            self.expr = Some(ExprOrIdent::Parameter(parameter));
        }
    }

    fn visit_keyword(&mut self, keyword: &'a Keyword) {
        walk_keyword(self, keyword);

        if self.expr.is_none() {
            if let Some(ref ident) = keyword.arg {
                if ident.range().contains(self.offset) {
                    self.expr = Some(ExprOrIdent::Ident(ident));
                }
            }
        } else {
            walk_keyword(self, keyword)
        }
    }

    fn visit_pattern_keyword(&mut self, pattern_keyword: &'a PatternKeyword) {
        walk_pattern_keyword(self, pattern_keyword);

        if self.expr.is_none() && pattern_keyword.clone().attr.range().contains(self.offset) {
            self.expr = Some(ExprOrIdent::Ident(&pattern_keyword.attr));
        } else {
            walk_pattern_keyword(self, pattern_keyword);
        }
    }

    fn visit_type_param(&mut self, type_param: &'a TypeParam) {
        if type_param.range().contains(self.offset) {
            if self.expr.is_none() {
                walk_type_param(self, type_param);
                let ident = match type_param {
                    TypeParam::TypeVar(t) => Some(&t.name),
                    TypeParam::ParamSpec(t) => Some(&t.name),
                    TypeParam::TypeVarTuple(t) => Some(&t.name),
                };

                if ident.is_some() && ident.unwrap().range().contains(self.offset) {
                    self.expr = Some(ExprOrIdent::Ident(ident.unwrap()));
                }

            }
        } else {
            walk_type_param(self, type_param);
        }
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        if pattern.range().contains(self.offset) {
            if self.expr.is_none() {
                walk_pattern(self, pattern);
                let ident  = match pattern {
                    Pattern::MatchMapping(mapping) => &mapping.rest,
                    Pattern::MatchStar(mapping) => &mapping.name,
                    Pattern::MatchAs(mapping) => &mapping.name,
                    _ => &None
                };

                if let Some(ident) = ident {
                    if ident.range().contains(self.offset) {
                        self.expr = Some(ExprOrIdent::Ident(ident));
                    }
                }
            }
        }
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
        if self.expr.is_none() {
            let idents = match stmt {
                Stmt::FunctionDef(stmt) => vec![&stmt.name],
                Stmt::ClassDef(stmt) => vec![&stmt.name],
                Stmt::Global(stmt) => stmt.names.iter().collect(),
                Stmt::Nonlocal(stmt) => stmt.names.iter().collect(),
                _ => vec![],
            };

            for ident in idents {
                if ident.range().contains(self.offset) {
                    self.expr = Some(ExprOrIdent::Ident(ident));
                    break;
                }
            }
        }
    }
}

