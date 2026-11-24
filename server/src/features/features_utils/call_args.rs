use itertools::Itertools;
use ruff_python_ast::{Arguments, Expr, ExprCall, ExprStringLiteral};
use ruff_text_size::Ranged;
use crate::core::symbols::symbol_keys::{ModelSymbolKey, ModuleKey, SymbolKey, VariableKey};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::FunctionSymbol;

use crate::constants::SymType;
use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak};
use crate::core::evaluation_context::ContextKey;
use crate::threads::SessionInfo;

use super::api_decorators::{classify_api_decorator, find_decorator_arg_symbols};
use super::field_params::resolve_field_param;
use super::field_paths;
use super::model_members;
use super::order;
use super::special_args::SpecialArg;
use super::string_resolution::{SegmentPick, StringResolution};

/// Resolve `string` in an argument of `call_expr`, merging the results of all the callables.
pub(super) fn resolve_call_argument(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    pick: SegmentPick,
) -> Option<StringResolution> {
    let argument = find_argument(&call_expr.arguments, string)?;
    let (_, value, keyword_name) = argument;
    if keyword_name
        .is_some_and(|name| !SpecialArg::can_be_special_keyword(session, name, value, string))
    {
        return None;
    }
    resolve_callables(session, scope, call_expr)
        .into_iter()
        .filter_map(|(callable, callable_sym)| {
            resolve_string_argument(
                session,
                scope,
                from_module,
                string,
                call_expr,
                argument,
                &callable,
                callable_sym,
                pick,
            )
        })
        .reduce(StringResolution::merge)
}

/// Find the argument containing `string`.
/// Returns its index (keyword arguments are counted after the positional ones), its value,
/// and its name if it is a keyword argument.
fn find_argument<'a>(
    arguments: &'a Arguments,
    string: &ExprStringLiteral,
) -> Option<(usize, &'a Expr, Option<&'a str>)> {
    if let Some((index, arg)) = arguments
        .args
        .iter()
        .find_position(|arg| arg.range().contains_range(string.range))
    {
        Some((index, arg, None))
    } else if let Some((index, keyword)) = arguments.keywords.iter()
        .find_position(|keyword| keyword.range().contains_range(string.range))
        // `**kwargs` has no name
        && let Some(name) = keyword.arg.as_ref()
    {
        Some((
            arguments.args.len() + index,
            &keyword.value,
            Some(name.as_str()),
        ))
    } else {
        None
    }
}

/// Evaluate and follow the refs of the function being called in `call_expr`.
/// Returns the `EvaluationSymbolWeak` and its corresponding `SymbolKey` for each callable found.
fn resolve_callables(
    session: &mut SessionInfo,
    scope: SymbolKey,
    call_expr: &ExprCall,
) -> Vec<(EvaluationSymbolWeak, SymbolKey)> {
    let evaluations = Evaluation::eval_from_ast(
        session,
        &call_expr.func,
        scope,
        &call_expr.func.range().start(),
        false,
        &mut vec![],
    )
    .0;
    let mut callables = vec![];
    for eval in evaluations {
        let followed_evals = SymbolTable::follow_ref(
            &eval.symbol.get_symbol(session, None, &mut vec![], None),
            session,
            None,
            true,
            false,
            None,
            None,
        );
        callables.extend(followed_evals.into_iter().filter_map(|followed_eval| {
            match followed_eval {
                EvaluationSymbolPtr::SELF(callable) | EvaluationSymbolPtr::WEAK(callable) => {
                    callable
                        .weak
                        .upgrade(session.st())
                        .map(|callable_sym| (callable, callable_sym))
                }
                _ => None,
            }
        }));
    }
    callables
}

/// Try to resolve the meaning of the `string` argument in the context of the callable.
/// e.g. a domain or a field in an Odoo model.
fn resolve_string_argument(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    (arg_index, value, keyword_name): (usize, &Expr, Option<&str>),
    callable: &EvaluationSymbolWeak,
    callable_sym: SymbolKey,
    pick: SegmentPick,
) -> Option<StringResolution> {
    if let SymbolKey::Function(function_key) = callable_sym
        && session.st()[function_key].is_property
    {
        return None;
    }
    if let Some(decorator) = classify_api_decorator(session, callable_sym) {
        return StringResolution::from_members(find_decorator_arg_symbols(session, scope, from_module, string, decorator, pick));
    }

    let param = find_call_param(
        session,
        from_module,
        callable,
        callable_sym,
        call_expr,
        arg_index,
    );
    match SpecialArg::classify_call_argument(session, callable_sym, param, keyword_name)
        .filter(|special_arg| special_arg.is_in_value(value, string))?
    {
        SpecialArg::Domain => {
            let model = model_called_on(session, callable)?;
            StringResolution::from_members(field_paths::find_nested_fields(session, model, from_module, string, pick))
        },
        SpecialArg::Order => {
            let model = model_called_on(session, callable)?;
            StringResolution::from_members(order::find_order_fields(session, model, from_module, string, pick))
        },
        SpecialArg::Field(field_param) => {
            resolve_field_param(session, scope, from_module, string, call_expr, field_param, pick)
        },
        SpecialArg::RecordValuesList | SpecialArg::RecordValues => {
            let model = model_called_on(session, callable)?;
            StringResolution::from_members(
                model_members::find_fields(session, model, from_module, string.value.to_str())
                    .into_iter()
                    .map(|symbol| (symbol, string.range))
                    .collect()
            )
        },
    }
}

/// Returns the parameter the argument at `arg_index` in `call_expr` is given to.
/// `arg_index` counts the keyword arguments after the positional ones.
pub fn find_call_param(
    session: &mut SessionInfo,
    from_module: Option<ModuleKey>,
    callable: &EvaluationSymbolWeak,
    callable_sym: SymbolKey,
    call_expr: &ExprCall,
    arg_index: usize,
) -> Option<VariableKey> {
    let func_key = match callable_sym {
        SymbolKey::Function(function_key) => function_key,
        SymbolKey::Class(_) => {
            let Some(&SymbolKey::Function(init_method)) = SymbolTable::get_member_symbol(
                session,
                callable_sym,
                "__init__",
                from_module,
                false,
                false,
                true,
                false,
                false,
            )
            .0
            .first() else {
                return None;
            };
            init_method
        }
        _ => return None,
    };
    let is_on_instance = if callable_sym.typ() == SymType::CLASS {
        // skip self on __init__
        Some(true)
    } else {
        callable.context.get(ContextKey::IsAttrOfInstance).map(|v| v.as_bool())
    };
    let argument = FunctionSymbol::get_indexed_arg_in_call(
        session.st(),
        func_key,
        call_expr,
        arg_index as u32,
        is_on_instance,
    )?;
    argument.symbol.upgrade(session.st())
}

/// The model the callable is called on, e.g. `res.partner` in `self.env['res.partner'].search(...)`.
/// Tries to fetch the model from the `BaseAttr` context of the callable.
pub fn model_called_on(
    session: &SessionInfo,
    callable: &EvaluationSymbolWeak,
) -> Option<ModelSymbolKey> {
    callable
        .context
        .get(ContextKey::BaseAttr)?
        .as_symbol()
        .upgrade(session.st())?
        .as_model_symbol_key()
}
