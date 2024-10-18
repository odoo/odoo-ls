use std::{cell::RefCell, rc::Rc};
use lsp_types::{CompletionItem, CompletionList, CompletionResponse};
use ruff_python_ast::visitor::{walk_alias, walk_except_handler, walk_expr, walk_keyword, walk_parameter, walk_pattern, walk_pattern_keyword, walk_stmt, walk_type_param, Visitor};
use ruff_python_ast::{Alias, ExceptHandler, Expr, Keyword, Parameter, Pattern, PatternKeyword, Stmt, StmtGlobal, StmtImport, StmtImportFrom, StmtNonlocal, TypeParam};
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
        return complete_vec_stmt(ast, session, file_symbol, offset)
    }
}

fn complete_stmt(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, stmt: &Stmt, offset: usize) -> Option<CompletionResponse> {
    match stmt {
        Stmt::FunctionDef(stmt_function_def) => complete_function_def_stmt(session, file, stmt_function_def, offset),
        Stmt::ClassDef(stmt_class_def) => complete_class_def_stmt(session, file, stmt_class_def, offset),
        Stmt::Return(stmt_return) => complete_return_stmt(session, file, stmt_return, offset),
        Stmt::Delete(stmt_delete) => complete_delete_stmt(session, file, stmt_delete, offset),
        Stmt::Assign(stmt_assign) => complete_assign_stmt(session, file, stmt_assign, offset),
        Stmt::AugAssign(stmt_aug_assign) => complete_aug_assign_stmt(session, file, stmt_aug_assign, offset),
        Stmt::AnnAssign(stmt_ann_assign) => complete_ann_assign_stmt(session, file, stmt_ann_assign, offset),
        Stmt::TypeAlias(stmt_type_alias) => complete_type_alias_stmt(session, file, stmt_type_alias, offset),
        Stmt::For(stmt_for) => complete_for_stmt(session, file, stmt_for, offset),
        Stmt::While(stmt_while) => complete_while_stmt(session, file, stmt_while, offset),
        Stmt::If(stmt_if) => complete_if_stmt(session, file, stmt_if, offset),
        Stmt::With(stmt_with) => complete_with_stmt(session, file, stmt_with, offset),
        Stmt::Match(stmt_match) => complete_match_stmt(session, file, stmt_match, offset),
        Stmt::Raise(stmt_raise) => complete_raise_stmt(session, file, stmt_raise, offset),
        Stmt::Try(stmt_try) => complete_try_stmt(session, file, stmt_try, offset),
        Stmt::Assert(stmt_assert) => complete_assert_stmt(session, file, stmt_assert, offset),
        Stmt::Import(stmt_import) => complete_import_stmt(session, file, stmt_import, offset),
        Stmt::ImportFrom(stmt_import_from) => complete_import_from_stmt(session, file, stmt_import_from, offset),
        Stmt::Global(stmt_global) => complete_global_stmt(session, file, stmt_global, offset),
        Stmt::Nonlocal(stmt_nonlocal) => complete_nonlocal_stmt(session, file, stmt_nonlocal, offset),
        Stmt::Expr(stmt_expr) => complete_expr(&stmt_expr.value, session, file, offset),
        Stmt::Pass(stmt_pass) => None,
        Stmt::Break(stmt_break) => None,
        Stmt::Continue(stmt_continue) => None,
        Stmt::IpyEscapeCommand(stmt_ipy_escape_command) => None,
    }
}

fn complete_vec_stmt(stmts: &Vec<Stmt>, session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, offset: usize) -> Option<CompletionResponse> {
    let mut previous = None;
    for stmt in stmts.iter() {
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
    if stmts.iter().last().unwrap().range().end().to_usize() < offset {
        return complete_stmt(session, file_symbol, stmts.iter().last().unwrap(), offset);
    }
    unreachable!("This code should not be reachable ! ");
}

fn complete_function_def_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_function_def: &ruff_python_ast::StmtFunctionDef, offset: usize) -> Option<CompletionResponse> {
    if stmt_function_def.body.len() > 0 {
        if offset < stmt_function_def.body.first().unwrap().range().start().to_usize() && stmt_function_def.body.last().unwrap().range().end().to_usize() < offset {
            return complete_vec_stmt(&stmt_function_def.body, session, file, offset);
        }
    }
    None
}

