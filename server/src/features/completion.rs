use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::{cell::RefCell, rc::Rc};
use itertools::Itertools;
use lsp_types::{CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionList, CompletionResponse, MarkupContent};
use ruff_python_ast::{Decorator, ExceptHandler, Expr, ExprAttribute, ExprIf, ExprName, ExprSubscript, ExprYield, Stmt, StmtGlobal, StmtImport, StmtImportFrom, StmtNonlocal};
use ruff_text_size::{Ranged, TextSize};

use crate::constants::{OYarn, SymType};
use crate::core::evaluation::{Context, ContextValue, Evaluation, EvaluationSymbol, EvaluationValue, EvaluationSymbolPtr, EvaluationSymbolWeak};
use crate::core::import_resolver;
use crate::core::odoo::SyncOdoo;
use crate::core::python_arch_eval_hooks::PythonArchEvalHooks;
use crate::core::symbols::module_symbol::ModuleSymbol;
use crate::threads::SessionInfo;
use crate::utils::compare_semver;
use crate::{oyarn, Sy, S};
use crate::core::symbols::symbol::Symbol;
use crate::features::features_utils::FeaturesUtils;
use crate::core::file_mgr::FileInfo;

use super::features_utils::TypeInfo;


#[allow(non_camel_case_types)]
#[derive(Debug, Clone)]
pub enum ExpectedType {
    MODEL_NAME,
    DOMAIN(Rc<RefCell<Symbol>>),
    DOMAIN_LIST(Rc<RefCell<Symbol>>),
    DOMAIN_OPERATOR,
    DOMAIN_FIELD(Rc<RefCell<Symbol>>),
    DOMAIN_COMPARATOR,
    CLASS(Rc<RefCell<Symbol>>),
    SIMPLE_FIELD(Option<OYarn>),
    NESTED_FIELD(Option<OYarn>),
    METHOD_NAME,
    INHERITS,
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
        let file_info_ast = file_info.borrow().file_info_ast.clone();
        let file_info_ast = file_info_ast.borrow();
        let ast = file_info_ast.ast.as_ref().unwrap();
        complete_vec_stmt(ast, session, file_symbol, offset).or_else(|| complete_name(session, file_symbol, offset, false, &S!("")))
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
    if !stmts.is_empty() && stmts.iter().last().unwrap().range().end().to_usize() >= offset {
        return complete_stmt(session, file_symbol, stmts.iter().last().unwrap(), offset);
    }
    //The user is writing after the last stmt
    None
}

