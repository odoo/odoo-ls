use super::features_utils::TypeInfo;
use crate::constants::{OYarn, SymType};
use crate::core::evaluation::{
    Evaluation, EvaluationSymbol, EvaluationSymbolPtr, EvaluationSymbolWeak, HookName
};
use crate::core::evaluation_context::{Context, ContextKey, ContextValue};
use crate::core::evaluation_utils::DeepFieldEvalWalker;
use crate::core::file_mgr::FileInfo;
use crate::core::import_resolver;
use crate::core::odoo::SyncOdoo;
use crate::core::python_odoo_builder::ACCESS_OPERATOR_OPTIONS;
use crate::core::symbols::storage::xml::xml_field_symbol::XmlFieldName;
use crate::core::symbols::{FunctionSymbol, ModuleSymbol};
use crate::core::symbols::symbol_keys::{ClassKey, ModuleKey, SourceFileKey, SymbolKey};
use crate::core::symbols::storage::SymbolTable;
use crate::features::ast_utils::AstUtils;
use crate::features::features_utils::FeaturesUtils;
use crate::threads::SessionInfo;
use crate::tree::OYarnExt;
use crate::{Sy, S};
use itertools::Itertools;
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionList, CompletionResponse, MarkupContent,
};
use ruff_python_ast::{
    Decorator, ExceptHandler, Expr, ExprAttribute, ExprIf, ExprName, ExprSlice, ExprSubscript,
    ExprYield, Stmt, StmtGlobal, StmtImport, StmtImportFrom, StmtNonlocal,
};
use ruff_text_size::{Ranged, TextSize};
use crate::utils::HashSet;
use std::{cell::RefCell, rc::Rc};


#[allow(non_camel_case_types)]
#[derive(Debug, Clone)]
pub enum ExpectedType {
    MODEL_NAME,
    DOMAIN(SymbolKey),
    DOMAIN_LIST(SymbolKey),
    DOMAIN_OPERATOR,
    DOMAIN_FIELD(SymbolKey),
    DOMAIN_COMPARATOR,
    DOMAIN_ACCESS_VALUE,
    CLASS(ClassKey),
    SIMPLE_FIELD(Option<OYarn>),
    NESTED_FIELD(Option<OYarn>),
    EXTERNAL_FIELD(OYarn), // Like in inverse_name='field_name', we attach the comodel_name
    METHOD_NAME,
    INHERITS,
}

pub struct CompletionFeature;

impl CompletionFeature {

    pub fn autocomplete(session: &mut SessionInfo,
        file_symbol: SourceFileKey,
        file_info: &Rc<RefCell<FileInfo>>,
        completion_context: Option<CompletionContext>,
        line: u32,
        character: u32
    ) -> Option<CompletionResponse> {
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let file_info_ast = file_info.borrow().file_info_ast.clone();
        let file_info_ast = file_info_ast.borrow();
        let ast = file_info_ast.get_stmts().unwrap();
        let is_completion_invoked = completion_context.as_ref().is_none_or(|context| {
            context.trigger_kind == lsp_types::CompletionTriggerKind::INVOKED
        });
        complete_vec_stmt(ast, session, file_symbol, offset).or_else(|| {
            if is_completion_invoked {
                // Only complete names on empty result if invoked manually not with trigger character
                // This avoid autocompleting on every dot or parenthesis or comma, which is not always wanted
                complete_name(session, file_symbol, offset, false, "")
            } else {
                None
            }
        })
    }
}

/* **********************************************************************
***************************** Statements ********************************
*********************************************************************** */

fn complete_stmt(session: &mut SessionInfo, file: SourceFileKey, stmt: &Stmt, offset: usize) -> Option<CompletionResponse> {
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
        Stmt::Expr(stmt_expr) => complete_expr(&stmt_expr.value, session, file, offset, false, &[]),
        Stmt::Pass(_) => None,
        Stmt::Break(_) => None,
        Stmt::Continue(_) => None,
        Stmt::IpyEscapeCommand(_) => None,
    }
}

fn complete_vec_stmt(stmts: &[Stmt], session: &mut SessionInfo, file_symbol: SourceFileKey, offset: usize) -> Option<CompletionResponse> {
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
    if let Some(last_statement) = previous
        && last_statement.end().to_usize() >= offset
    {
        return complete_stmt(session, file_symbol, last_statement, offset);
    }
    //The user is writing after the last stmt
    None
}

fn complete_function_def_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_function_def: &ruff_python_ast::StmtFunctionDef, offset: usize) -> Option<CompletionResponse> {
    for decorator in stmt_function_def.decorator_list.iter(){
        if let Some(result) = complete_decorator_call(session, file, offset, decorator, &stmt_function_def.range.start()){
            return Some(result);
        }
    }
    if !stmt_function_def.body.is_empty()
        && offset > stmt_function_def.body.first().unwrap().range().start().to_usize() && stmt_function_def.body.last().unwrap().range().end().to_usize() >= offset
    {
        return complete_vec_stmt(&stmt_function_def.body, session, file, offset);
    }
    None
}

fn complete_class_def_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_class_def: &ruff_python_ast::StmtClassDef, offset: usize) -> Option<CompletionResponse> {
    if let Some(base) = stmt_class_def.bases().iter().find(|base| offset > base.range().start().to_usize() && offset <= base.range().end().to_usize()) {
        return complete_expr( base, session, file, offset, false, &[]); //TODO only classes?
    }
    if !stmt_class_def.body.is_empty()
        && offset > stmt_class_def.body.first().unwrap().range().start().to_usize() && stmt_class_def.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_class_def.body, session, file, offset);
        }
    None
}

fn complete_return_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_return: &ruff_python_ast::StmtReturn, offset: usize) -> Option<CompletionResponse> {
    if let Some(expr) = stmt_return.value.as_ref() {
        return complete_expr( expr, session, file, offset, false, &[]);
    }
    None
}

fn complete_delete_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_delete: &ruff_python_ast::StmtDelete, offset: usize) -> Option<CompletionResponse> {
    let target = stmt_delete.targets.iter().find(|target| offset > target.range().start().to_usize() && offset <= target.range().end().to_usize())?;
    complete_expr( target, session, file, offset, false, &[])
}

