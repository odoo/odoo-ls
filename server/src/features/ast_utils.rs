use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::core::evaluation::{AnalyzeAstResult, Context, Evaluation};
use crate::core::symbol::Symbol;
use crate::core::file_mgr::FileInfo;
use crate::core::odoo::SyncOdoo;
use tower_lsp::lsp_types::Diagnostic;
use ruff_python_ast::visitor::{Visitor, walk_expr};
use ruff_python_ast::Expr;
use ruff_python_ast::Stmt;
use ruff_text_size::{Ranged, TextRange};

pub struct AstUtils {}

impl AstUtils {

    pub fn get_symbols(odoo: &mut SyncOdoo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, offset: u32) -> (AnalyzeAstResult, Option<TextRange>) {
        let parent_symbol = Symbol::get_scope_symbol(file_symbol.clone(), offset);
        let mut expr: Option<&Expr> = None;
        let file_info_borrowed = file_info.borrow();
        for stmt in file_info_borrowed.ast.as_ref().unwrap().iter() {
            expr = ExprFinderVisitor::find_expr_at(stmt, offset);
            if expr.is_some() {
                break;
            }
        }
        if expr.is_none() {
            println!("expr not found");
            return (AnalyzeAstResult::default(), None);
        }
        let expr = expr.unwrap();
        let analyse_ast_result: AnalyzeAstResult = Evaluation::analyze_ast(odoo, expr, parent_symbol, &expr.range());
        return (analyse_ast_result, Some(expr.range()));
    }

}

struct ExprFinderVisitor<'a> {
    offset: u32,
    expr: Option<&'a Expr>
}

impl<'a> ExprFinderVisitor<'a> {

    pub fn find_expr_at(stmt: &'a Stmt, offset: u32) -> Option<&Expr> {
        let mut visitor = Self {
            offset: offset,
            expr: None
        };
        visitor.visit_stmt(stmt);
        visitor.expr
    }

}

impl<'a> Visitor<'a> for ExprFinderVisitor<'a> {

    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.range().start().to_u32() <= self.offset && expr.range().end().to_u32() >= self.offset {
            walk_expr(self, expr);
            if self.expr.is_none() { //do not put expr if inner expr is valid
                self.expr = Some(expr);
            }
        } else {
            walk_expr(self, expr);
        }
    }

}