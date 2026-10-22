use itertools::Itertools;
use ruff_python_ast::{Expr, ExprCall, ExprStringLiteral, Keyword};
use ruff_text_size::{Ranged, TextRange};
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol_keys::{ModuleKey, SymbolKey};
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
        if let SymbolKey::Function(function_key) = callable_sym {
            if session.st()[function_key].is_property {
                continue;
            }
            // Check if we are in api.onchange/constrains/depends
            let func_sym_tree = session.st().get_tree(callable_sym);
            let is_18_1_or_later = session.sync_odoo.version >= (18, 1);
            let is_api_decorator = ((!is_18_1_or_later && func_sym_tree.0.ends_with_strs(&["odoo", "api"]))
                || (is_18_1_or_later && func_sym_tree.0.ends_with_strs(&["odoo", "orm", "decorators"])))
                && SyncOdoo::is_in_main_entry(session, &func_sym_tree.0);
            if is_api_decorator && let [decorator] = func_sym_tree.1.as_slice() {
                match decorator.as_str() {
                    "onchange" | "constrains" => {
                        arg_symbols.extend(
                            find_simple_decorator_field_symbol(session, scope, from_module, string.value.to_str())
                            .into_iter().map(|symbol| (symbol, string.range))
                        );
                        continue;
                    },
                    "depends" => {
                        arg_symbols.extend(
                            field_paths::find_nested_fields_in_class(session, scope, from_module, string, pick)
                        );
                        continue;
                    },
                    _ => {},
                }
            }
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
            // skip self on __init__
            Some(true)
        } else {
            callable.context.get(ContextKey::IsAttrOfInstance).map(|v| v.as_bool())
        };

        let func_arg = FunctionSymbol::get_indexed_arg_in_call(
            session.st(),
            func_key,
            call_expr,
            arg_index as u32,
            is_on_instance);
        let Some(func_arg_sym) = func_arg.and_then(|func_arg| func_arg.symbol.upgrade(session.st())) else {
            continue
        };

        if SymbolTable::is_field_class(session, callable_sym) {
            if let Some(field_param) = FieldParam::of(session, session.st()[func_arg_sym].name.as_str(), arg_expr, string) {
                arg_symbols.extend(
                    find_field_param_symbols(session, scope, from_module, string, call_expr, field_param, pick)
                );
            }
            continue;
        }
        for evaluation in session.st()[func_arg_sym].evaluations.clone() {
            if matches!(evaluation.symbol.get_symbol_ptr(), EvaluationSymbolPtr::DOMAIN) {
                arg_symbols.extend(
                    field_paths::find_domain_param_symbols(session, &callable, string, from_module, pick)
                );
            }
        }

    }
    arg_symbols
}

/// Find the symbols corresponding to a keyword argument in a call, if it represents a field parameter.
/// The call now is always field class call.
fn find_keyword_argument_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    keyword: &Keyword,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    // Classify the parameter first: only then evaluate the callee
    let Some(field_param) = keyword.arg.as_ref()
        .and_then(|name| FieldParam::of(session, name.as_str(), &keyword.value, string)) else {
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
    } else if let Some(keyword) = call_expr
        .arguments
        .keywords
        .iter()
        .find(|arg| arg.range().contains_range(string.range))
    {
        find_keyword_argument_symbols(
            session,
            scope,
            from_module,
            string,
            call_expr,
            keyword,
            pick,
        )
    } else {
        vec![]
    }
}
