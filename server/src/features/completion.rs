use std::collections::HashMap;
use std::{cell::RefCell, rc::Rc};
use lsp_types::{CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionList, CompletionResponse, MarkupContent};
use ruff_python_ast::{ExceptHandler, Expr, ExprAttribute, ExprIf, ExprName, ExprSubscript, ExprYield, Stmt, StmtGlobal, StmtImport, StmtImportFrom, StmtNonlocal};
use ruff_text_size::Ranged;
use weak_table::traits::WeakElement;

use crate::constants::SymType;
use crate::core::evaluation::{Evaluation, EvaluationSymbolWeak};
use crate::core::import_resolver;
use crate::core::python_arch_eval_hooks::PythonArchEvalHooks;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::threads::SessionInfo;
use crate::S;
use crate::core::symbols::symbol::Symbol;
use crate::core::file_mgr::FileInfo;

use super::hover::HoverFeature;


#[allow(non_camel_case_types)]
pub enum ExpectedType {
    MODEL_NAME,
    CLASS(Rc<RefCell<Symbol>>),
}

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
        complete_vec_stmt(ast, session, file_symbol, offset)
    }
}

/* **********************************************************************
***************************** Statements ********************************
*********************************************************************** */

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
        Stmt::Expr(stmt_expr) => complete_expr(&stmt_expr.value, session, file, offset, false, &vec![]),
        Stmt::Pass(_) => None,
        Stmt::Break(_) => None,
        Stmt::Continue(_) => None,
        Stmt::IpyEscapeCommand(_) => None,
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
    if stmts.iter().last().unwrap().range().end().to_usize() >= offset {
        return complete_stmt(session, file_symbol, stmts.iter().last().unwrap(), offset);
    }
    //The user is writting after the last stmt
    None
}

fn complete_function_def_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_function_def: &ruff_python_ast::StmtFunctionDef, offset: usize) -> Option<CompletionResponse> {
    if !stmt_function_def.body.is_empty() {
        if offset > stmt_function_def.body.first().unwrap().range().start().to_usize() && stmt_function_def.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_function_def.body, session, file, offset);
        }
    }
    None
}

fn complete_class_def_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_class_def: &ruff_python_ast::StmtClassDef, offset: usize) -> Option<CompletionResponse> {
    for base in stmt_class_def.bases().iter() {
        if offset > base.range().start().to_usize() && offset <= base.range().end().to_usize() {
            return complete_expr( base, session, file, offset, false, &vec![]); //TODO only classes?
        }
    }
    if !stmt_class_def.body.is_empty() {
        if offset > stmt_class_def.body.first().unwrap().range().start().to_usize() && stmt_class_def.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_class_def.body, session, file, offset);
        }
    }
    None
}

fn complete_return_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_return: &ruff_python_ast::StmtReturn, offset: usize) -> Option<CompletionResponse> {
    if stmt_return.value.is_some() {
        return complete_expr( stmt_return.value.as_ref().unwrap(), session, file, offset, false, &vec![]);
    }
    None
}

fn complete_delete_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_delete: &ruff_python_ast::StmtDelete, offset: usize) -> Option<CompletionResponse> {
    for target in stmt_delete.targets.iter() {
        if offset > target.range().start().to_usize() && offset <= target.range().end().to_usize() {
            return complete_expr( target, session, file, offset, false, &vec![]);
        }
    }
    None
}

fn complete_assign_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_assign: &ruff_python_ast::StmtAssign, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_assign.value.range().start().to_usize() && offset <= stmt_assign.value.range().end().to_usize() {
        return complete_expr( &stmt_assign.value, session, file, offset, false, &vec![]);
    }
    None
}

fn complete_aug_assign_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_aug_assign: &ruff_python_ast::StmtAugAssign, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_aug_assign.value.range().start().to_usize() && offset <= stmt_aug_assign.value.range().end().to_usize() {
        return complete_expr( &stmt_aug_assign.value, session, file, offset, false, &vec![]);
    }
    None
}

fn complete_ann_assign_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_ann_assign: &ruff_python_ast::StmtAnnAssign, offset: usize) -> Option<CompletionResponse> {
    if stmt_ann_assign.value.is_some() {
        if offset > stmt_ann_assign.value.as_ref().unwrap().range().start().to_usize() && offset <= stmt_ann_assign.value.as_ref().unwrap().range().end().to_usize() {
            return complete_expr( stmt_ann_assign.value.as_ref().unwrap(), session, file, offset, false, &vec![]);
        }
    }
    None
}

