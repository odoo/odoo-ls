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

use super::api_decorators::{api_decorator, find_decorator_arg_symbols};
use super::field_params::{find_field_param_resolution, FieldParam};
use super::field_paths;
use super::string_resolution::{SegmentPick, StringResolution};

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

/// What a string given to a parameter names, for the parameters naming something of a model.
enum ParamKind {
    /// A domain on the model the method is called on, e.g. `search(domain=[('name', '=', 'x')])`
    Domain,
    /// A parameter of a field class, e.g. `fields.Char(related='partner_id.name')`
    Field(FieldParam),
}

impl ParamKind {
    /// What `string`, in the `value` of an argument given to `callable_sym`, names: from its declared
    /// `param`, or its `keyword_name`.
    fn of(
        session: &SessionInfo,
        callable_sym: SymbolKey,
        param: Option<VariableKey>,
        keyword_name: Option<&str>,
        value: &Expr,
        string: &ExprStringLiteral,
    ) -> Option<Self> {
        if param.is_some_and(|param| is_domain_param(session, param)) {
            return Some(ParamKind::Domain);
        }
        if !SymbolTable::is_field_class(session, callable_sym) {
            return None;
        }
        // Field classes do not declare most of their parameters: a keyword is known by its name
        let name = match keyword_name {
            Some(name) => name,
            None => session.st()[param?].name.as_str(),
        };
        FieldParam::of(session, callable_sym, name, value, string).map(ParamKind::Field)
    }

    /// Whether the keyword argument `name` can be of a kind, known before evaluating the callee: only
    /// the `domain` parameter of `search` is marked as a domain, see its hook, and whether the field
    /// class takes the parameter is checked once known.
    fn can_be_keyword(session: &SessionInfo, name: &str, value: &Expr, string: &ExprStringLiteral) -> bool {
        name == "domain"
            || FieldParam::of_name(session, name).is_some_and(|field_param| field_param.is_in_value(value, string))
    }
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
) -> Option<StringResolution> {
    let (arg_index, value, keyword_name) = find_argument(&call_expr.arguments, string)?;
    // Skip a keyword argument that cannot be of a kind, without evaluating the callee
    if keyword_name.is_some_and(|name| !ParamKind::can_be_keyword(session, name, value, string)) {
        return None;
    }
    let mut resolution: Option<StringResolution> = None;
    for (callable, callable_sym) in find_callables(session, scope, call_expr) {
        if let SymbolKey::Function(function_key) = callable_sym
            && session.st()[function_key].is_property {
            continue;
        }
        let callable_resolution = if let Some(decorator) = api_decorator(session, callable_sym) {
            StringResolution::from_members(find_decorator_arg_symbols(session, scope, from_module, string, decorator, pick))
        } else {
            // Call parameter symbol corresponding to the argument
            let param = find_call_param(session, from_module, &callable, callable_sym, call_expr, arg_index);
            match ParamKind::of(session, callable_sym, param, keyword_name, value, string) {
                Some(ParamKind::Domain) => model_called_on(session, &callable).and_then(|model|
                    StringResolution::from_members(field_paths::find_domain_param_symbols(session, model, string, from_module, pick))
                ),
                Some(ParamKind::Field(field_param)) => {
                    find_field_param_resolution(session, scope, from_module, string, call_expr, field_param, pick)
                },
                None => None,
            }
        };
        if let Some(callable_resolution) = callable_resolution {
            resolution = Some(match resolution {
                Some(resolution) => resolution.merge(callable_resolution),
                None => callable_resolution,
            });
        }
    }
    resolution
}
