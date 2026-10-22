use itertools::Itertools;
use ruff_python_ast::{Arguments, Expr, ExprCall, ExprStringLiteral};
use ruff_text_size::{Ranged, TextRange};
use crate::core::symbols::symbol_keys::{ModelSymbolKey, ModuleKey, SymbolKey, VariableKey};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::FunctionSymbol;

use crate::constants::SymType;
use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak};
use crate::core::evaluation_context::ContextKey;
use crate::threads::SessionInfo;

use super::api_decorators::{api_decorator, find_decorator_arg_symbols};
use super::field_params::{find_field_param_symbols, FieldParam};
use super::field_paths;
use super::string_resolution::SegmentPick;

/// What the callee of `call_expr` refers to, each as its weak evaluation and its symbol.
fn find_callables(
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

/// The parameter of the callable that the argument at `arg_index` of `call_expr` is given to, the
/// keyword arguments being indexed after the positional ones.
fn find_call_param(
    session: &mut SessionInfo,
    from_module: Option<ModuleKey>,
    callable: &EvaluationSymbolWeak,
    callable_sym: SymbolKey,
    call_expr: &ExprCall,
    arg_index: usize,
) -> Option<VariableKey> {
    let func_key = match callable_sym {
        SymbolKey::Function(function_key) => function_key,
        // if class get __init__ method, we need to get the argument from there
        SymbolKey::Class(_) => {
            let Some(&SymbolKey::Function(init_method)) = SymbolTable::get_member_symbol(session, callable_sym, "__init__", from_module, false, false, true, false, false).0.first() else {
                return None;
            };
            init_method
        },
        _ => return None,
    };
    let is_on_instance = if callable_sym.typ() == SymType::CLASS {
        // skip self on __init__
        Some(true)
    } else {
        callable.context.get(ContextKey::IsAttrOfInstance).map(|v| v.as_bool())
    };
    FunctionSymbol::get_indexed_arg_in_call(session.st(), func_key, call_expr, arg_index as u32, is_on_instance)
        .and_then(|func_arg| func_arg.symbol.upgrade(session.st()))
}

/// The model the callable is called on, e.g. `res.partner` in `self.env['res.partner'].search(...)`.
fn model_called_on(session: &SessionInfo, callable: &EvaluationSymbolWeak) -> Option<ModelSymbolKey> {
    let base_sym = callable.context.get(ContextKey::BaseAttr)?.as_symbol().upgrade(session.st())?;
    match base_sym {
        SymbolKey::Class(class_key) => Some(class_key.into()),
        SymbolKey::XmlRecord(xml_rec_key) => Some(xml_rec_key.into()),
        _ => None,
    }
}

/// Whether `param` takes a domain, e.g. `domain` of `search`.
fn is_domain_param(session: &SessionInfo, param: VariableKey) -> bool {
    session.st()[param].evaluations.iter()
        .any(|evaluation| matches!(evaluation.symbol.get_symbol_ptr(), EvaluationSymbolPtr::DOMAIN))
}

/// The argument holding the string: its index, keyword arguments coming after the positional
/// ones, its value, and its name if it is a keyword argument
fn find_argument<'a>(arguments: &'a Arguments, string: &ExprStringLiteral) -> Option<(usize, &'a Expr, Option<&'a str>)> {
    if let Some((index, arg)) = arguments.args.iter()
        .find_position(|arg| arg.range().contains_range(string.range)) {
        Some((index, arg, None))
    } else if let Some((index, keyword)) = arguments.keywords.iter()
        .find_position(|keyword| keyword.range().contains_range(string.range))
        // `**kwargs` has no name
        && let Some(name) = keyword.arg.as_ref() {
        Some((arguments.args.len() + index, &keyword.value, Some(name.as_str())))
    } else {
        None
    }
}

pub(super) fn find_argument_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    let Some((arg_index, value, keyword_name)) = find_argument(&call_expr.arguments, string) else {
        return vec![];
    };

    let keyword_field_param = keyword_name.and_then(|name| FieldParam::of(session, name, value, string));

    // If the keyword argument is not "domain" and cannot be classified as a field parameter, skip it
    if keyword_name.is_some_and(|name| name != "domain") && keyword_field_param.is_none() {
        return vec![];
    }
    let mut arg_symbols = vec![];
    for (callable, callable_sym) in find_callables(session, scope, call_expr) {
        if let SymbolKey::Function(function_key) = callable_sym
            && session.st()[function_key].is_property {
            continue;
        }
        if let Some(decorator) = api_decorator(session, callable_sym) {
            arg_symbols.extend(
                find_decorator_arg_symbols(session, scope, from_module, string, decorator, pick)
            );
            continue;
        }
        // Call parameter symbol corresponding to the argument
        let param = find_call_param(session, from_module, &callable, callable_sym, call_expr, arg_index);

        // If the parameter is a domain parameter, find its symbols using the domain-specific logic
        if param.is_some_and(|param| is_domain_param(session, param)) {
            if let Some(model) = model_called_on(session, &callable) {
                arg_symbols.extend(
                    field_paths::find_domain_param_symbols(session, model, string, from_module, pick)
                );
            }
            continue;
        }

        if !SymbolTable::is_field_class(session, callable_sym) {
            continue;
        }
        let field_param = match keyword_name {
            Some(_) => keyword_field_param,
            None => param.and_then(|param| FieldParam::of(session, session.st()[param].name.as_str(), value, string)),
        };
        if let Some(field_param) = field_param {
            arg_symbols.extend(
                find_field_param_symbols(session, scope, from_module, string, call_expr, field_param, pick)
            );
        }
    }

    arg_symbols.into_iter().unique().collect()
}