fn complete_assign_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_assign: &ruff_python_ast::StmtAssign, offset: usize) -> Option<CompletionResponse> {
    let mut expected_type = vec![];
    if stmt_assign.targets.len() == 1
        && let Some(target_name) = stmt_assign.targets.first().unwrap().as_name_expr() {
            match target_name.id.as_str() {
                "_inherit" => expected_type.push(ExpectedType::MODEL_NAME),
                "_inherits" => expected_type.push(ExpectedType::INHERITS),
                _ => {}
            }
        }
    if offset > stmt_assign.value.range().start().to_usize() && offset <= stmt_assign.value.range().end().to_usize() {
        return complete_expr( &stmt_assign.value, session, file, offset, false, &expected_type);
    }
    None
}

fn complete_aug_assign_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_aug_assign: &ruff_python_ast::StmtAugAssign, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_aug_assign.value.range().start().to_usize() && offset <= stmt_aug_assign.value.range().end().to_usize() {
        return complete_expr( &stmt_aug_assign.value, session, file, offset, false, &[]);
    }
    None
}

fn complete_ann_assign_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_ann_assign: &ruff_python_ast::StmtAnnAssign, offset: usize) -> Option<CompletionResponse> {
    if let Some(expr) = stmt_ann_assign.value.as_ref()
        && offset > expr.range().start().to_usize() && offset <= expr.range().end().to_usize() {
            return complete_expr( expr, session, file, offset, false, &[]);
        }
    None
}

fn complete_type_alias_stmt(_session: &mut SessionInfo<'_>, _file: SourceFileKey, _stmt_type_alias: &ruff_python_ast::StmtTypeAlias, _offset: usize) -> Option<CompletionResponse> {
    None
}

fn complete_for_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_for: &ruff_python_ast::StmtFor, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_for.iter.range().start().to_usize() && offset <= stmt_for.iter.range().end().to_usize() {
        return complete_expr( &stmt_for.iter, session, file, offset, false, &[]);
    }
    if !stmt_for.body.is_empty()
        && offset > stmt_for.body.first().unwrap().range().start().to_usize() && stmt_for.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_for.body, session, file, offset);
        }
    None
}

fn complete_while_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_while: &ruff_python_ast::StmtWhile, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_while.test.range().start().to_usize() && offset <= stmt_while.test.range().end().to_usize() {
        return complete_expr( &stmt_while.test, session, file, offset, false, &[]);
    }
    if !stmt_while.body.is_empty()
        && offset > stmt_while.body.first().unwrap().range().start().to_usize() && stmt_while.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_while.body, session, file, offset);
        }
    None
}

fn complete_if_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_if: &ruff_python_ast::StmtIf, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_if.test.range().start().to_usize() && offset <= stmt_if.test.range().end().to_usize() {
        return complete_expr( &stmt_if.test, session, file, offset, false, &[]);
    }
    if !stmt_if.body.is_empty()
        && offset > stmt_if.body.first().unwrap().range().start().to_usize() && stmt_if.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_if.body, session, file, offset);
        }
    None
}

fn complete_with_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_with: &ruff_python_ast::StmtWith, offset: usize) -> Option<CompletionResponse> {
    //TODO complete with items
    // if stmt_with.items.len() > 0 {
    //     for item in stmt_with.items.iter() {
    //         if offset > item.range().start().to_usize() && offset < item.range().end().to_usize() {
    //             return complete_expr( item, session, file, offset);
    //         }
    //     }
    // }
    if !stmt_with.body.is_empty()
        && offset > stmt_with.body.first().unwrap().range().start().to_usize() && stmt_with.body.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_with.body, session, file, offset);
        }
    None
}

fn complete_match_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_match: &ruff_python_ast::StmtMatch, offset: usize) -> Option<CompletionResponse> {
    let case = stmt_match.cases.iter().find(|case| {
        !case.body.is_empty()
            && offset > case.body.first().as_ref().unwrap().range().start().to_usize() && offset <= case.body.last().as_ref().unwrap().range().end().to_usize()
    })?;
    complete_vec_stmt(&case.body, session, file, offset)
}

fn complete_raise_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_raise: &ruff_python_ast::StmtRaise, offset: usize) -> Option<CompletionResponse> {
    if let Some(exc) = &stmt_raise.exc
        && offset > exc.range().start().to_usize() && offset <= exc.range().end().to_usize()
    {
        return complete_expr( exc, session, file, offset, false, &[]);
    }
    None
}

fn complete_try_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_try: &ruff_python_ast::StmtTry, offset: usize) -> Option<CompletionResponse> {
    if !stmt_try.body.is_empty()
        && offset > stmt_try.body.first().unwrap().range().start().to_usize() && stmt_try.body.last().unwrap().range().end().to_usize() >= offset
    {
        return complete_vec_stmt(&stmt_try.body, session, file, offset);
    }
    let handler_hit = stmt_try.handlers.iter().find_map(|handler| {
        match handler {
            ExceptHandler::ExceptHandler(except_handler_except_handler) => {
                (offset > except_handler_except_handler.range().start().to_usize() && except_handler_except_handler.range().end().to_usize() >= offset)
                    .then(|| complete_vec_stmt(&except_handler_except_handler.body, session, file, offset))
            },
        }
    });
    if let Some(result) = handler_hit {
        return result;
    }
    if !stmt_try.orelse.is_empty()
        && offset > stmt_try.orelse.first().unwrap().range().start().to_usize() && stmt_try.orelse.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_try.orelse, session, file, offset);
        }
    if !stmt_try.finalbody.is_empty()
        && offset > stmt_try.finalbody.first().unwrap().range().start().to_usize() && stmt_try.finalbody.last().unwrap().range().end().to_usize() >= offset {
            return complete_vec_stmt(&stmt_try.finalbody, session, file, offset);
        }
    None
}