fn complete_class_def_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_class_def: &ruff_python_ast::StmtClassDef, offset: usize) -> Option<CompletionResponse> {
    if stmt_class_def.body.len() > 0 {
        if offset < stmt_class_def.body.first().unwrap().range().start().to_usize() && stmt_class_def.body.last().unwrap().range().end().to_usize() < offset {
            return complete_vec_stmt(&stmt_class_def.body, session, file, offset);
        }
    }
    None
}

fn complete_return_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_return: &ruff_python_ast::StmtReturn, offset: usize) -> Option<CompletionResponse> {
    if stmt_return.value.is_some() {
        return complete_expr( stmt_return.value.as_ref().unwrap(), session, file, offset);
    }
    None
}

fn complete_delete_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_delete: &ruff_python_ast::StmtDelete, offset: usize) -> Option<CompletionResponse> {
    for target in stmt_delete.targets.iter() {
        if offset > target.range().start().to_usize() && offset < target.range().end().to_usize() {
            return complete_expr( target, session, file, offset);
        }
    }
    None
}

fn complete_assign_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_assign: &ruff_python_ast::StmtAssign, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_assign.value.range().start().to_usize() && offset < stmt_assign.value.range().end().to_usize() {
        return complete_expr( &stmt_assign.value, session, file, offset);
    }
    None
}

fn complete_aug_assign_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_aug_assign: &ruff_python_ast::StmtAugAssign, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_aug_assign.value.range().start().to_usize() && offset < stmt_aug_assign.value.range().end().to_usize() {
        return complete_expr( &stmt_aug_assign.value, session, file, offset);
    }
    None
}

fn complete_ann_assign_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_ann_assign: &ruff_python_ast::StmtAnnAssign, offset: usize) -> Option<CompletionResponse> {
    if stmt_ann_assign.value.is_some() {
        if offset > stmt_ann_assign.value.as_ref().unwrap().range().start().to_usize() && offset < stmt_ann_assign.value.as_ref().unwrap().range().end().to_usize() {
            return complete_expr( stmt_ann_assign.value.as_ref().unwrap(), session, file, offset);
        }
    }
    None
}

fn complete_type_alias_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_type_alias: &ruff_python_ast::StmtTypeAlias, offset: usize) -> Option<CompletionResponse> {
    None
}

fn complete_for_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_for: &ruff_python_ast::StmtFor, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_for.iter.range().start().to_usize() && offset < stmt_for.iter.range().end().to_usize() {
        return complete_expr( &stmt_for.iter, session, file, offset);
    }
    if stmt_for.body.len() > 0 {
        if offset < stmt_for.body.first().unwrap().range().start().to_usize() && stmt_for.body.last().unwrap().range().end().to_usize() < offset {
            return complete_vec_stmt(&stmt_for.body, session, file, offset);
        }
    }
    None
}

fn complete_while_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_while: &ruff_python_ast::StmtWhile, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_while.test.range().start().to_usize() && offset < stmt_while.test.range().end().to_usize() {
        return complete_expr( &stmt_while.test, session, file, offset);
    }
    if stmt_while.body.len() > 0 {
        if offset < stmt_while.body.first().unwrap().range().start().to_usize() && stmt_while.body.last().unwrap().range().end().to_usize() < offset {
            return complete_vec_stmt(&stmt_while.body, session, file, offset);
        }
    }
    None
}

fn complete_if_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_if: &ruff_python_ast::StmtIf, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_if.test.range().start().to_usize() && offset < stmt_if.test.range().end().to_usize() {
        return complete_expr( &stmt_if.test, session, file, offset);
    }
    if stmt_if.body.len() > 0 {
        if offset < stmt_if.body.first().unwrap().range().start().to_usize() && stmt_if.body.last().unwrap().range().end().to_usize() < offset {
            return complete_vec_stmt(&stmt_if.body, session, file, offset);
        }
    }
    None
}

