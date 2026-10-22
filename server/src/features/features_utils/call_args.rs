use itertools::Itertools;
use ruff_python_ast::{Expr, ExprCall, ExprStringLiteral, Identifier, Keyword};
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

/// Whether `call_expr` calls a field class, e.g. `fields.Char(...)`.
fn is_field_call(session: &mut SessionInfo, scope: SymbolKey, call_expr: &ExprCall) -> bool {
    find_callables(session, scope, call_expr)
        .into_iter()
        .any(|(_, callable_sym)| SymbolTable::is_field_class(session, callable_sym))
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

fn find_model_method_symbol(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    method_name: &str,
) -> Vec<SymbolKey> {
    let Some(parent_class) = field_paths::enclosing_model_class(session, scope) else {
        return vec![];
    };
    field_paths::find_simple_method_in_class(session, parent_class.into(), from_module, method_name)
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

fn find_simple_decorator_field_symbol(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    field_name: &str,
) -> Vec<SymbolKey> {
    let Some(parent_class) = field_paths::enclosing_model_class(session, scope) else {
        return vec![];
    };
    field_paths::find_simple_field_in_class(session, parent_class.into(), from_module, field_name)
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
            find_simple_decorator_field_symbol(session, scope, from_module, string.value.to_str())
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
        FieldParam::ModelMethod => find_model_method_symbol(session, scope, from_module, string.value.to_str()),
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

fn find_positional_argument_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    arg_index: usize,
    arg_expr: &Expr,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    let mut arg_symbols: Vec<(SymbolKey, TextRange)> = vec![];
    for (callable, callable_sym) in find_callables(session, scope, call_expr) {
        if ![SymType::FUNCTION, SymType::CLASS].contains(&callable_sym.typ()) {
            continue;
        }
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
        let Some(param) = find_call_param(session, from_module, &callable, callable_sym, call_expr, arg_index) else {
            continue;
        };
        if is_domain_param(session, param) {
            arg_symbols.extend(
                field_paths::find_domain_param_symbols(session, &callable, string, from_module, pick)
            );
            continue;
        }
        if SymbolTable::is_field_class(session, callable_sym)
            && let Some(field_param) = FieldParam::of(session, session.st()[param].name.as_str(), arg_expr, string) {
            arg_symbols.extend(
                find_field_param_symbols(session, scope, from_module, string, call_expr, field_param, pick)
            );
        }
    }
    arg_symbols
}

/// Find the symbols corresponding to a keyword argument in a call, if it is a domain or a field
/// parameter.
fn find_keyword_argument_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    keyword_index: usize,
    keyword: &Keyword,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    // `**kwargs` has no name
    let Some(name) = keyword.arg.as_ref().map(Identifier::as_str) else {
        return vec![];
    };
    // Only the `domain` parameter of `search` takes a domain, see its hook
    if name == "domain" {
        let arg_index = call_expr.arguments.args.len() + keyword_index;
        let mut arg_symbols = vec![];
        for (callable, callable_sym) in find_callables(session, scope, call_expr) {
            if let Some(param) = find_call_param(session, from_module, &callable, callable_sym, call_expr, arg_index)
                && is_domain_param(session, param) {
                arg_symbols.extend(
                    field_paths::find_domain_param_symbols(session, &callable, string, from_module, pick)
                );
            }
        }
        return arg_symbols;
    }
    // Classify the parameter first: only then evaluate the callee
    let Some(field_param) = FieldParam::of(session, name, &keyword.value, string) else {
        return vec![];
    };
    if !is_field_call(session, scope, call_expr) {
        return vec![];
    }
    find_field_param_symbols(session, scope, from_module, string, call_expr, field_param, pick)
}

pub(super) fn find_argument_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    if let Some((arg_index, arg_expr)) = call_expr
        .arguments
        .args
        .iter()
        .find_position(|arg| arg.range().contains_range(string.range))
    {
        find_positional_argument_symbols(
            session,
            scope,
            from_module,
            string,
            call_expr,
            arg_index,
            arg_expr,
            pick,
        )
    } else if let Some((keyword_index, keyword)) = call_expr
        .arguments
        .keywords
        .iter()
        .find_position(|arg| arg.range().contains_range(string.range))
    {
        find_keyword_argument_symbols(
            session,
            scope,
            from_module,
            string,
            call_expr,
            keyword_index,
            keyword,
            pick,
        )
    } else {
        vec![]
    }
}