fn complete_assert_stmt(session: &mut SessionInfo<'_>, file: SourceFileKey, stmt_assert: &ruff_python_ast::StmtAssert, offset: usize) -> Option<CompletionResponse> {
    if offset > stmt_assert.test.as_ref().range().start().to_usize() && offset <= stmt_assert.test.as_ref().range().end().to_usize() {
        return complete_expr( stmt_assert.test.as_ref(), session, file, offset, false, &[]);
    }
    if let Some(msg) = &stmt_assert.msg
        && offset > msg.range().start().to_usize() && offset <= msg.range().end().to_usize() {
            return complete_expr( msg, session, file, offset, false, &[]);
        }
    None
}

fn complete_import_stmt(session: &mut SessionInfo, file: SourceFileKey, stmt_import: &StmtImport, offset: usize) -> Option<CompletionResponse> {
    let mut items = vec![];
    if let Some(alias) = stmt_import.names.iter().find(|alias| alias.name.range().start().to_usize() < offset && alias.name.range.end().to_usize() >= offset) {
        let to_complete = alias.name.id.to_string().get(0 .. offset - alias.name.range.start().to_usize()).unwrap_or("").to_string();
        let names = import_resolver::get_all_valid_names(session, file, None, to_complete, 0, false);
        for (name, sym_typ) in names {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(get_completion_item_kind(&sym_typ)),
                ..Default::default()
            });
        }
    }
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items
    }))
}

fn complete_import_from_stmt(session: &mut SessionInfo, file: SourceFileKey, stmt_import: &StmtImportFrom, offset: usize) -> Option<CompletionResponse> {
    let mut items = vec![];
    if let Some(module) = stmt_import.module.as_ref()
        && module.range.start().to_usize() < offset && module.range.end().to_usize() >= offset {
            let to_complete = module.id.to_string().get(0 .. offset - module.range.start().to_usize()).unwrap_or("").to_string();
            let names = import_resolver::get_all_valid_names(session, file, Some(to_complete), S!(""), stmt_import.level, true);
            for (name, sym_type) in names {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(get_completion_item_kind(&sym_type)),
                    ..Default::default()
                });
            }
        }
    if let Some(alias) = stmt_import.names.iter().find(|alias| alias.name.range().start().to_usize() < offset && alias.name.range.end().to_usize() >= offset) {
        let to_complete = alias.name.id.to_string().get(0 .. offset - alias.name.range.start().to_usize()).unwrap_or("").to_string();
        let names = import_resolver::get_all_valid_names(session, file, stmt_import.module.as_ref().map(|m| m.id.to_string()), to_complete, stmt_import.level, false);
        for (name, sym_type) in names {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(get_completion_item_kind(&sym_type)),
                ..Default::default()
            });
        }
    }
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items
    }))
}

fn complete_global_stmt(_session: &mut SessionInfo, _file: SourceFileKey, _stmt_global: &StmtGlobal, _offset: usize) -> Option<CompletionResponse> {
    None
}

fn complete_nonlocal_stmt(_session: &mut SessionInfo, _file: SourceFileKey, _stmt_nonlocal: &StmtNonlocal, _offset: usize) -> Option<CompletionResponse> {
    None
}

/* *********************************************************************
**************************** Expressions *******************************
********************************************************************* */

fn complete_expr(expr: &Expr, session: &mut SessionInfo, file: SourceFileKey, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
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
        Expr::Slice(expr_slice) => complete_slice(session, file, expr_slice, offset, is_param, expected_type),
        Expr::IpyEscapeCommand(_) => None,
    }
}

fn compare_bool_op(session: &mut SessionInfo, file: SourceFileKey, expr_bool_op: &ruff_python_ast::ExprBoolOp, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    let value = expr_bool_op.values.iter().find(|value| offset > value.range().start().to_usize() && offset <= value.range().end().to_usize())?;
    complete_expr( value, session, file, offset, is_param, expected_type)
}

fn compare_named(session: &mut SessionInfo, file: SourceFileKey, expr_named: &ruff_python_ast::ExprNamed, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    if offset > expr_named.value.range().start().to_usize() && offset <= expr_named.value.range().end().to_usize() {
        return complete_expr( &expr_named.value, session, file, offset, is_param, expected_type);
    }
    None
}

fn compare_bin_op(session: &mut SessionInfo, file: SourceFileKey, expr_bin_op: &ruff_python_ast::ExprBinOp, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    if offset > expr_bin_op.left.range().start().to_usize() && offset <= expr_bin_op.left.range().end().to_usize() {
        return complete_expr( &expr_bin_op.left, session, file, offset, is_param, expected_type);
    }
    if offset > expr_bin_op.right.range().start().to_usize() && offset <= expr_bin_op.right.range().end().to_usize() {
        return complete_expr( &expr_bin_op.right, session, file, offset, is_param, expected_type);
    }
    None
}

fn compare_unary_op(session: &mut SessionInfo, file: SourceFileKey, expr_unary_op: &ruff_python_ast::ExprUnaryOp, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    if offset > expr_unary_op.operand.range().start().to_usize() && offset <= expr_unary_op.operand.range().end().to_usize() {
        return complete_expr( &expr_unary_op.operand, session, file, offset, is_param, expected_type);
    }
    None
}

fn compare_lambda(session: &mut SessionInfo, file: SourceFileKey, expr_lambda: &ruff_python_ast::ExprLambda, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    if offset > expr_lambda.body.range().start().to_usize() && offset <= expr_lambda.body.range().end().to_usize() {
        return complete_expr( &expr_lambda.body, session, file, offset, is_param, expected_type);
    }
    None
}

