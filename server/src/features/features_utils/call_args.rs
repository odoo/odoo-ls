use itertools::Itertools;
use ruff_python_ast::{Arguments, Expr, ExprCall, ExprStringLiteral};
use ruff_text_size::{Ranged, TextRange};
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol_keys::{ModuleKey, SymbolKey, VariableKey};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::FunctionSymbol;
use crate::tree::OYarnExt;

use crate::constants::SymType;
use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak};
use crate::core::evaluation_context::ContextKey;
use crate::threads::SessionInfo;

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

/// An `api` decorator whose arguments name fields of the model.
pub enum ApiDecorator {
    Onchange,
    Constrains,
    Depends,
}

/// The decorator `symbol` is, if it is `api.onchange`, `api.constrains` or `api.depends` of Odoo.
pub fn api_decorator(session: &mut SessionInfo, symbol: SymbolKey) -> Option<ApiDecorator> {
    let SymbolKey::Function(_) = symbol else {
        return None;
    };
    let tree = session.st().get_tree(symbol);
    let is_18_1_or_later = session.sync_odoo.version >= (18, 1);
    let is_api = (!is_18_1_or_later && tree.0.ends_with_strs(&["odoo", "api"]))
        || (is_18_1_or_later && tree.0.ends_with_strs(&["odoo", "orm", "decorators"]));
    if !is_api || !SyncOdoo::is_in_main_entry(session, &tree.0) {
        return None;
    }
    let [name] = tree.1.as_slice() else {
        return None;
    };
    match name.as_str() {
        "onchange" => Some(ApiDecorator::Onchange),
        "constrains" => Some(ApiDecorator::Constrains),
        "depends" => Some(ApiDecorator::Depends),
        _ => None,
    }
}

/// What a string given to a parameter of a field class names.
#[derive(Clone, Copy)]
enum FieldParam {
    /// A field path on the model, e.g. `related='partner_id.name'`
    FieldPath,
    /// A field of the comodel, e.g. `inverse_name='partner_id'`
    ComodelField,
    /// A method of the model, e.g. `compute='_compute_name'`
    ModelMethod,
}

impl FieldParam {
    /// What `string`, in the `value` of the field class parameter `name`, names.
    fn of(session: &SessionInfo, name: &str, value: &Expr, string: &ExprStringLiteral) -> Option<Self> {
        if name == "depends" {
            // The field paths are the items of the value, e.g. `depends=['partner_id']`
            let items = match value {
                Expr::List(list) => &list.elts,
                Expr::Tuple(tuple) => &tuple.elts,
                _ => return None,
            };
            return items.iter().any(|item| item.range() == string.range).then_some(FieldParam::FieldPath);
        }
        // Otherwise the value is the string itself:
        // in order to not trigger on an item of `selection=[('key', 'Label')]`
        if !matches!(value, Expr::StringLiteral(value_string) if value_string.range == string.range) {
            return None;
        }
        match name {
            "related" => Some(FieldParam::FieldPath),
            "inverse_name" => Some(FieldParam::ComodelField),
            "compute" | "group_expand" | "inverse" | "search" | "selection" => Some(FieldParam::ModelMethod),
            "compute_sql" if session.sync_odoo.version >= (19, 1) => Some(FieldParam::ModelMethod),
            "init_storage" if session.sync_odoo.version >= (20, 0) => Some(FieldParam::ModelMethod),
            _ => None,
        }
    }
}

fn find_inverse_name_field_symbol(
    session: &mut SessionInfo,
    from_module: Option<ModuleKey>,
    field_value: &str,
    call_expr: &ExprCall,
) -> Vec<SymbolKey> {
    let model_name = if let Some(Expr::StringLiteral(expr)) = call_expr.arguments.args.first() {
        expr.value.to_str()
    } else {
        let Some(model_name) = call_expr.arguments.keywords.iter().find(|kw| kw.arg.as_ref().map(|arg| arg.id == "comodel_name").unwrap_or(false))
            .and_then(|kw| match &kw.value {
                Expr::StringLiteral(expr) => Some(expr.value.to_str()),
                _ => None
            }) else {
                return vec![];
            };
        model_name
    };
    let Some(model) = session.model_mgr().get_model_key(model_name) else {
        return vec![];
    };
    let main_syms = session.model_mgr()[model].get_main_symbols(session, from_module).collect::<Vec<_>>();
    main_syms.iter().flat_map(|&main_sym| field_paths::find_simple_field_in_class(session, main_sym, from_module, field_value)).collect()
}

/// Symbols of `string` given to `decorator`, e.g. `@api.depends('partner_id.name')`.
fn find_decorator_arg_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    decorator: ApiDecorator,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    match decorator {
        ApiDecorator::Onchange | ApiDecorator::Constrains => {
            field_paths::find_field_in_enclosing_model(session, scope, from_module, string.value.to_str())
                .into_iter()
                .map(|symbol| (symbol, string.range))
                .collect()
        },
        ApiDecorator::Depends => field_paths::find_nested_fields_in_class(session, scope, from_module, string, pick),
    }
}

fn find_field_param_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    field_param: FieldParam,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    let symbols = match field_param {
        FieldParam::FieldPath => {
            return field_paths::find_nested_fields_in_class(session, scope, from_module, string, pick);
        },
        FieldParam::ComodelField => find_inverse_name_field_symbol(session, from_module, string.value.to_str(), call_expr),
        FieldParam::ModelMethod => field_paths::find_method_in_enclosing_model(session, scope, from_module, string.value.to_str()),
    };
    symbols.into_iter().map(|symbol| (symbol, string.range)).collect()
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

/// Whether `param` takes a domain, e.g. `domain` of `search`.
fn is_domain_param(session: &SessionInfo, param: VariableKey) -> bool {
    session.st()[param].evaluations.iter()
        .any(|evaluation| matches!(evaluation.symbol.get_symbol_ptr(), EvaluationSymbolPtr::DOMAIN))
}


/// The argument holding the string: its index, keyword arguments coming after the positional
// ones, its value, and its name if it is a keyword argument
fn find_argument<'a>(arguments: &'a Arguments, string: &ExprStringLiteral) -> Option<(usize, &'a Expr, Option<&'a str>)>{
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
            arg_symbols.extend(
                field_paths::find_domain_param_symbols(session, &callable, string, from_module, pick)
            );
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