fn complete_type_alias_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_type_alias: &ruff_python_ast::StmtTypeAlias, offset: usize) -> Option<CompletionResponse> {
    None
}

fn complete_for_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_for: &ruff_python_ast::StmtFor, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_for.iter.range().start().to_usize() && offset <= stmt_for.iter.range().end().to_usize() {
        return complete_expr( &stmt_for.iter, session, file, offset, false, &vec![]);
    }
    if !stmt_for.body.is_empty() {
        if offset > stmt_for.body.first().unwrap().range().start().to_usize() && stmt_for.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_for.body, session, file, offset);
        }
    }
    None
}

fn complete_while_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_while: &ruff_python_ast::StmtWhile, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_while.test.range().start().to_usize() && offset <= stmt_while.test.range().end().to_usize() {
        return complete_expr( &stmt_while.test, session, file, offset, false, &vec![]);
    }
    if !stmt_while.body.is_empty() {
        if offset > stmt_while.body.first().unwrap().range().start().to_usize() && stmt_while.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_while.body, session, file, offset);
        }
    }
    None
}

fn complete_if_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_if: &ruff_python_ast::StmtIf, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_if.test.range().start().to_usize() && offset <= stmt_if.test.range().end().to_usize() {
        return complete_expr( &stmt_if.test, session, file, offset, false, &vec![]);
    }
    if !stmt_if.body.is_empty() {
        if offset > stmt_if.body.first().unwrap().range().start().to_usize() && stmt_if.body.last().unwrap().range().end().to_usize() >= offset {
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
    if !stmt_with.body.is_empty() {
        if offset > stmt_with.body.first().unwrap().range().start().to_usize() && stmt_with.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_with.body, session, file, offset);
        }
    }
    None
}

fn complete_match_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_match: &ruff_python_ast::StmtMatch, offset: usize) -> Option<CompletionResponse> {
    for case in stmt_match.cases.iter() {
        if !case.body.is_empty() {
            if offset > case.body.first().as_ref().unwrap().range().start().to_usize() && offset <= case.body.last().as_ref().unwrap().range().end().to_usize() {
                return complete_vec_stmt(&case.body, session, file, offset);
            }
        }
    }
    None
}

fn complete_raise_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_raise: &ruff_python_ast::StmtRaise, offset: usize) -> Option<CompletionResponse> {
    if stmt_raise.exc.is_some() {
        if offset > stmt_raise.exc.as_ref().unwrap().range().start().to_usize() && offset <= stmt_raise.exc.as_ref().unwrap().range().end().to_usize() {
            return complete_expr( stmt_raise.exc.as_ref().unwrap(), session, file, offset, false, &vec![]);
        }
    }
    None
}

fn complete_try_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_try: &ruff_python_ast::StmtTry, offset: usize) -> Option<CompletionResponse> {
    if !stmt_try.body.is_empty() {
        if offset > stmt_try.body.first().unwrap().range().start().to_usize() && stmt_try.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_try.body, session, file, offset);
        }
    }
    for handler in  stmt_try.handlers.iter() {
        match handler {
            ExceptHandler::ExceptHandler(except_handler_except_handler) => {
                if offset > except_handler_except_handler.range().start().to_usize() && except_handler_except_handler.range().end().to_usize() >= offset {
                    return complete_vec_stmt(&except_handler_except_handler.body, session, file, offset);
                }
            },
        }
    }
    if !stmt_try.orelse.is_empty() {
        if offset > stmt_try.orelse.first().unwrap().range().start().to_usize() && stmt_try.orelse.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_try.orelse, session, file, offset);
        }
    }
    if !stmt_try.finalbody.is_empty() {
        if offset > stmt_try.finalbody.first().unwrap().range().start().to_usize() && stmt_try.finalbody.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_try.finalbody, session, file, offset);
        }
    }
    None
}