//Expr if, used in "a if b else c"
fn complete_if_expr(session: &mut SessionInfo, file: SourceFileKey, expr_if: &ExprIf, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
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

fn complete_dict(session: &mut SessionInfo, file: SourceFileKey, expr_dict: &ruff_python_ast::ExprDict, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    expr_dict.items.iter().find_map(|dict_item| {
        let dict_item_key = dict_item.key.as_ref()?;
        // For expected type INHERITS, we want to complete the model name for the key
        // and a simple field of type Many2one for the value
        if offset > dict_item_key.range().start().to_usize() && offset <= dict_item_key.range().end().to_usize() {
            let expected_type= expected_type.iter().map(|e| match e {
                ExpectedType::INHERITS => ExpectedType::MODEL_NAME,
                _ => e.clone(),
            }).collect::<Vec<_>>();
            return Some(complete_expr( dict_item_key, session, file, offset, is_param, &expected_type));
        }
        if offset > dict_item.value.range().start().to_usize() && offset <= dict_item.value.range().end().to_usize() {
            // if expected type has model name, replace it with simple field
            // for _inherits completion
            let expected_type = expected_type.iter().map(|e| match e {
                ExpectedType::INHERITS => ExpectedType::SIMPLE_FIELD(Some(Sy!("Many2one"))),
                _ => e.clone(),
            }).collect::<Vec<_>>();
            return Some(complete_expr( &dict_item.value, session, file, offset, is_param, &expected_type));
        }
        None
    }).flatten()
}

fn complete_set(session: &mut SessionInfo, file: SourceFileKey, expr_set: &ruff_python_ast::ExprSet, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    let set_item = expr_set.elts.iter().find(|set_item| offset > set_item.range().start().to_usize() && offset <= set_item.range().end().to_usize())?;
    // A set expression here is just starting to write the inherits dict
    let expected_type= expected_type.iter().map(|e| match e {
        ExpectedType::INHERITS => ExpectedType::MODEL_NAME,
        _ => e.clone(),
    }).collect::<Vec<_>>();
    complete_expr(set_item, session, file, offset, is_param, &expected_type)
}

fn complete_yield(session: &mut SessionInfo, file: SourceFileKey, expr_yield: &ExprYield, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    if let Some(expr) = &expr_yield.value
        && offset > expr.range().start().to_usize() && offset <= expr.range().end().to_usize()
    {
        return complete_expr(expr, session, file, offset, is_param, expected_type);
    }
    None
}

fn complete_compare(session: &mut SessionInfo, file: SourceFileKey, expr_compare: &ruff_python_ast::ExprCompare, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    if offset > expr_compare.left.range().start().to_usize() && offset <= expr_compare.left.range().end().to_usize() {
        return complete_expr( &expr_compare.left, session, file, offset, is_param, expected_type);
    }
    let expr = expr_compare.comparators.iter().find(|expr| offset > expr.range().start().to_usize() && offset <= expr.range().end().to_usize())?;
    complete_expr( expr, session, file, offset, is_param, expected_type)
}

fn complete_decorator_call(
    session: &mut SessionInfo,
    file: SourceFileKey,
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
    let scope = session.st().get_scope_symbol(file, offset as u32, false);
    AstUtils::build_scope(session, scope);
    let dec_evals = Evaluation::eval_from_ast(session, decorator_base, scope, max_infer, false, &mut vec![]).0;
    let mut followed_evals = vec![];
    for eval in dec_evals {
        followed_evals.extend(
            SymbolTable::follow_ref(&eval.symbol.get_symbol(session, None, &mut vec![], None), session, None, true, false, None, None)
        );
    }
    for decorator_eval in followed_evals{
        let EvaluationSymbolPtr::WEAK(decorator_eval_sym_weak) = decorator_eval else {
            continue;
        };
        let Some(dec_sym) = decorator_eval_sym_weak.weak.upgrade(session.st()) else {
            continue;
        };
        let dec_sym_tree = session.st().get_tree(dec_sym);
        let is_18_1_or_later = session.sync_odoo.version >= (18, 1);
        let expected_types = if (!is_18_1_or_later && dec_sym_tree.0.ends_with_strs(&["odoo", "api"])) ||
                (is_18_1_or_later && dec_sym_tree.0.ends_with_strs(&["odoo", "orm", "decorators"])) {
            if (dec_sym_tree.1 == ["onchange"] || dec_sym_tree.1 == ["constrains"]) && SyncOdoo::is_in_main_entry(session, &dec_sym_tree.0) {
                &[ExpectedType::SIMPLE_FIELD(None)]
            } else if dec_sym_tree.1 == ["depends"] && SyncOdoo::is_in_main_entry(session, &dec_sym_tree.0){
                &[ExpectedType::NESTED_FIELD(None)]
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
                return complete_expr(arg, session, file, offset, false, expected_types);
            }
        }
    }
    None
}

fn complete_call(session: &mut SessionInfo, file: SourceFileKey, expr_call: &ruff_python_ast::ExprCall, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    if offset > expr_call.func.range().start().to_usize() && offset <= expr_call.func.range().end().to_usize() {
        return complete_expr( &expr_call.func, session, file, offset, is_param, expected_type);
    }
    let scope = session.st().get_scope_symbol(file, offset as u32, is_param);
    let from_module = session.st().find_module(file);
    AstUtils::build_scope(session, scope);
    let callable_evals = Evaluation::eval_from_ast(session, &expr_call.func, scope, &expr_call.func.range().start(), false, &mut vec![]).0;
    let callable_eval_sym_ptrs = callable_evals.iter().flat_map(|callable_eval|
        SymbolTable::follow_ref(&callable_eval.symbol.get_symbol(session, None, &mut vec![], None), session, None, false, false, None, None)
    ).collect::<Vec<_>>();
    if let Some((arg_index, arg)) = expr_call.arguments.args.iter().find_position(|arg|
        offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize())
    {
        for callable_eval in callable_eval_sym_ptrs.iter() {
            let EvaluationSymbolPtr::WEAK(callable) = callable_eval else {
                continue;
            };
            let Some(callable_sym) = callable.weak.upgrade(session.st())  else {continue};
            if ![SymType::FUNCTION, SymType::CLASS].contains(&callable_sym.typ()){
                continue;
            }
            // Relational fields first argument is a model name.
            if callable_sym.typ() == SymType::CLASS
            && arg_index == 0
            && SymbolTable::is_specific_field_class(session, callable_sym, &["Many2one", "One2many", "Many2many"]) {
                    return complete_expr(arg, session, file, offset, is_param, &[ExpectedType::MODEL_NAME]);
            }
            // if class get __init__ method, we need to get the argument from there
            let func_key = if callable_sym.typ() == SymType::CLASS {
                if let Some(&SymbolKey::Function(init_method)) = SymbolTable::get_member_symbol(session, callable_sym, "__init__", from_module, false, false, true, false, false).0.first() {
                    init_method
                } else {
                    continue;
                }
            } else {
                callable_sym.unwrap_function_key()
            };
            let is_on_instance = if callable_sym.typ() == SymType::CLASS {
                Some(true)
            } else {
                callable.context.get(ContextKey::IsAttrOfInstance).map(|v| v.as_bool())
            };
            let Some(func_arg_sym) = FunctionSymbol::get_indexed_arg_in_call(
                session.st(),
                func_key,
                expr_call,
                arg_index as u32,
                is_on_instance)
                .and_then(|func_arg| func_arg.symbol.upgrade(session.st())) else {
                continue;
            };
            let mut expected_type = vec![];
            if session.st()[func_arg_sym].name == "inverse_name"
                && callable_sym.typ() == SymType::CLASS
                && SymbolTable::is_specific_field_class(session, callable_sym, &["One2many"]) {
                let comodel_name_option = match expr_call.arguments.args.first() {
                    Some(Expr::StringLiteral(expr)) => Some(ExpectedType::EXTERNAL_FIELD(Sy!(expr.value.to_string()))),
                    _ => expr_call
                        .arguments
                        .keywords
                        .iter()
                        .find(|kw|
                            kw.arg.as_ref().map(|arg| arg.id == "comodel_name").unwrap_or(false)
                        )
                        .and_then(|kw| match &kw.value {
                            Expr::StringLiteral(expr) => Some(ExpectedType::EXTERNAL_FIELD(Sy!(expr.value.to_string()))),
                            _ => None
                        })
                };
                if let Some(comodel_name) = comodel_name_option {
                    expected_type.push(comodel_name);
                }
            } else {
                for evaluation in session.st()[func_arg_sym].evaluations.clone() {
                    match evaluation.symbol.get_symbol_ptr() {
                        EvaluationSymbolPtr::WEAK(_weak) => {
                            //if weak, use get_symbol
                            let symbol =  evaluation.symbol.get_symbol_as_weak(session, None, &mut vec![], None);
                            if let Some(evaluation) = symbol.weak.upgrade(session.st())
                                && let SymbolKey::Class(class) = evaluation {
                                    expected_type.push(ExpectedType::CLASS(class));
                                }
                        },
                        EvaluationSymbolPtr::DOMAIN => {
                            if let Some(parent) = callable.context.get(ContextKey::BaseAttr)
                                .and_then(|parent_value| parent_value.as_symbol().upgrade(session.st())) {
                                expected_type.push(ExpectedType::DOMAIN(parent));
                            }
                            return complete_expr(arg, session, file, offset, is_param, &expected_type);
                        }
                        _ => {}
                    }
                }
            }
            return complete_expr(arg, session, file, offset, is_param, &expected_type);
        }
        //if we didn't find anything, still try to complete
        return complete_expr(arg, session, file, offset, is_param, &[]);
    }
    let keyword = expr_call.arguments.keywords.iter().find(|arg| {
        offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize()
    })?;
    for callable_eval_sym_ptr in callable_eval_sym_ptrs.iter() {
        let callable_option = callable_eval_sym_ptr.upgrade_weak(session.st());
        let Some(callable_sym) = callable_option else {continue};
        if callable_sym.typ() != SymType::CLASS || !SymbolTable::is_field_class(session, callable_sym){
            continue;
        }
        let Some(expected_type) = keyword.arg.as_ref().and_then(|kw_arg_id|
            match kw_arg_id.id.as_str() {
                "related" => Some(vec![ExpectedType::NESTED_FIELD(Some(session.st().name(callable_sym).clone()))]),
                "comodel_name" => if SymbolTable::is_specific_field_class(session, callable_sym, &["Many2one", "One2many", "Many2many"]) {
                        Some(vec![ExpectedType::MODEL_NAME])
                    } else {
                        None
                    },
                "inverse_name" => {
                    if let Some(Expr::StringLiteral(expr)) = expr_call.arguments.args.first() {
                        Some(vec![ExpectedType::EXTERNAL_FIELD(Sy!(expr.value.to_string()))])
                    } else {
                        expr_call.arguments.keywords.iter().find(|kw| kw.arg.as_ref().map(|arg| arg.id == "comodel_name").unwrap_or(false))
                        .and_then(|kw| match &kw.value {
                            Expr::StringLiteral(expr) => Some(vec![ExpectedType::EXTERNAL_FIELD(Sy!(expr.value.to_string()))]),
                            _ => None
                        })
                    }
                },
                "inverse" | "search" | "compute" => Some(vec![ExpectedType::METHOD_NAME]),
                _ => None,
            }
        ) else {
            continue;
        };
        return complete_expr(&keyword.value, session, file, offset, is_param, &expected_type);
    }
    complete_expr(&keyword.value, session, file, offset, is_param, &[])
}

fn complete_string_literal(session: &mut SessionInfo, file: SourceFileKey, expr_string_literal: &ruff_python_ast::ExprStringLiteral, _offset: usize, _is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    let mut items = vec![];
    let current_module = session.st().find_module(file);
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
                    if !model.borrow_mut().has_symbols(session.st()) {
                        continue;
                    }
                    if model_name.starts_with(prefix) && model_name != "_unknown" {
                        let label = model_name.clone();
                        let insert_text = model_name.strip_prefix(prefix_head).map(|s| s.to_string());
                        let mut label_details = None;
                        let mut sort_text = Some(format!("_{}", label.clone()));


                        if let Some(current_module) = current_module {
                            let model_ref = model.borrow();
                            let model_class_definitions = model_ref.get_main_symbols(session, None);
                            let modules = model_class_definitions.flat_map(|model_key|
                                session.st().find_module(model_key));
                            let required_modules = modules.filter(|&module|
                                !ModuleSymbol::is_in_deps(session.st(), current_module, &session.st()[module].dir_name));
                            let dep_names: Vec<OYarn> = required_modules.map(|module| session.st()[module].dir_name.clone()).collect();
                            if !dep_names.is_empty() {
                                if !session.sync_odoo.config.ac_filter_model_names(){
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
            ExpectedType::DOMAIN(_) => {},
            ExpectedType::DOMAIN_OPERATOR => {
                for operator in ["!", "&", "|"].iter() {
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
            ExpectedType::DOMAIN_LIST(_) => {},
            ExpectedType::DOMAIN_COMPARATOR => {
                const BASE_OPERATORS: &[(&str, &str)] = &[("=", "a"), ("!=", "b"), (">", "c"), (">=", "d"),
                    ("<", "e"), ("<=", "f"), ("=?", "g"),  ("like", "h"), ("=like", "i"), ("not like", "j"),
                    ("ilike", "k"), ("=ilike", "l"),  ("not ilike", "m"),  ("in", "n"),  ("not in", "o"),
                    ("child_of", "p"), ("parent_of", "q"), ("any", "r"), ("not any", "s")];
                let extra: &[(&str, &str)] = if session.sync_odoo.version >= (19, 3) {
                   &[("access", "t")]
                } else {
                    &[]
                };
                for (operator, sort_text) in BASE_OPERATORS.iter().chain(extra) {
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
            ExpectedType::DOMAIN_ACCESS_VALUE => {
                for access_value in ACCESS_OPERATOR_OPTIONS {
                    items.push(CompletionItem {
                        label: access_value.to_string(),
                        insert_text: None,
                        kind: Some(lsp_types::CompletionItemKind::TEXT),
                        label_details: None,
                        sort_text: None,
                        ..Default::default()
                    });
                }
            },
            ExpectedType::DOMAIN_FIELD(parent) => {
                add_nested_field_names(session, &mut items, current_module, expr_string_literal.value.to_str(), *parent, true, &None);
            },
            ExpectedType::SIMPLE_FIELD(_) | ExpectedType::NESTED_FIELD(_) | ExpectedType::METHOD_NAME => 'field_block:  {
                let scope = session.st().get_scope_symbol(file, expr_string_literal.range().start().to_u32(), true);
                AstUtils::build_scope(session, scope);
                let Some(SymbolKey::Class(parent_class)) = session.st().get_in_parents(scope, &[SymType::CLASS], true) else {
                    break 'field_block;
                };
                if session.st()[parent_class]._model.is_none() {
                    break 'field_block;
                }
                match expected_type {
                    ExpectedType::SIMPLE_FIELD(maybe_field_type) => add_model_attributes(
                        session, &mut items, current_module, parent_class.into(), false, true, false, expr_string_literal.value.to_str(), maybe_field_type),
                    ExpectedType::METHOD_NAME =>  add_model_attributes(
                        session, &mut items, current_module, parent_class.into(), false, false, true, expr_string_literal.value.to_str(), &None),
                    ExpectedType::NESTED_FIELD(maybe_field_type) => add_nested_field_names(
                        session, &mut items, current_module, expr_string_literal.value.to_str(), parent_class.into(), false, maybe_field_type),
                    _ => unreachable!()
                }
            },
            ExpectedType::EXTERNAL_FIELD(model_name) => {
                let Some(model) = session.sync_odoo.models.get(model_name).cloned() else {
                    break;
                };
                // Only python main symbols, because it is relation defining check
                // Needs deploying Odoo and checking
                let main_syms = model.borrow().get_main_symbols(session, current_module).collect::<Vec<_>>();
                main_syms.iter().filter_map(|s| s.as_class_key()).for_each(|class_key| {
                    add_model_attributes(session, &mut items, current_module, class_key.into(), false, true, false, expr_string_literal.value.to_str(), &Some(Sy!("Many2one")))
                });
            },
            ExpectedType::CLASS(_) => {},
            ExpectedType::INHERITS => {},
        }
    }
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items
    }))
}

fn complete_attribut(session: &mut SessionInfo, file: SourceFileKey, attr: &ExprAttribute, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    let mut items = vec![];
    let start_expr = attr.range.start().to_u32();
    //TODO actually using start_expr instead of offset, because when we complete an attr, like "self.", the ast is invalid, preventing any rebuild
    //As symbols are not rebuilt, boundaries are not rights, and a "return self." at the end of a function/class body would be out of scope.
    //Temporary, by using the start of expr, we can hope that it is still in the right scope.
    let scope = session.st().get_scope_symbol(file, start_expr, is_param);
    AstUtils::build_scope(session, scope);
    if offset > attr.value.range().start().to_usize() && offset <= attr.value.range().end().to_usize() {
        return complete_expr( &attr.value, session, file, offset, is_param, expected_type);
    } else {
        let parent = Evaluation::eval_from_ast(session, &attr.value, scope, &attr.range().start(), false, &mut vec![]).0;

        let from_module = session.st().find_module(file);
        for parent_eval in parent.iter() {
            //TODO shouldn't we set and clean context here?
            let parent_sym_eval = parent_eval.symbol.get_symbol(session, None, &mut vec![], Some(scope));
            if !parent_sym_eval.is_expired_if_weak(session.st()) {
                let parent_sym_types = SymbolTable::follow_ref(&parent_sym_eval, session, None, false, false, None, None);
                for parent_sym_type in parent_sym_types.iter() {
                    let Some(parent_sym) = parent_sym_type.upgrade_weak(session.st()) else {continue};
                    add_model_attributes(session, &mut items, from_module, parent_sym, parent_sym_eval.get_weak().is_super, false, false, attr.attr.id.as_str(), &None)
                }
            }
        }
    }
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items
    }))
}

