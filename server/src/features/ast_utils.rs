use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use crate::core::evaluation::{AnalyzeAstResult, Context, ContextValue, Evaluation, ExprOrIdent};
use crate::core::symbols::symbol::Symbol;
use crate::core::file_mgr::FileInfo;
use crate::threads::SessionInfo;
use crate::S;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt, walk_alias, walk_except_handler, walk_parameter, walk_keyword, walk_pattern_keyword, walk_type_param, walk_pattern};
use ruff_python_ast::{Expr, Stmt, Alias, ExceptHandler, Parameter, Keyword, PatternKeyword, TypeParam, Pattern};
use ruff_text_size::{Ranged, TextRange, TextSize};
use tracing::warn;

pub struct AstUtils {}

impl AstUtils {

    pub fn get_symbols(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, offset: u32) -> (AnalyzeAstResult, Option<TextRange>) {
        let mut expr: Option<ExprOrIdent> = None;
        let file_info_borrowed = file_info.borrow();
        for stmt in file_info_borrowed.ast.as_ref().unwrap().iter() {
            expr = ExprFinderVisitor::find_expr_at(stmt, offset);
            if expr.is_some() {
                break;
            }
        }
        if expr.is_none() {
            warn!("expr not found");
            return (AnalyzeAstResult::default(), None);
        }
        let expr = expr.unwrap();
        let parent_symbol = Symbol::get_scope_symbol(file_symbol.clone(), offset, matches!(expr, ExprOrIdent::Parameter(_)));
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
        let analyse_ast_result: AnalyzeAstResult = Evaluation::analyze_ast(session, &expr, parent_symbol.clone(), &expr.range().end(), &mut context);
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

    pub fn find_stmt_from_ast<'a>(ast: &'a [Stmt], indexes: &[u16]) -> &'a Stmt {
        let mut stmt = ast.get(indexes[0] as usize).expect("index not found in ast");
        let mut i_index = 1;
        while i_index < indexes.len() {
            match stmt {
                Stmt::ClassDef(c) => {
                    stmt = c.body.get(*indexes.get(i_index).unwrap() as usize).expect("index not found in ast");
                },
                Stmt::FunctionDef(f) => {
                    stmt = f.body.get(*indexes.get(i_index).unwrap() as usize).expect("index not found in ast");
                },
                Stmt::If(if_stmt) => {
                    let bloc = indexes.get(i_index).unwrap();
                    i_index += 1;
                    let stmt_index = indexes.get(i_index).unwrap();
                    if *bloc == 0 {
                        stmt = if_stmt.body.get(*stmt_index as usize).expect("index not found in ast");
                    } else {
                        stmt = if_stmt.elif_else_clauses.get((bloc-1) as usize).expect("Bloc not found in if stmt").body.get(*stmt_index as usize).expect("index not found in ast");
                    }
                },
                Stmt::Try(try_stmt) => {
                    let first_index = indexes.get(i_index).unwrap();
                    i_index += 1;
                    let second_index = indexes.get(i_index).unwrap();
                    if *first_index == 0 {
                        stmt = try_stmt.body.get(*second_index as usize).expect("index not found in ast");
                    } else if *first_index == 1 {
                        stmt = try_stmt.orelse.get(*second_index as usize).expect("index not found in ast");
                    } else if *first_index == 2 {
                        stmt = try_stmt.finalbody.get(*second_index as usize).expect("index not found in ast");
                    } else if *first_index == 3 {
                        i_index += 1;
                        let third_index = indexes.get(i_index).unwrap();
                        let handler = try_stmt.handlers.get(*second_index as usize).expect("Handler not found in ast");
                        stmt = handler.as_except_handler().unwrap().body.get(*third_index as usize).expect("index not found in handler ast");
                    } else {
                        panic!("Wrong try bloc");
                    }
                },
                Stmt::For(for_stmt) => {
                    let bloc = indexes.get(i_index).unwrap();
                    i_index += 1;
                    let stmt_index = indexes.get(i_index).unwrap();
                    if *bloc == 0 {
                        stmt = for_stmt.body.get(*stmt_index as usize).expect("index not found in ast");
                    } else if *bloc == 1 {
                        stmt = for_stmt.orelse.get(*stmt_index as usize).expect("index not found in ast");
                    } else {
                        panic!("Wrong for bloc");
                    }
                },
                Stmt::With(with_stmt) => {
                    stmt = with_stmt.body.get(*indexes.get(i_index).unwrap() as usize).expect("index not found in with stmt");
                }
                _ => {}
            }
            i_index += 1;
        }
        stmt
    }

}

pub struct ExprFinderVisitor<'a> {
    offset: TextSize,
    expr: Option<ExprOrIdent<'a>>,
}

impl<'a> ExprFinderVisitor<'a> {

    pub fn find_expr_at(stmt: &'a Stmt, offset: u32) -> Option<ExprOrIdent> {
        let mut visitor = Self {
            offset: TextSize::new(offset),
            expr: None
        };
        visitor.visit_stmt(stmt);
        visitor.expr
    }

}

impl<'a> Visitor<'a> for ExprFinderVisitor<'a> {

    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.range().contains(self.offset) {
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

                if let Some(ref ident) = ident {
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
                Stmt::ImportFrom(stmt) => if let Some(ref module) = stmt.module {vec![module]} else {vec![]},
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
        } else {
            walk_stmt(self, stmt);
        }
    }
}