fn complete_assert_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_assert: &ruff_python_ast::StmtAssert, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_assert.test.as_ref().range().start().to_usize() && offset <= stmt_assert.test.as_ref().range().end().to_usize() {
        return complete_expr( stmt_assert.test.as_ref(), session, file, offset, false, &vec![]);
    }
    if stmt_assert.msg.is_some() {
        if offset > stmt_assert.msg.as_ref().unwrap().range().start().to_usize() && offset <= stmt_assert.msg.as_ref().unwrap().range().end().to_usize() {
            return complete_expr( stmt_assert.msg.as_ref().unwrap(), session, file, offset, false, &vec![]);
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
    if let Some(module) = stmt_import.module.as_ref() {
        if module.range.end().to_usize() == offset && !stmt_import.names.is_empty() {
            let names = import_resolver::get_all_valid_names(session, file, None, S!(stmt_import.names[0].name.id.as_str()), Some(stmt_import.level));
            for name in names {
                items.push(CompletionItem {
                    label: name,
                    kind: Some(lsp_types::CompletionItemKind::MODULE),
                    ..Default::default()
                });
            }
        }
    }
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

/* *********************************************************************
**************************** Expressions *******************************
********************************************************************* */

fn complete_expr(expr: &Expr, session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    match expr {
        Expr::BoolOp(expr_bool_op) => compare_bool_op(session, file, expr_bool_op, offset, is_param, expected_type),
        Expr::Named(expr_named) => compare_named(session, file, expr_named, offset, is_param, expected_type),
        Expr::BinOp(expr_bin_op) => compare_bin_op(session, file, expr_bin_op, offset, is_param, expected_type),
        Expr::UnaryOp(expr_unary_op) => compare_unary_op(session, file, expr_unary_op, offset, is_param, expected_type),
        Expr::Lambda(expr_lambda) => compare_lambda(session, file, expr_lambda, offset, is_param, expected_type),
        Expr::If(expr_if) => complete_if_expr(session, file, expr_if, offset, is_param, expected_type),
        Expr::Dict(expr_dict) => complete_dict(session, file, expr_dict, offset, is_param, expected_type),
        Expr::Set(_) => None,
        Expr::ListComp(_) => None,
        Expr::SetComp(_) => None,
        Expr::DictComp(_) => None,
        Expr::Generator(_) => None,
        Expr::Await(_) => None,
        Expr::Yield(expr_yield) => complete_yield(session, file, expr_yield, offset, is_param, expected_type),
        Expr::YieldFrom(_) => None,
        Expr::Compare(expr_compare) => complete_compare(session, file, expr_compare, offset, is_param, expected_type),
        Expr::Call(expr_call) => complete_call(session, file, expr_call, offset, is_param, expected_type),
        Expr::FString(_) => None,
        Expr::StringLiteral(expr_string_literal) => complete_string_literal(session, file, expr_string_literal, offset, is_param, expected_type),
        Expr::BytesLiteral(_) => None,
        Expr::NumberLiteral(_) => None,
        Expr::BooleanLiteral(_) => None,
        Expr::NoneLiteral(_) => None,
        Expr::EllipsisLiteral(_) => None,
        Expr::Attribute(expr_attribute) => complete_attribut(session, file, expr_attribute, offset, is_param, expected_type),
        Expr::Subscript(expr_subscript) => complete_subscript(session, file, expr_subscript, offset, is_param, expected_type),
        Expr::Starred(_) => None,
        Expr::Name(expr_name) => complete_name(session, file, expr_name, offset, is_param, expected_type),
        Expr::List(expr_list) => complete_list(session, file, expr_list, offset, is_param, expected_type),
        Expr::Tuple(_) => None,
        Expr::Slice(_) => None,
        Expr::IpyEscapeCommand(_) => None,
    }
}

fn compare_bool_op(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_bool_op: &ruff_python_ast::ExprBoolOp, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    for value in expr_bool_op.values.iter() {
        if offset > value.range().start().to_usize() && offset <= value.range().end().to_usize() {
            return complete_expr( value, session, file, offset, is_param, expected_type);
        }
    }
    None
}

fn compare_named(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_named: &ruff_python_ast::ExprNamed, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    if offset > expr_named.value.range().start().to_usize() && offset <= expr_named.value.range().end().to_usize() {
        return complete_expr( &expr_named.value, session, file, offset, is_param, expected_type);
    }
    None
}

fn compare_bin_op(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_bin_op: &ruff_python_ast::ExprBinOp, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    if offset > expr_bin_op.left.range().start().to_usize() && offset <= expr_bin_op.left.range().end().to_usize() {
        return complete_expr( &expr_bin_op.left, session, file, offset, is_param, expected_type);
    }
    if offset > expr_bin_op.right.range().start().to_usize() && offset <= expr_bin_op.right.range().end().to_usize() {
        return complete_expr( &expr_bin_op.right, session, file, offset, is_param, expected_type);
    }
    None
}

fn compare_unary_op(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_unary_op: &ruff_python_ast::ExprUnaryOp, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    if offset > expr_unary_op.operand.range().start().to_usize() && offset <= expr_unary_op.operand.range().end().to_usize() {
        return complete_expr( &expr_unary_op.operand, session, file, offset, is_param, expected_type);
    }
    None
}

fn compare_lambda(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_lambda: &ruff_python_ast::ExprLambda, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    if offset > expr_lambda.body.range().start().to_usize() && offset <= expr_lambda.body.range().end().to_usize() {
        return complete_expr( &expr_lambda.body, session, file, offset, is_param, expected_type);
    }
    None
}

//Expr if, used in "a if b else c"
fn complete_if_expr(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_if: &ExprIf, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    if offset > expr_if.test.range().start().to_usize() && offset <= expr_if.test.range().end().to_usize() {
        return complete_expr( &expr_if.test, session, file, offset, is_param, expected_type);
    }
    if offset > expr_if.body.range().start().to_usize() && offset <= expr_if.body.range().end().to_usize() {
        return complete_expr( &expr_if.body, session, file, offset, is_param, expected_type);
    }
    if offset > expr_if.orelse.range().start().to_usize() && offset <= expr_if.orelse.range().end().to_usize() {
        return complete_expr( &expr_if.orelse, session, file, offset, is_param, expected_type);
    }
    None
}

fn complete_dict(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_dict: &ruff_python_ast::ExprDict, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    for dict_item in expr_dict.items.iter() {
        if dict_item.key.is_some() {
            if offset > dict_item.value.range().start().to_usize() && offset <= dict_item.value.range().end().to_usize() {
                return complete_expr( &dict_item.value, session, file, offset, is_param, expected_type);
            }
        }
    }
    None
}

fn complete_yield(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_yield: &ExprYield, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    if expr_yield.value.is_some() && offset > expr_yield.value.as_ref().unwrap().range().start().to_usize() && offset <= expr_yield.value.as_ref().unwrap().range().end().to_usize() {
        return complete_expr( expr_yield.value.as_ref().unwrap(), session, file, offset, is_param, expected_type);
    }
    None
}

fn complete_compare(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_compare: &ruff_python_ast::ExprCompare, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    if offset > expr_compare.left.range().start().to_usize() && offset <= expr_compare.left.range().end().to_usize() {
        return complete_expr( &expr_compare.left, session, file, offset, is_param, expected_type);
    }
    for expr in expr_compare.comparators.iter() {
        if offset > expr.range().start().to_usize() && offset <= expr.range().end().to_usize() {
            return complete_expr( expr, session, file, offset, is_param, expected_type);
        }
    }
    None
}

fn complete_call(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_call: &ruff_python_ast::ExprCall, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    if offset > expr_call.func.range().start().to_usize() && offset <= expr_call.func.range().end().to_usize() {
        return complete_expr( &expr_call.func, session, file, offset, is_param, expected_type);
    }
    for arg in expr_call.arguments.args.iter() {
        if offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize() {
            return complete_expr(arg, session, file, offset, is_param, expected_type);
        }
    }
    None
}

fn complete_string_literal(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_string_literal: &ruff_python_ast::ExprStringLiteral, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    let mut items = vec![];
    let current_module = file.borrow().find_module();
    let models = session.sync_odoo.models.clone();
    for expected_type in expected_type.iter() {
        match expected_type {
            ExpectedType::MODEL_NAME => {
                let prefix = expr_string_literal.value.to_str();
                let prefix_head = match prefix.rfind('.') {
                    Some(index) => &prefix[..=index],
                    None => "",
                };
                for (model_name, model) in models.iter() {
                    if model_name.starts_with(prefix) && model_name != "_unknown" {
                        let label = model_name.clone();
                        let insert_text = model_name.strip_prefix(prefix_head).map(|s| s.to_string());
                        let mut label_details = None;
                        let mut sort_text = Some(format!("_{}", label.clone()));


                        if let Some(ref current_module) = current_module {
                            let model_class_syms = model.borrow().get_main_symbols(session, None,&mut None);
                            let modules = model_class_syms.iter().flat_map(|model_rc| 
                                model_rc.borrow().find_module());
                            let required_modules = modules.filter(|module| 
                                !ModuleSymbol::is_in_deps(session, &current_module, &module.borrow().as_module_package().dir_name, &mut None));
                            let dep_names: Vec<String> = required_modules.map(|module| module.borrow().as_module_package().dir_name.clone()).collect();
                            if !dep_names.is_empty() {
                                if !session.sync_odoo.config.ac_filter_model_names{
                                    continue
                                }
                                label_details = Some(CompletionItemLabelDetails {
                                    detail: None,
                                    description: Some(S!(format!(
                                        "require {}",
                                        dep_names.join(", ")
                                    ))),
                                });
                                sort_text = Some(label.clone());
                            };
                        }

                        items.push(CompletionItem {
                            label,
                            insert_text,
                            kind: Some(lsp_types::CompletionItemKind::CLASS),
                            label_details,
                            sort_text,
                            ..Default::default()
                    });
                    }
                }
            },
            ExpectedType::CLASS(_) => {},
        }
    }
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items: items
    }))
}