fn complete_subscript(session: &mut SessionInfo, file: SourceFileKey, expr_subscript: &ExprSubscript, offset: usize, is_param: bool, _expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    let scope = session.st().get_scope_symbol(file, offset as u32, is_param);
    AstUtils::build_scope(session, scope);
    let subscripted = Evaluation::eval_from_ast(session, &expr_subscript.value, scope, &expr_subscript.value.range().start(), false, &mut vec![]).0;
    for eval in subscripted.iter() {
        let eval_symbol = eval.symbol.get_symbol(session, None, &mut vec![], Some(scope));
        if !eval_symbol.is_expired_if_weak(session.st()) {
            let symbol_types = SymbolTable::follow_ref(&eval_symbol, session, None, false, false, None, None);
            for symbol_type in symbol_types.iter() {
                if let Some(symbol_type) = symbol_type.upgrade_weak(session.st()) {
                    let get_item = session.st().get_symbol(symbol_type, (&[], &["__getitem__"]), u32::MAX);
                    if let Some(&get_item) = get_item.last() {
                        let evaluations = session.st().evaluations(get_item).unwrap();
                        if evaluations.len() == 1 {
                            let get_item_eval = evaluations.first().unwrap();
                            if get_item_eval.symbol.get_symbol_hook.as_ref().map(|hook| hook.name == HookName::EvalEnvGetItem).unwrap_or_default() {
                                return complete_expr(&expr_subscript.slice, session, file, offset, is_param, &[ExpectedType::MODEL_NAME]);
                            }
                        }
                    }
                }
            }
        }
    }
    complete_expr(&expr_subscript.slice, session, file, offset, false, &[])
}