fn complete_function_def_stmt(session: &mut SessionInfo<'_>, file: &Rc<RefCell<Symbol>>, stmt_function_def: &ruff_python_ast::StmtFunctionDef, offset: usize) -> Option<CompletionResponse> {
    for decorator in stmt_function_def.decorator_list.iter(){
        if let Some(result) = complete_decorator_call(session, file, offset, decorator, &stmt_function_def.range.start()){
            return Some(result);
        }
    }
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
    let mut expected_type = vec![];
    if stmt_assign.targets.len() == 1 {
        if let Some(target_name) = stmt_assign.targets.first().unwrap().as_name_expr() {
            match target_name.id.as_str() {
                "_inherit" => expected_type.push(ExpectedType::MODEL_NAME),
                "_inherits" => expected_type.push(ExpectedType::INHERITS),
                _ => {}
            }
        }
    }
    if offset > stmt_assign.value.range().start().to_usize() && offset <= stmt_assign.value.range().end().to_usize() {
        return complete_expr( &stmt_assign.value, session, file, offset, false, &expected_type);
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
                    label: name.to_string(),
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
                    label: name.to_string(),
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
                    label: name.to_string(),
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
        Expr::Set(expr_set) => complete_set(session, file, expr_set, offset, is_param, expected_type),
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
        Expr::TString(_) => None,
        Expr::StringLiteral(expr_string_literal) => complete_string_literal(session, file, expr_string_literal, offset, is_param, expected_type),
        Expr::BytesLiteral(_) => None,
        Expr::NumberLiteral(_) => None,
        Expr::BooleanLiteral(_) => None,
        Expr::NoneLiteral(_) => None,
        Expr::EllipsisLiteral(_) => None,
        Expr::Attribute(expr_attribute) => complete_attribut(session, file, expr_attribute, offset, is_param, expected_type),
        Expr::Subscript(expr_subscript) => complete_subscript(session, file, expr_subscript, offset, is_param, expected_type),
        Expr::Starred(_) => None,
        Expr::Name(expr_name) => complete_name_expression(session, file, expr_name, offset, is_param, expected_type),
        Expr::List(expr_list) => complete_list(session, file, expr_list, offset, is_param, expected_type),
        Expr::Tuple(expr_tuple) => complete_tuple(session, file, expr_tuple, offset, is_param, expected_type),
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
        if let Some(dict_item_key) = &dict_item.key {
            // For expected type INHERITS, we want to complete the model name for the key
            // and a simple field of type Many2one for the value
            if offset > dict_item_key.range().start().to_usize() && offset <= dict_item_key.range().end().to_usize() {
                let expected_type= expected_type.iter().map(|e| match e {
                    ExpectedType::INHERITS => ExpectedType::MODEL_NAME,
                    _ => e.clone(),
                }).collect();
                return complete_expr( dict_item_key, session, file, offset, is_param, &expected_type);
            }
            if offset > dict_item.value.range().start().to_usize() && offset <= dict_item.value.range().end().to_usize() {
                // if expected type has model name, replace it with simple field
                // for _inherits completion
                let expected_type = expected_type.iter().map(|e| match e {
                    ExpectedType::INHERITS => ExpectedType::SIMPLE_FIELD(Some(Sy!("Many2one"))),
                    _ => e.clone(),
                }).collect();
                return complete_expr( &dict_item.value, session, file, offset, is_param, &expected_type);
            }
        }
    }
    None
}

