use std::{cell::RefCell, rc::Rc};
use lsp_types::{CompletionItem, CompletionList, CompletionResponse};
use ruff_python_ast::visitor::{walk_alias, walk_except_handler, walk_expr, walk_keyword, walk_parameter, walk_pattern, walk_pattern_keyword, walk_stmt, walk_type_param, Visitor};
use ruff_python_ast::{Alias, ExceptHandler, Expr, Keyword, Parameter, Pattern, PatternKeyword, Stmt, StmtImport, StmtImportFrom, TypeParam};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::core::evaluation::ExprOrIdent;
use crate::core::import_resolver;
use crate::threads::SessionInfo;
use crate::S;
use crate::core::symbols::symbol::Symbol;
use crate::core::file_mgr::FileInfo;

use super::ast_utils::ExprFinderVisitor;



pub struct CompletionFeature;

impl CompletionFeature {

    pub fn autocomplete(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<CompletionResponse> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let file_info =  file_info.borrow();
        let ast = file_info.ast.as_ref().unwrap();
        let mut expr: Option<ExprOrIdent> = None;
        //
        let mut previous = None;
        for stmt in ast.iter() {
            if previous.is_none() {
                previous = Some(stmt);
                continue;
            }
            if stmt.range().start().to_usize() > offset { //Next stmt is too far, previous was the right one !
                return complete_stmt(session, file_symbol, previous.unwrap(), offset);
            } else if stmt.range().end().to_usize() > offset { //This stmt finish after the offset, so the actual is the right one !
                return complete_stmt(session, file_symbol, stmt, offset);
            }
            previous = Some(stmt);
        }
        //if the right stmt is the last one
        if ast.iter().last().unwrap().range().end().to_usize() < offset {
            return complete_stmt(session, file_symbol, ast.iter().last().unwrap(), offset);
        }
        unreachable!("This code should not be reachable ! ");
    }
}

fn complete_stmt(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, stmt: &Stmt, offset: usize) -> Option<CompletionResponse> {
    match stmt {
        Stmt::FunctionDef(stmt_function_def) => None,
        Stmt::ClassDef(stmt_class_def) => None,
        Stmt::Return(stmt_return) => None,
        Stmt::Delete(stmt_delete) => None,
        Stmt::Assign(stmt_assign) => None,
        Stmt::AugAssign(stmt_aug_assign) => None,
        Stmt::AnnAssign(stmt_ann_assign) => None,
        Stmt::TypeAlias(stmt_type_alias) => None,
        Stmt::For(stmt_for) => None,
        Stmt::While(stmt_while) => None,
        Stmt::If(stmt_if) => None,
        Stmt::With(stmt_with) => None,
        Stmt::Match(stmt_match) => None,
        Stmt::Raise(stmt_raise) => None,
        Stmt::Try(stmt_try) => None,
        Stmt::Assert(stmt_assert) => None,
        Stmt::Import(stmt_import) => complete_import_stmt(session, file, stmt_import, offset),
        Stmt::ImportFrom(stmt_import_from) => complete_importFrom_stmt(session, file, stmt_import_from, offset),
        Stmt::Global(stmt_global) => None,
        Stmt::Nonlocal(stmt_nonlocal) => None,
        Stmt::Expr(stmt_expr) => None,
        Stmt::Pass(stmt_pass) => None,
        Stmt::Break(stmt_break) => None,
        Stmt::Continue(stmt_continue) => None,
        Stmt::IpyEscapeCommand(stmt_ipy_escape_command) => None,
    }

    // Some(CompletionResponse::List(CompletionList {
    //     is_incomplete: false,
    //     items: vec![
    //         CompletionItem {
    //             label: S!("test"),
    //             ..Default::default()
    //         }
    //     ]
    // }))
}

fn complete_import_stmt(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, stmt_import: &StmtImport, offset: usize) -> Option<CompletionResponse> {
    let mut items = vec![];
    for alias in stmt_import.names.iter() {
        if alias.name.range().end().to_usize() == offset {
            let names = import_resolver::get_all_valid_names(session, file, None, S!(alias.name.id.as_str()), None);
            for name in names {
                items.push(CompletionItem {
                    label: name,
                    kind: Some(lsp_types::CompletionItemKind::MODULE),
                    ..Default::default()
                });
            }
        }
    }
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items
    }))
}

fn complete_importFrom_stmt(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, stmt_import: &StmtImportFrom, offset: usize) -> Option<CompletionResponse> {
    let mut items = vec![];
    for alias in stmt_import.names.iter() {
        if alias.name.range().end().to_usize() == offset {
            let names = import_resolver::get_all_valid_names(session, file, stmt_import.module.as_ref(), S!(alias.name.id.as_str()), Some(stmt_import.level));
            for name in names {
                items.push(CompletionItem {
                    label: name,
                    kind: Some(lsp_types::CompletionItemKind::MODULE),
                    ..Default::default()
                });
            }
        }
    }
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items
    }))
}



































pub struct ExprBeforeOffsetFinder<'a> {
    offset: TextSize,
    expr: Option<ExprOrIdent<'a>>,
}

impl<'a> ExprBeforeOffsetFinder<'a> {

    pub fn find_expr_before(stmt: &'a Stmt, offset: u32) -> Option<ExprOrIdent> {
        let mut visitor = Self {
            offset: TextSize::new(offset),
            expr: None
        };
        visitor.visit_stmt(stmt);
        visitor.expr
    }

}

impl<'a> Visitor<'a> for ExprBeforeOffsetFinder<'a> {

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
                    self.expr = Some(ExprOrIdent::Ident(&asname))
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
                    self.expr = Some(ExprOrIdent::Ident(&ident));
                }
            }
        } else {
            walk_except_handler(self, except_handler);
        }
    }

    fn visit_parameter(&mut self, parameter: &'a Parameter) {
        walk_parameter(self, parameter);
        if self.expr.is_none() && parameter.name.range().contains(self.offset) {
            self.expr = Some(ExprOrIdent::Parameter(&parameter));
        }
    }

    fn visit_keyword(&mut self, keyword: &'a Keyword) {
        walk_keyword(self, keyword);

        if self.expr.is_none() {
            if let Some(ref ident) = keyword.arg {
                if ident.range().contains(self.offset) {
                    self.expr = Some(ExprOrIdent::Ident(&ident));
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
                    self.expr = Some(ExprOrIdent::Ident(&ident));
                    break;
                }
            }
        } else {
            walk_stmt(self, stmt);
        }
    }
}