fn complete_name_expression(session: &mut SessionInfo, file: SourceFileKey, expr_name: &ExprName, offset: usize, is_param: bool, _expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    if expr_name.range.end().to_usize() == offset {
        complete_name(session, file, offset, is_param, &expr_name.id)
    } else {
        None
    }
}

fn complete_name(session: &mut SessionInfo, file: SourceFileKey, offset: usize, is_param: bool, name: &str) -> Option<CompletionResponse> {
    let scope = session.st().get_scope_symbol(file, offset as u32, is_param);
    AstUtils::build_scope(session, scope);
    let symbols = session.st().get_all_inferred_names(scope, name, offset as u32);
    Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items: symbols.into_iter().map(|(symbol_name, symbols)| {
            build_completion_item_from_symbol(session, symbols, &symbol_name, Context::default())
        }).collect::<Vec<_>>(),
    }))
}

fn complete_list(session: &mut SessionInfo, file: SourceFileKey, expr_list: &ruff_python_ast::ExprList, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    complete_list_or_tuple(session, file, &expr_list.elts, offset, is_param, expected_type)
}

pub fn complete_tuple(session: &mut SessionInfo, file: SourceFileKey, expr_tuple: &ruff_python_ast::ExprTuple, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    complete_list_or_tuple(session, file, &expr_tuple.elts, offset, is_param, expected_type)
}