fn complete_attribut(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, attr: &ExprAttribute, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    let mut items = vec![];
    let scope = Symbol::get_scope_symbol(file.clone(), offset as u32, is_param);
    if offset > attr.value.range().start().to_usize() && offset <= attr.value.range().end().to_usize() {
        return complete_expr( &attr.value, session, file, offset, is_param, expected_type);
    } else {
        let parent = Evaluation::eval_from_ast(session, &attr.value, scope, &attr.range().start()).0;

        for parent_eval in parent.iter() {
            let parent_sym_eval_weak = parent_eval.symbol.get_symbol(session, &mut None, &mut vec![], Some(file.clone()));
            if !parent_sym_eval_weak.weak.is_expired() {
                let parent_sym_types = Symbol::follow_ref(&parent_sym_eval_weak, session, &mut None, true, false, None, &mut vec![]);
                for parent_sym_type in parent_sym_types.iter() {
                    if let Some(parent_sym) = parent_sym_type.weak.upgrade() {
                        let mut all_symbols: HashMap<String, Vec<(Rc<RefCell<Symbol>>, Option<String>)>> = HashMap::new();
                        let from_module = parent_sym.borrow().find_module().clone();
                        Symbol::all_members(&parent_sym, session, &mut all_symbols, true, from_module, &mut None, parent_sym_eval_weak.is_super);
                        for (_symbol_name, symbols) in all_symbols {
                            //we could use symbol_name to remove duplicated names, but it would hide functions vs variables
                            if _symbol_name.starts_with(attr.attr.id.as_str()) {
                                if let Some((final_sym, dep)) = symbols.first() {
                                    items.push(build_completion_item_from_symbol(session, final_sym, dep.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items
    }))
}

fn complete_subscript(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_subscript: &ExprSubscript, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    let scope = Symbol::get_scope_symbol(file.clone(), offset as u32, is_param);
    let subscripted = Evaluation::eval_from_ast(session, &expr_subscript.value, scope, &expr_subscript.value.range().start()).0;
    for eval in subscripted.iter() {
        let eval_symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], Some(file.clone()));
        if !eval_symbol.weak.is_expired() {
            let symbol_types = Symbol::follow_ref(&eval_symbol, session, &mut None, true, false, None, &mut vec![]);
            for symbol_type in symbol_types.iter() {
                if let Some(symbol_type) = symbol_type.weak.upgrade() {
                    let borrowed = symbol_type.borrow();
                    let get_item = borrowed.get_symbol(&(vec![], vec![S!("__getitem__")]), u32::MAX);
                    if let Some(get_item) = get_item.last() {
                        if get_item.borrow().evaluations().as_ref().unwrap().len() == 1 {
                            let get_item_bw = get_item.borrow();
                            let get_item_eval = get_item_bw.evaluations().as_ref().unwrap().first().unwrap();
                            if get_item_eval.symbol.get_symbol_hook == Some(PythonArchEvalHooks::eval_env_get_item) {
                                return complete_expr(&expr_subscript.slice, session, file, offset, is_param, &vec![ExpectedType::MODEL_NAME]);
                            }
                        }
                    }
                }
            }
        }
    }
    complete_expr(&expr_subscript.slice, session, file, offset, false, &vec![])
}

fn complete_name(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_name: &ExprName, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    let mut items = vec![];
    let name = expr_name.id.to_string();
    if expr_name.range.end().to_usize() == offset {
        let scope = Symbol::get_scope_symbol(file.clone(), offset as u32, is_param);
        let symbols = Symbol::get_all_infered_names(session.sync_odoo,& scope, &name, Some(offset as u32));
        for symbol in symbols {
            items.push(CompletionItem {
                label: symbol.borrow().name().clone(),
                kind: Some(lsp_types::CompletionItemKind::VARIABLE),
                ..Default::default()
            });
        }
    }
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items
    }))
}

fn complete_list(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_list: &ruff_python_ast::ExprList, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    //TODO complete domains
    for expr in expr_list.elts.iter() {
        if offset > expr.range().start().to_usize() && offset < expr.range().end().to_usize() {
            return complete_expr( expr, session, file, offset, is_param, expected_type);
        }
    }
    None
}

fn build_completion_item_from_symbol(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>, dependency: Option<String>) -> CompletionItem {
    //TODO use dependency to show it? or to filter depending of configuration
    let typ = Symbol::follow_ref(&EvaluationSymbolWeak::new(
        Rc::downgrade(symbol),
        None,
        false,
    ), session, &mut None, true, true, None, &mut vec![]);
    let mut label_details = Some(CompletionItemLabelDetails {
        detail: None,
        description: None,
    });
    if typ.len() > 1 {
        label_details = Some(CompletionItemLabelDetails {
            detail: None,
            description: Some(S!("Any")),
        })
    } else if typ.len() == 1 {
        label_details= match typ[0].weak.upgrade().unwrap().borrow().typ() {
            SymType::CLASS => Some(CompletionItemLabelDetails {
                detail: None,
                description: Some(typ[0].weak.upgrade().unwrap().borrow().name().clone()),
            }),
            SymType::VARIABLE => {
                let var_upgraded = typ[0].weak.upgrade().unwrap();
                let var = var_upgraded.borrow();
                if var.evaluations().as_ref().unwrap().len() == 1 {
                    if var.evaluations().as_ref().unwrap()[0].value.is_some() {
                        match var.evaluations().as_ref().unwrap()[0].value.as_ref().unwrap() {
                            crate::core::evaluation::EvaluationValue::ANY() => None,
                            crate::core::evaluation::EvaluationValue::CONSTANT(expr) => {
                                match expr {
                                    Expr::StringLiteral(expr_string_literal) => {
                                        Some(CompletionItemLabelDetails {
                                            detail: None,
                                            description: Some(expr_string_literal.value.to_string()),
                                        })
                                    },
                                    Expr::BytesLiteral(_) => None,
                                    Expr::NumberLiteral(_) => {
                                        Some(CompletionItemLabelDetails {
                                            detail: None,
                                            description: Some(S!("Number")),
                                        })
                                    },
                                    Expr::BooleanLiteral(expr_boolean_literal) => {
                                        Some(CompletionItemLabelDetails {
                                            detail: None,
                                            description: Some(expr_boolean_literal.value.to_string()),
                                        })
                                    },
                                    Expr::NoneLiteral(_) => {
                                        Some(CompletionItemLabelDetails {
                                            detail: None,
                                            description: Some(S!("None")),
                                        })
                                    },
                                    Expr::EllipsisLiteral(_) => None,
                                    _ => {None}
                                }
                            },
                            crate::core::evaluation::EvaluationValue::DICT(_) => None,
                            crate::core::evaluation::EvaluationValue::LIST(_) => None,
                            crate::core::evaluation::EvaluationValue::TUPLE(_) => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            SymType::FUNCTION => {
                let func_upgraded = typ[0].weak.upgrade().unwrap();
                let func = func_upgraded.borrow();
                if func.evaluations().as_ref().unwrap().len() == 1 { //TODO handle multiple evaluations
                    if func.evaluations().as_ref().unwrap()[0].value.is_some() {
                        match func.evaluations().as_ref().unwrap()[0].value.as_ref().unwrap() {
                            crate::core::evaluation::EvaluationValue::ANY() => None,
                            crate::core::evaluation::EvaluationValue::CONSTANT(expr) => {
                                match expr {
                                    Expr::StringLiteral(expr_string_literal) => {
                                        Some(CompletionItemLabelDetails {
                                            detail: None,
                                            description: Some(expr_string_literal.value.to_string()),
                                        })
                                    },
                                    Expr::BytesLiteral(_) => None,
                                    Expr::NumberLiteral(_) => {
                                        Some(CompletionItemLabelDetails {
                                            detail: None,
                                            description: Some(S!("Number")),
                                        })
                                    },
                                    Expr::BooleanLiteral(expr_boolean_literal) => {
                                        Some(CompletionItemLabelDetails {
                                            detail: None,
                                            description: Some(expr_boolean_literal.value.to_string()),
                                        })
                                    },
                                    Expr::NoneLiteral(_) => {
                                        Some(CompletionItemLabelDetails {
                                            detail: None,
                                            description: Some(S!("None")),
                                        })
                                    },
                                    Expr::EllipsisLiteral(_) => None,
                                    _ => {None}
                                }
                            },
                            crate::core::evaluation::EvaluationValue::DICT(_) => None,
                            crate::core::evaluation::EvaluationValue::LIST(_) => None,
                            crate::core::evaluation::EvaluationValue::TUPLE(_) => None,
                        }
                    } else {
                        //TODO
                        Some(CompletionItemLabelDetails {
                            detail: None,
                            description: Some(S!("Any")),
                        })
                    }
                } else {
                    if func.evaluations().as_ref().unwrap().is_empty() {
                        Some(CompletionItemLabelDetails {
                            detail: None,
                            description: Some(S!("None")),
                        })
                    } else {
                        Some(CompletionItemLabelDetails {
                            detail: None,
                            description: Some(S!("Any")),
                        })
                    }
                }
            }
            _ => {Some(CompletionItemLabelDetails {
                detail: None,
                description: None,
            })}
        };
    }
    CompletionItem {
        label: symbol.borrow().name().clone(),
        label_details: label_details,
        detail: None,
        kind: Some(get_completion_item_kind(symbol)),
        sort_text: Some(get_sort_text_for_symbol(symbol)),
        documentation: Some(
            lsp_types::Documentation::MarkupContent(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: HoverFeature::build_markdown_description(session, None, &vec![Evaluation::eval_from_symbol(&Rc::downgrade(symbol), None)])
            })),
        ..Default::default()
    }
}

fn get_sort_text_for_symbol(sym: &Rc<RefCell<Symbol>>/*, cl: Option<Rc<RefCell<Symbol>>>, cl_to_complete: Option<Rc<RefCell<Symbol>>>*/) -> String {
    // return the text used for sorting the result for "symbol". cl is the class owner of symbol, and cl_to_complete the class
    // of the symbol to complete
    // ~ is used as last char of ascii table and } before last one
    let mut base_dist = 0;
    /*if cl_to_complete.is_some() {
        base_dist = cl_to_complete.as_ref().unwrap().borrow().get_base_distance(&sym.borrow().name().clone(),0);
        if base_dist == -1 {
            base_dist = 0;
        }
    }
    let cl_name = match cl {
        Some(x) => x.borrow().name().clone(),
        None => S!("")
    };*/
    //TODO use cl and cl_to_complete
    let mut text = "}".repeat(base_dist as usize)/* + cl_name.as_str()*/ + sym.borrow().name().as_str();
    if sym.borrow().name().starts_with("_") {
        text = "~".to_string() + text.as_str();
    }
    if sym.borrow().name().starts_with("__") {
        text = "~".to_string() + text.as_str();
    }
    text
}

fn get_completion_item_kind(symbol: &Rc<RefCell<Symbol>>) -> CompletionItemKind {
    match symbol.borrow().typ() {
        SymType::ROOT => CompletionItemKind::TEXT,
        SymType::NAMESPACE => CompletionItemKind::FOLDER,
        SymType::PACKAGE(_) => CompletionItemKind::MODULE,
        SymType::FILE => CompletionItemKind::FILE,
        SymType::COMPILED => CompletionItemKind::FILE,
        SymType::VARIABLE => CompletionItemKind::VARIABLE,
        SymType::CLASS => CompletionItemKind::CLASS,
        SymType::FUNCTION => CompletionItemKind::FUNCTION,
    }
}