fn complete_set(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_set: &ruff_python_ast::ExprSet, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    for set_item in expr_set.elts.iter() {
        if offset > set_item.range().start().to_usize() && offset <= set_item.range().end().to_usize() {
            // A set expression here is just starting to write the inherits dict
            let expected_type= expected_type.iter().map(|e| match e {
                ExpectedType::INHERITS => ExpectedType::MODEL_NAME,
                _ => e.clone(),
            }).collect();
            return complete_expr( set_item, session, file, offset, is_param, &expected_type);
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

fn complete_decorator_call(
    session: &mut SessionInfo,
    file: &Rc<RefCell<Symbol>>,
    offset: usize,
    decorator: &Decorator,
    max_infer: &TextSize,
) -> Option<CompletionResponse> {
    let (decorator_base, decorator_args) = match &decorator.expression {
        Expr::Call(call_expr) => {
            (&call_expr.func, &call_expr.arguments)
        },
        _ => {return None;}
    };
    if decorator_args.args.is_empty(){
        return None; // All the decorators we handle have at least one arg for now
    }
    let scope = Symbol::get_scope_symbol(file.clone(), offset as u32, false);
    let dec_evals = Evaluation::eval_from_ast(session, &decorator_base, scope.clone(), max_infer, false, &mut vec![]).0;
    let mut followed_evals = vec![];
    for eval in dec_evals {
        followed_evals.extend(Symbol::follow_ref(&eval.symbol.get_symbol(session, &mut None, &mut vec![], None), session, &mut None, true, false, None, &mut vec![]));
    }
    for decorator_eval in followed_evals{
        let EvaluationSymbolPtr::WEAK(decorator_eval_sym_weak) = decorator_eval else {
            continue;
        };
        let Some(dec_sym) = decorator_eval_sym_weak.weak.upgrade() else {
            continue;
        };
        let dec_sym_tree = dec_sym.borrow().get_tree();
        let version_comparison = compare_semver(session.sync_odoo.full_version.as_str(), "18.1.0");
        let expected_types = if (version_comparison < Ordering::Equal && dec_sym_tree.0.ends_with(&[Sy!("odoo"), Sy!("api")])) ||
                (version_comparison >= Ordering::Equal && dec_sym_tree.0.ends_with(&[Sy!("odoo"), Sy!("orm"), Sy!("decorators")])) {
            if [vec![Sy!("onchange")], vec![Sy!("constrains")]].contains(&dec_sym_tree.1) && SyncOdoo::is_in_main_entry(session, &dec_sym_tree.0) {
                &vec![ExpectedType::SIMPLE_FIELD(None)]
            } else if dec_sym_tree.1 == vec![Sy!("depends")] && SyncOdoo::is_in_main_entry(session, &dec_sym_tree.0){
                &vec![ExpectedType::NESTED_FIELD(None)]
            } else {
                continue;
            }
        } else {
            continue;
        };
        // if dec_sym_tree == (vec![S!("odoo"), S!("api")], vec![S!("returns")]){
        //     // Todo
        // } else
        for arg in decorator_args.args.iter() {
            if offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize() {
                return complete_expr(arg, session, file, offset, false, &expected_types);
            }
        }
    }
    None
}

fn complete_call(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_call: &ruff_python_ast::ExprCall, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    if offset > expr_call.func.range().start().to_usize() && offset <= expr_call.func.range().end().to_usize() {
        return complete_expr( &expr_call.func, session, file, offset, is_param, expected_type);
    }
    let scope = Symbol::get_scope_symbol(file.clone(), offset as u32, is_param);
    let callable_evals = Evaluation::eval_from_ast(session, &expr_call.func, scope, &expr_call.func.range().start(), false, &mut vec![]).0;
    for (arg_index, arg) in expr_call.arguments.args.iter().enumerate() {
        if offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize() {
            for callable_eval in callable_evals.iter() {
                let callable = callable_eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None);
                let Some(callable_sym) = callable.weak.upgrade()  else {continue};
                match callable_sym.borrow().typ(){
                    SymType::FUNCTION => {
                        let func = callable_sym.borrow();
                        let func = func.as_func();
                        let func_arg = func.get_indexed_arg_in_call(
                            expr_call,
                            arg_index as u32,
                            callable.context.get(&S!("is_attr_of_instance")).map(|v| v.as_bool()));
                        if let Some(func_arg) = func_arg {
                            if let Some(func_arg_sym) = func_arg.symbol.upgrade() {
                                let mut expected_type = vec![];
                                for evaluation in func_arg_sym.borrow().evaluations().unwrap().iter() {
                                    match evaluation.symbol.get_symbol_ptr() {
                                        EvaluationSymbolPtr::WEAK(_weak) => {
                                            //if weak, use get_symbol
                                            let symbol=  evaluation.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None);
                                            if let Some(evaluation) = symbol.weak.upgrade() {
                                                if evaluation.borrow().typ() == SymType::CLASS {
                                                    expected_type.push(ExpectedType::CLASS(evaluation.clone()));
                                                }
                                            }
                                        },
                                        EvaluationSymbolPtr::DOMAIN => {
                                            if let Some(parent) = callable.context.get(&S!("base_attr"))
                                                .and_then(|parent_value| parent_value.as_symbol().upgrade()) {
                                                expected_type.push(ExpectedType::DOMAIN(parent));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                return complete_expr(arg, session, file, offset, is_param, &expected_type);
                            }
                        }
                    },
                    SymType::CLASS => {
                        // check for completion of first positional argument for comodel_name
                        if arg_index != 0 || !callable_sym.borrow().is_specific_field_class(session, &["Many2one", "One2many", "Many2many"]) {
                            break;
                        }
                        return complete_expr(arg, session, file, offset, is_param, &vec![ExpectedType::MODEL_NAME]);
                    },
                    _ => {}
                };
            }
            //if we didn't find anything, still try to complete
            return complete_expr(arg, session, file, offset, is_param, &vec![]);
        }
    }
    for keyword in expr_call.arguments.keywords.iter(){
        if offset <= keyword.value.range().start().to_usize() || offset > keyword.value.range().end().to_usize() {
            continue;
        }
        for callable_eval in callable_evals.iter() {
            let callable = callable_eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None);
            let Some(callable_sym) = callable.weak.upgrade() else {continue};
            if callable_sym.borrow().typ() != SymType::CLASS || !callable_sym.borrow().is_field_class(session){
                continue;
            }
            let Some(expected_type) = keyword.arg.as_ref().and_then(|kw_arg_id|
                match kw_arg_id.id.as_str() {
                    "related" => Some(vec![ExpectedType::NESTED_FIELD(Some(oyarn!("{}", callable_sym.borrow().name())))]),
                    "comodel_name" => if callable_sym.borrow().is_specific_field_class(session, &["Many2one", "One2many", "Many2many"]){
                            Some(vec![ExpectedType::MODEL_NAME])
                        } else {
                            None
                        },
                    "inverse" | "search" | "compute" => Some(vec![ExpectedType::METHOD_NAME]),
                    _ => None,
                }
            ) else {
                continue;
            };
            return complete_expr(&keyword.value, session, file, offset, is_param, &expected_type);
        }
        return complete_expr(&keyword.value, session, file, offset, is_param, &vec![]);
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
                            let model_class_syms = model.borrow().get_main_symbols(session, None);
                            let modules = model_class_syms.iter().flat_map(|model_rc|
                                model_rc.borrow().find_module());
                            let required_modules = modules.filter(|module|
                                !ModuleSymbol::is_in_deps(session, &current_module, &module.borrow().as_module_package().dir_name));
                            let dep_names: Vec<OYarn> = required_modules.map(|module| module.borrow().as_module_package().dir_name.clone()).collect();
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
                                sort_text = Some(label.to_string());
                            };
                        }

                        items.push(CompletionItem {
                            label: label.to_string(),
                            insert_text,
                            kind: Some(lsp_types::CompletionItemKind::CLASS),
                            label_details,
                            sort_text,
                            ..Default::default()
                    });
                    }
                }
            },
            ExpectedType::DOMAIN(parent) => {},
            ExpectedType::DOMAIN_OPERATOR => {
                for operator in vec!["!", "&", "|"].iter() {
                    items.push(CompletionItem {
                        label: operator.to_string(),
                        insert_text: None,
                        kind: Some(lsp_types::CompletionItemKind::CLASS),
                        label_details: None,
                        sort_text: None,
                        ..Default::default()
                    });
                }
            },
            ExpectedType::DOMAIN_LIST(parent) => {},
            ExpectedType::DOMAIN_COMPARATOR => {
                for (operator, sort_text) in vec![("=", "a"), ("!=", "b"), (">", "c"), (">=", "d"), ("<", "e"), ("<=", "f"), ("=?", "g"),  ("like", "h"), ("=like", "i"), ("not like", "j"), ("ilike", "k"),
                    ("=ilike", "l"),  ("not ilike", "m"),  ("in", "n"),  ("not in", "o"), ("child_of", "p"), ("parent_of", "q"), ("any", "r"), ("not any", "s")].iter() {
                    items.push(CompletionItem {
                        label: operator.to_string(),
                        insert_text: None,
                        kind: Some(lsp_types::CompletionItemKind::CLASS),
                        label_details: None,
                        sort_text: Some(sort_text.to_string()),
                        ..Default::default()
                    });
                }
            },
            ExpectedType::DOMAIN_FIELD(parent) => {
                add_nested_field_names(session, &mut items, current_module.clone(), expr_string_literal.value.to_str(), parent.clone(), true, &None);
            },
            ExpectedType::SIMPLE_FIELD(_) | ExpectedType::NESTED_FIELD(_) | ExpectedType::METHOD_NAME => 'field_block:  {
                let scope = Symbol::get_scope_symbol(file.clone(), expr_string_literal.range().start().to_u32(), true);
                let Some(parent_class) = scope.borrow().get_in_parents(&vec![SymType::CLASS], true).and_then(|p| p.upgrade()) else {
                    break 'field_block;
                };
                if parent_class.borrow().as_class_sym()._model.is_none(){
                    break 'field_block;
                }
                match expected_type {
                    ExpectedType::SIMPLE_FIELD(maybe_field_type) =>  add_model_attributes(
                        session, &mut items, current_module.clone(), parent_class, false, true, false, expr_string_literal.value.to_str(), maybe_field_type),
                    ExpectedType::METHOD_NAME =>  add_model_attributes(
                        session, &mut items, current_module.clone(), parent_class, false, false, true, expr_string_literal.value.to_str(), &None),
                    ExpectedType::NESTED_FIELD(maybe_field_type) => add_nested_field_names(
                        session, &mut items, current_module.clone(), expr_string_literal.value.to_str(), parent_class, false, maybe_field_type),
                    _ => unreachable!()
                }
            },
            ExpectedType::CLASS(_) => {},
            ExpectedType::INHERITS => {},
        }
    }
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items: items
    }))
}