fn complete_list_or_tuple(session: &mut SessionInfo, file: SourceFileKey, list_or_tuple_elts: &[Expr], offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    for expected_type in expected_type.iter() {
        match expected_type {
            &ExpectedType::DOMAIN(parent) => {
                if let Some(expr) = list_or_tuple_elts.iter().find(|expr| offset > expr.range().start().to_usize() && offset <= expr.range().end().to_usize()) {
                    match expr {
                        Expr::StringLiteral(expr_string_literal) => {
                            return complete_string_literal(session, file, expr_string_literal, offset, is_param, &[ExpectedType::DOMAIN_OPERATOR]);
                        },
                        Expr::Tuple(_) => {
                            return complete_expr(expr, session, file, offset, is_param, &[ExpectedType::DOMAIN_LIST(parent)]);
                        },
                        Expr::List(_) => {
                            return complete_expr(expr, session, file, offset, is_param, &[ExpectedType::DOMAIN_LIST(parent)]);
                        }
                        _ => {}
                    }
                }
            },
            &ExpectedType::DOMAIN_LIST(parent) => {
                if list_or_tuple_elts.is_empty()
                    && let Some(completion) = session.sync_odoo.capabilities.text_document.as_ref()
                            .and_then(|capability_text_doc| capability_text_doc.completion.as_ref())
                            .and_then(|completion| completion.completion_item.as_ref())
                        && completion.snippet_support.unwrap_or(false) {
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
                let mut access_op = false;
                for (index, expr) in list_or_tuple_elts.iter().enumerate() {
                    access_op |= index == 1
                        && session.sync_odoo.version >= (19, 3)
                        && expr
                            .as_string_literal_expr()
                            .map(|expr_string_literal| {
                                expr_string_literal.value.to_str() == "access"
                            })
                            .unwrap_or(false);
                    if offset > expr.range().start().to_usize() && offset <= expr.range().end().to_usize() {
                        let expected_type = match index {
                            0 => vec![ExpectedType::DOMAIN_FIELD(parent)],
                            1 => vec![ExpectedType::DOMAIN_COMPARATOR],
                            2 if access_op => vec![ExpectedType::DOMAIN_ACCESS_VALUE],
                            _ => vec![],
                        };
                        return complete_expr(expr, session, file, offset, is_param, &expected_type);
                    }
                }
            }
            ExpectedType::MODEL_NAME => { //In case of Model_name, transfer this expected type to items. It is used in _inherit = [""] for example, but can maybe be wrong elsewhere?
                if let Some(expr) = list_or_tuple_elts.iter().find(|expr| offset > expr.range().start().to_usize() && offset <= expr.range().end().to_usize()) {
                    return complete_expr(expr, session, file, offset, is_param, &[ExpectedType::MODEL_NAME]);
                }
            }
            _ => {}
        }
    }
    if expected_type.is_empty()
        && let Some(expr) = list_or_tuple_elts.iter().find(|expr| offset > expr.range().start().to_usize() && offset < expr.range().end().to_usize())
    {
        return complete_expr( expr, session, file, offset, is_param, expected_type);
    }
    None
}

fn complete_slice(session: &mut SessionInfo, file: SourceFileKey, expr_slice: &ExprSlice, offset: usize, is_param: bool, expected_type: &[ExpectedType]) -> Option<CompletionResponse> {
    // And incomplete subscript is always a slice, so self.env["ffff is a slice with ffff as lower
    if let Some(expr) = expr_slice.lower.as_ref()
        && offset > expr.range().start().to_usize()
        && offset <= expr.range().end().to_usize()
    {
        return complete_expr(expr, session, file, offset, is_param, expected_type);
    }
    if let Some(expr) = expr_slice.upper.as_ref().or(expr_slice.step.as_ref())
        && offset > expr.range().start().to_usize()
        && offset <= expr.range().end().to_usize()
    {
        return complete_expr(expr, session, file, offset, is_param, &[]);
    }
    None
}
/* *********************************************************************
**************************** Common utils ******************************
********************************************************************** */

fn add_nested_field_names(
    session: &mut SessionInfo,
    items: &mut Vec<CompletionItem>,
    from_module: Option<ModuleKey>,
    field_prefix: &str,
    parent: SymbolKey,
    add_date_completions: bool,
    specific_field_type: &Option<OYarn>,
){
    let split_expr: Vec<_> = field_prefix.split(".").collect();
    let mut deep_field_walker = DeepFieldEvalWalker::new(parent, from_module);
    let mut date_mode = false;
    for (index, &name) in split_expr.iter().enumerate() {
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
        let Some(base_symbol) = deep_field_walker.get_model_symbol(session) else {
            break;
        };
        if index == split_expr.len() - 1 {
            let all_symbols = SymbolTable::all_members(
                base_symbol,
                session,
                true,
                true,
                false,
                from_module,
                false,
            );
            for (symbol_name, symbols) in all_symbols {
                //we could use symbol_name to remove duplicated names, but it would hide functions vs variables
                if symbol_name.starts_with(name) {
                    let mut found_one = false;
                    for final_sym in symbols.iter() {
                        if specific_field_type.is_none() || SymbolTable::is_specific_field(session, *final_sym, &["Many2one", "One2many", "Many2many", specific_field_type.as_ref().unwrap().as_str()]){
                            items.push(build_completion_item_from_symbol(session, vec![*final_sym], &symbol_name, Context::default()));
                            found_one = true;
                        }
                    }
                    if found_one {
                        continue;
                    }
                }
            }
        } else {
            let field_symbols = deep_field_walker.get_model_fields(session, base_symbol, name);
            if !add_date_completions {
                continue;
            }
            for symbol in field_symbols {
                match symbol {
                    SymbolKey::Variable(_) => {
                        if SymbolTable::is_specific_field(session, symbol, &["Date", "Datetime"]) {
                            date_mode = true;
                        }
                    }
                    SymbolKey::XmlRecord(key) => {
                        let Some(ttype) =
                            session.st()[key].get_field_text(XmlFieldName::Type, session.st())
                        else {
                            continue;
                        };
                        if ["date", "datetime"].contains(&ttype.as_str()) {
                            date_mode = true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn add_model_attributes(
    session: &mut SessionInfo,
    items: &mut Vec<CompletionItem>,
    from_module: Option<ModuleKey>,
    parent_sym: SymbolKey, // always ClassKey?
    is_super: bool,
    only_fields: bool,
    only_methods: bool,
    attribute_name: &str,
    specific_field_type: &Option<OYarn>,
){
    let all_symbols = SymbolTable::all_members(
        parent_sym,
        session,
        true,
        only_fields,
        only_methods,
        from_module,
        is_super,
    );
    for (symbol_name, symbols) in all_symbols {
        //we could use symbol_name to remove duplicated names, but it would hide functions vs variables
        let Some(final_sym) = symbols.first() else {
            continue;
        };
        if let Some(field_type) = specific_field_type
            && !SymbolTable::is_specific_field(session, *final_sym, &[field_type.as_str()])
        {
            continue;
        }
        if symbol_name.starts_with(attribute_name) {
            let context_of_symbol = Context::from_iter([(ContextKey::BaseAttr, ContextValue::SYMBOL(parent_sym.into()))]);
            items.push(build_completion_item_from_symbol(session, vec![*final_sym], &symbol_name, context_of_symbol));
        }
    }
}

fn build_completion_item_from_symbol(session: &mut SessionInfo, symbols: Vec<SymbolKey>, symbol_name: &str, context_of_symbol: Context) -> CompletionItem {
    if symbols.is_empty() {
        return CompletionItem::default();
    }
    //TODO use dependency to show it? or to filter depending of configuration
    let typ = symbols.iter().flat_map(|&symbol|
        SymbolTable::follow_ref(&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
            symbol,
            None,
            false,
        )), session, None, false, false, None, None)
    ).collect::<Vec<_>>();
    let type_details = typ.iter().map(|eval|
        FeaturesUtils::get_inferred_types(session, eval, Some(&context_of_symbol), &symbols[0].typ())
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
        label: symbol_name.to_string(),
        label_details: Some(CompletionItemLabelDetails {
            detail: None,
            description: label_details_description,
        }),
        detail: Some(type_details.iter().map(|detail| detail.to_string()).join(" | ").to_string()),
        kind: Some(get_completion_item_kind(&symbols[0].typ())),
        sort_text: Some(get_sort_text_for_symbol(session.st(), symbols[0])),
        documentation: Some(
            lsp_types::Documentation::MarkupContent(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: FeaturesUtils::build_markdown_description(session, None, None, &symbols.iter().map(|&symbol|
                    Evaluation {
                        symbol: EvaluationSymbol::new_with_symbol(symbol.into(), None,
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

fn get_sort_text_for_symbol(symbol_table: &SymbolTable, sym: SymbolKey/*, cl: Option<Rc<RefCell<Symbol>>>, cl_to_complete: Option<Rc<RefCell<Symbol>>>*/) -> String {
    // return the text used for sorting the result for "symbol". cl is the class owner of symbol, and cl_to_complete the class
    // of the symbol to complete
    // ~ is used as last char of ascii table and } before last one
    let base_dist = 0;
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
    let name = symbol_table.repr(sym);
    let mut text = "}".repeat(base_dist as usize)/* + cl_name.as_str()*/ + &name;
    if name.starts_with("_") {
        text = "~".to_string() + text.as_str();
    }
    if name.starts_with("__") {
        text = "~".to_string() + text.as_str();
    }
    text
}

fn get_completion_item_kind(typ: &SymType) -> CompletionItemKind {
    match typ {
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
        SymType::XML_RECORD => CompletionItemKind::CONSTANT,
        SymType::XML_FIELD => CompletionItemKind::CONSTANT,
        SymType::XML_MENUITEM => CompletionItemKind::CONSTANT,
        SymType::XML_TEMPLATE => CompletionItemKind::CONSTANT,
        SymType::XML_ASSET => CompletionItemKind::CONSTANT,
        SymType::XML_DELETE => CompletionItemKind::CONSTANT,
        SymType::JS_FILE => CompletionItemKind::FILE,
    }
}