fn complete_with_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_with: &ruff_python_ast::StmtWith, offset: usize) -> Option<CompletionResponse> {
    //TODO complete with items
    // if stmt_with.items.len() > 0 {
    //     for item in stmt_with.items.iter() {
    //         if offset > item.range().start().to_usize() && offset < item.range().end().to_usize() {
    //             return complete_expr( item, session, file, offset);
    //         }
    //     }
    // }
    if stmt_with.body.len() > 0 {
        if offset < stmt_with.body.first().unwrap().range().start().to_usize() && stmt_with.body.last().unwrap().range().end().to_usize() < offset {
            return complete_vec_stmt(&stmt_with.body, session, file, offset);
        }
    }
    None
}

fn complete_match_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_match: &ruff_python_ast::StmtMatch, offset: usize) -> Option<CompletionResponse> {
    for case in stmt_match.cases.iter() {
        if !case.body.is_empty() {
            if offset > case.body.first().as_ref().unwrap().range().start().to_usize() && offset < case.body.last().as_ref().unwrap().range().end().to_usize() {
                return complete_vec_stmt(&case.body, session, file, offset);
            }
        }
    }
    None
}

fn complete_raise_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_raise: &ruff_python_ast::StmtRaise, offset: usize) -> Option<CompletionResponse> {
    if stmt_raise.exc.is_some() {
        if offset > stmt_raise.exc.as_ref().unwrap().range().start().to_usize() && offset < stmt_raise.exc.as_ref().unwrap().range().end().to_usize() {
            return complete_expr( stmt_raise.exc.as_ref().unwrap(), session, file, offset);
        }
    }
    None
}

fn complete_try_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_try: &ruff_python_ast::StmtTry, offset: usize) -> Option<CompletionResponse> {
    if stmt_try.body.len() > 0 {
        if offset < stmt_try.body.first().unwrap().range().start().to_usize() && stmt_try.body.last().unwrap().range().end().to_usize() < offset {
            return complete_vec_stmt(&stmt_try.body, session, file, offset);
        }
    }
    //TODO handlers
    /*if stmt_try.handlers.len() > 0 {
        if offset < stmt_try.handlers.first().unwrap().range().start().to_usize() && stmt_try.handlers.last().unwrap().range().end().to_usize() < offset {
            return complete_vec_stmt(&stmt_try.handlers, session, file, offset);
        }
    }*/
    if stmt_try.orelse.len() > 0 {
        if offset < stmt_try.orelse.first().unwrap().range().start().to_usize() && stmt_try.orelse.last().unwrap().range().end().to_usize() < offset {
            return complete_vec_stmt(&stmt_try.orelse, session, file, offset);
        }
    }
    if stmt_try.finalbody.len() > 0 {
        if offset < stmt_try.finalbody.first().unwrap().range().start().to_usize() && stmt_try.finalbody.last().unwrap().range().end().to_usize() < offset {
            return complete_vec_stmt(&stmt_try.finalbody, session, file, offset);
        }
    }
    None
}

fn complete_assert_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_assert: &ruff_python_ast::StmtAssert, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_assert.test.as_ref().range().start().to_usize() && offset < stmt_assert.test.as_ref().range().end().to_usize() {
        return complete_expr( stmt_assert.test.as_ref(), session, file, offset);
    }
    if stmt_assert.msg.is_some() {
        if offset > stmt_assert.msg.as_ref().unwrap().range().start().to_usize() && offset < stmt_assert.msg.as_ref().unwrap().range().end().to_usize() {
            return complete_expr( stmt_assert.msg.as_ref().unwrap(), session, file, offset);
        }
    }
    None
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

fn complete_import_from_stmt(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, stmt_import: &StmtImportFrom, offset: usize) -> Option<CompletionResponse> {
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

fn complete_global_stmt(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, stmt_global: &StmtGlobal, offset: usize) -> Option<CompletionResponse> {
    None
}

fn complete_nonlocal_stmt(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, stmt_nonlocal: &StmtNonlocal, offset: usize) -> Option<CompletionResponse> {
    None
}

fn complete_expr(expr: &Expr, session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, offset: usize) -> Option<CompletionResponse> {

    None
}
