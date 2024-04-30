use std::rc::Rc;
use std::cell::RefCell;
use crate::core::symbol::Symbol;
use crate::core::file_mgr::FileInfo;
use ruff_python_ast::Expr;
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

pub struct AstUtils {}

impl AstUtils {

    pub fn get_symbols(file_symbol: Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, offset: u32) {
        let range = None;
        let scope = Symbol::get_scope_symbol(file_symbol, offset);
        let expr = AstUtils::get_expr_for_offset(&file_info.borrow().ast.unwrap(), offset);
    }

    /* Given an ast, return the Stmt that is at the given offset */
    pub fn get_expr_for_offset(stmts: &Vec<Stmt>, offset: u32) -> &Expr {
        for stmt in stmts.iter() {
            if stmt.range().start().to_u32() < offset && stmt.range().end().to_u32() < offset {
                match stmt {
                    Stmt::AnnAssign(a) => {
                        if AstUtils::is_in_range_expr(&*a.annotation, offset) {
                            return &a.annotation;
                        }
                        if AstUtils::is_in_range_expr(&*a.target, offset) {
                            return &a.target;
                        }
                        if let Some(value) = AstUtils::is_in_range_expr(a.value, offset)
                    }
                }
            }
        }
        todo!()
    }

    fn is_in_range_stmt(stmt: &Stmt, offset: u32) -> bool {
        stmt.range().start().to_u32() < offset && stmt.range().end().to_u32() > offset
    }
    fn is_in_range_expr(expr: &Expr, offset: u32) -> bool {
        expr.range().start().to_u32() < offset && expr.range().end().to_u32() > offset
    }

}