fn complete_attribut(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, attr: &ExprAttribute, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    let mut items = vec![];
    let start_expr = attr.range.start().to_u32();
    //TODO actually using start_expr instead of offset, because when we complete an attr, like "self.", the ast is invalid, preventing any rebuild
    //As symbols are not rebuilt, boundaries are not rights, and a "return self." at the end of a function/class body would be out of scope.
    //Temporary, by using the start of expr, we can hope that it is still in the right scope.
    let scope = Symbol::get_scope_symbol(file.clone(), start_expr, is_param);
    if offset > attr.value.range().start().to_usize() && offset <= attr.value.range().end().to_usize() {
        return complete_expr( &attr.value, session, file, offset, is_param, expected_type);
    } else {
        let parent = Evaluation::eval_from_ast(session, &attr.value, scope.clone(), &attr.range().start(), false, &mut vec![]).0;

        let from_module = file.borrow().find_module().clone();
        for parent_eval in parent.iter() {
            //TODO shouldn't we set and clean context here?
            let parent_sym_eval = parent_eval.symbol.get_symbol(session, &mut None, &mut vec![], Some(scope.clone()));
            if !parent_sym_eval.is_expired_if_weak() {
                let parent_sym_types = Symbol::follow_ref(&parent_sym_eval, session, &mut None, false, false, None, &mut vec![]);
                for parent_sym_type in parent_sym_types.iter() {
                    let Some(parent_sym) = parent_sym_type.upgrade_weak() else {continue};
                    add_model_attributes(session, &mut items, from_module.clone(), parent_sym, parent_sym_eval.as_weak().is_super, false, false, attr.attr.id.as_str(), &None)
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
    let subscripted = Evaluation::eval_from_ast(session, &expr_subscript.value, scope.clone(), &expr_subscript.value.range().start(), false, &mut vec![]).0;
    for eval in subscripted.iter() {
        let eval_symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], Some(scope.clone()));
        if !eval_symbol.is_expired_if_weak() {
            let symbol_types = Symbol::follow_ref(&eval_symbol, session, &mut None, false, false, None, &mut vec![]);
            for symbol_type in symbol_types.iter() {
                if let Some(symbol_type) = symbol_type.upgrade_weak() {
                    let borrowed = symbol_type.borrow();
                    let get_item = borrowed.get_symbol(&(vec![], vec![Sy!("__getitem__")]), u32::MAX);
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

fn complete_name_expression(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_name: &ExprName, offset: usize, is_param: bool, _expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    if expr_name.range.end().to_usize() == offset {
        complete_name(session, file, offset, is_param, &expr_name.id.to_string())
    } else {
        None
    }
}

fn complete_name(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, offset: usize, is_param: bool, name: &String) -> Option<CompletionResponse> {
    let scope = Symbol::get_scope_symbol(file.clone(), offset as u32, is_param);
    let symbols = Symbol::get_all_inferred_names(&scope, name, offset as u32);
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items: symbols.into_iter().map(|(_symbol_name, symbols)| {
            build_completion_item_from_symbol(session, symbols, HashMap::new())
        }).collect::<Vec<_>>(),
    }))
}

fn complete_list(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_list: &ruff_python_ast::ExprList, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    _complete_list_or_tuple(session, file, &expr_list.elts, offset, is_param, expected_type)
}

pub fn complete_tuple(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, expr_tuple: &ruff_python_ast::ExprTuple, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    _complete_list_or_tuple(session, file, &expr_tuple.elts, offset, is_param, expected_type)
}

pub fn _complete_list_or_tuple(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>, list_or_tuple_elts: &Vec<Expr>, offset: usize, is_param: bool, expected_type: &Vec<ExpectedType>) -> Option<CompletionResponse> {
    for expected_type in expected_type.iter() {
        match expected_type {
            ExpectedType::DOMAIN(parent) => {
                for expr in list_or_tuple_elts.iter() {
                    if offset > expr.range().start().to_usize() && offset <= expr.range().end().to_usize() {
                        match expr {
                            Expr::StringLiteral(expr_string_literal) => {
                                return complete_string_literal(session, file, expr_string_literal, offset, is_param, &vec![ExpectedType::DOMAIN_OPERATOR]);
                            },
                            Expr::Tuple(t) => {
                                return complete_expr(expr, session, file, offset, is_param, &vec![ExpectedType::DOMAIN_LIST(parent.clone())]);
                            },
                            Expr::List(l) => {
                                return complete_expr(expr, session, file, offset, is_param, &vec![ExpectedType::DOMAIN_LIST(parent.clone())]);
                            }
                            _ => {}
                        }
                    }
                }
            },
            ExpectedType::DOMAIN_LIST(parent) => {
                if list_or_tuple_elts.len() == 0 {
                    if let Some(completion) = session.sync_odoo.capabilities.text_document.as_ref()
                            .and_then(|capability_text_doc| capability_text_doc.completion.as_ref())
                            .and_then(|completion| completion.completion_item.as_ref()){
                        if completion.snippet_support.unwrap_or(false) {
                            return Some(CompletionResponse::List(CompletionList {
                                is_incomplete: false,
                                items: vec![CompletionItem {
                                    label: "(field, comparator, value)".to_string(),
                                    kind: Some(lsp_types::CompletionItemKind::CLASS),
                                    insert_text: Some("$1, ${2|\"=\",\"!=\",\">\",\">=\",\"<\",\"<=\",\"=?\",\"like\",\"=like\",\"not like\",\"ilike\",\"=ilike\",\"not ilike\",\"in\",\"not in\",\"child_of\",\"parent_of\",\"any\",\"not any\"|}, $3".to_string()),
                                    insert_text_format: Some(lsp_types::InsertTextFormat::SNIPPET),
                                    ..Default::default()
                                }]
                            }))
                        }
                    }
                }
                for (index, expr) in list_or_tuple_elts.iter().enumerate() {
                    if offset > expr.range().start().to_usize() && offset <= expr.range().end().to_usize() {
                        let expected_type = match index {
                            0 => vec![ExpectedType::DOMAIN_FIELD(parent.clone())],
                            1 => vec![ExpectedType::DOMAIN_COMPARATOR],
                            _ => vec![],
                        };
                        return complete_expr(expr, session, file, offset, is_param, &expected_type);
                    }
                }
            }
            ExpectedType::MODEL_NAME => { //In case of Model_name, transfer this expected type to items. It is used in _inherit = [""] for example, but can maybe be wrong elsewhere?
                for expr in list_or_tuple_elts.iter() {
                    if offset > expr.range().start().to_usize() && offset <= expr.range().end().to_usize() {
                        return complete_expr(expr, session, file, offset, is_param, &vec![ExpectedType::MODEL_NAME]);
                    }
                }
            }
            _ => {}
        }
    }
    if expected_type.is_empty() {
        for expr in list_or_tuple_elts.iter() {
            if offset > expr.range().start().to_usize() && offset < expr.range().end().to_usize() {
                return complete_expr( expr, session, file, offset, is_param, expected_type);
            }
        }
    }
    None
}

/* *********************************************************************
**************************** Common utils ******************************
********************************************************************** */

fn add_nested_field_names(
    session: &mut SessionInfo,
    items: &mut Vec<CompletionItem>,
    from_module: Option<Rc<RefCell<Symbol>>>,
    field_prefix: &str,
    parent: Rc<RefCell<Symbol>>,
    add_date_completions: bool,
    specific_field_type: &Option<OYarn>,
){
    let split_expr: Vec<String> = field_prefix.split(".").map(|x| x.to_string()).collect();
    let mut obj = Some(parent.clone());
    let mut date_mode = false;
    for (index, name) in split_expr.iter().enumerate() {
        if add_date_completions && date_mode {
            if index != split_expr.len() - 1 {
                break;
            }
            for value in ["year_number", "quarter_number", "month_number", "iso_week_number", "day_of_week", "day_of_month", "day_of_year", "hour_number", "minute_number", "second_number"] {
                if value.starts_with(name) {
                    items.push(CompletionItem {
                        label: value.to_string(),
                        insert_text: None,
                        kind: Some(lsp_types::CompletionItemKind::VARIABLE),
                        label_details: None,
                        sort_text: None,
                        ..Default::default()
                    });
                }
            }
            date_mode = false;
            continue;
        }
        if obj.is_none() {
            break;
        }
        if let Some(object) = &obj {
            if index == split_expr.len() - 1 {
                let all_symbols = Symbol::all_members(&object, session,  true, true, false, from_module.clone(), false);
                for (_symbol_name, symbols) in all_symbols {
                    //we could use symbol_name to remove duplicated names, but it would hide functions vs variables
                    if _symbol_name.starts_with(name) {
                        let mut found_one = false;
                        for (final_sym, dep) in symbols.iter() { //search for at least one that is a field
                            if dep.is_none() && (specific_field_type.is_none() || final_sym.borrow().is_specific_field(session, &["Many2one", "One2many", "Many2many", specific_field_type.as_ref().unwrap().as_str()])){
                                items.push(build_completion_item_from_symbol(session, vec![final_sym.clone()], HashMap::new()));
                                found_one = true;
                                continue;
                            }
                        }
                        if found_one {
                            continue;
                        }
                    }
                }
            } else {
                let (symbols, _diagnostics) = object.borrow().get_member_symbol(session,
                    &name.to_string(),
                    from_module.clone(),
                    false,
                    true,
                    true,
                    false);
                if symbols.is_empty() {
                    break;
                }
                obj = None;
                for s in symbols.iter() {
                    if s.borrow().is_specific_field(session, &["Many2one", "One2many", "Many2many"]) && s.borrow().typ() == SymType::VARIABLE{
                        let models = s.borrow().as_variable().get_relational_model(session, from_module.clone());
                        //only handle it if there is only one main symbol for this model
                        if models.len() == 1 {
                            obj = Some(models[0].clone());
                            break;
                        }
                    }
                    if add_date_completions && s.borrow().is_specific_field(session, &["Date"]) {
                        date_mode = true;
                        break;
                    }
                }
            }
        }
    }
}

fn add_model_attributes(
    session: &mut SessionInfo,
    items: &mut Vec<CompletionItem>,
    from_module: Option<Rc<RefCell<Symbol>>>,
    parent_sym: Rc<RefCell<Symbol>>,
    is_super: bool,
    only_fields: bool,
    only_methods: bool,
    attribute_name: &str,
    specific_field_type: &Option<OYarn>,
){
    let all_symbols = Symbol::all_members(&parent_sym, session, true, only_fields, only_methods, from_module.clone(), is_super);
    for (_symbol_name, symbols) in all_symbols {
        //we could use symbol_name to remove duplicated names, but it would hide functions vs variables
        let Some((final_sym, _dep)) = symbols.first() else {
            continue;
        };
        if let Some(field_type) = specific_field_type {
            if !final_sym.borrow().is_specific_field(session, &[field_type.as_str()]) {
                continue;
            }
        }
        if _symbol_name.starts_with(attribute_name) {
            let context_of_symbol = HashMap::from([(S!("base_attr"), ContextValue::SYMBOL(Rc::downgrade(&parent_sym)))]);
            items.push(build_completion_item_from_symbol(session, vec![final_sym.clone()], context_of_symbol));
        }
    }
}

fn build_completion_item_from_symbol(session: &mut SessionInfo, symbols: Vec<Rc<RefCell<Symbol>>>, context_of_symbol: Context) -> CompletionItem {
    if symbols.is_empty() {
        return CompletionItem::default();
    }
    //TODO use dependency to show it? or to filter depending of configuration
    let typ = symbols.iter().flat_map(|symbol|
        Symbol::follow_ref(&&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
            Rc::downgrade(symbol),
            None,
            false,
        )), session, &mut None, false, false, None, &mut vec![])
    ).collect::<Vec<_>>();
    let type_details = typ.iter().map(|eval|
        FeaturesUtils::get_inferred_types(session, eval, &mut Some(context_of_symbol.clone()), &symbols[0].borrow().typ())
    ).collect::<HashSet<_>>();
    let label_details_description = match type_details.len() {
        0 => None,
        1 => Some(match &type_details.iter().next().unwrap() {
            TypeInfo::CALLABLE(c) => c.return_types.clone(),
            TypeInfo::VALUE(v) => v.clone(),
        }),
        _ => Some(format!("{} types", type_details.len())),
    };

    CompletionItem {
        label: symbols[0].borrow().name().to_string(),
        label_details: Some(CompletionItemLabelDetails {
            detail: None,
            description: label_details_description,
        }),
        detail: Some(type_details.iter().map(|detail| detail.to_string()).join(" | ").to_string()),
        kind: Some(get_completion_item_kind(&symbols[0])),
        sort_text: Some(get_sort_text_for_symbol(&symbols[0])),
        documentation: Some(
            lsp_types::Documentation::MarkupContent(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: FeaturesUtils::build_markdown_description(session, None, &symbols.iter().map(|symbol|
                    Evaluation {
                        symbol: EvaluationSymbol::new_with_symbol(Rc::downgrade(symbol), None,
                            context_of_symbol.clone(),
                            None),
                        value: None,
                        range: None
                    }).collect::<Vec<_>>(),
                    &None, None)
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
        SymType::DISK_DIR => CompletionItemKind::FOLDER,
        SymType::NAMESPACE => CompletionItemKind::FOLDER,
        SymType::PACKAGE(_) => CompletionItemKind::MODULE,
        SymType::FILE => CompletionItemKind::FILE,
        SymType::COMPILED => CompletionItemKind::FILE,
        SymType::VARIABLE => CompletionItemKind::VARIABLE,
        SymType::CLASS => CompletionItemKind::CLASS,
        SymType::FUNCTION => CompletionItemKind::FUNCTION,
        SymType::XML_FILE => CompletionItemKind::FILE,
        SymType::CSV_FILE => CompletionItemKind::FILE,
    }
}