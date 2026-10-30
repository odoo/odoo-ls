use itertools::Itertools;
use ruff_python_ast::{Arguments, Expr, ExprCall, ExprStringLiteral};
use ruff_text_size::Ranged;
use crate::core::symbols::symbol_keys::{ModelSymbolKey, ModuleKey, SymbolKey, VariableKey};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::FunctionSymbol;
use crate::core::symbols::function_symbol::ParamRole;

use crate::constants::SymType;
use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak};
use crate::core::evaluation_context::ContextKey;
use crate::threads::SessionInfo;

use super::api_decorators::{api_decorator, find_decorator_arg_symbols};
use super::field_params::{resolve_field_param, FieldParam};
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

/// The parameter an argument is given to, see `find_call_param`
#[derive(Clone, Copy)]
pub struct CallParam {
    pub symbol: VariableKey,
    pub role: Option<ParamRole>,
}

/// The parameter of the callable that the argument at `arg_index` of `call_expr` is given to, the
/// keyword arguments being indexed after the positional ones.
pub fn find_call_param(
    session: &mut SessionInfo,
    from_module: Option<ModuleKey>,
    callable: &EvaluationSymbolWeak,
    callable_sym: SymbolKey,
    call_expr: &ExprCall,
    arg_index: usize,
) -> Option<CallParam> {
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
        .and_then(|func_arg| Some(CallParam { symbol: func_arg.symbol.upgrade(session.st())?, role: func_arg.role }))
}

/// The model the callable is called on, e.g. `res.partner` in `self.env['res.partner'].search(...)`.
pub fn model_called_on(session: &SessionInfo, callable: &EvaluationSymbolWeak) -> Option<ModelSymbolKey> {
    callable.context.get(ContextKey::BaseAttr)?.as_symbol().upgrade(session.st())?.as_model_symbol_key()
}

/// What the value given to a parameter names, for the parameters naming something of a model.
#[derive(Clone, Copy)]
pub enum ParamKind {
    /// A domain on the model the method is called on, e.g. `search(domain=[('name', '=', 'x')])`.
    /// Without the model, only its structure is known.
    Domain(Option<ModelSymbolKey>),
    /// A parameter of a field class, e.g. `fields.Char(related='partner_id.name')`
    Field(FieldParam),
}

impl ParamKind {
    /// The kind of the parameter `param` of `callable_sym`, when called on `model`. A keyword given
    /// to no declared parameter (`keyword_name`) is known by its name. The value is not looked at,
    /// see `is_in_value`.
    pub fn of(
        session: &SessionInfo,
        callable_sym: SymbolKey,
        param: Option<CallParam>,
        keyword_name: Option<&str>,
        model: Option<ModelSymbolKey>,
    ) -> Option<Self> {
        if let Some(param) = param {
            // A declared parameter: only its role tells
            return match param.role? {
                ParamRole::Domain => Some(ParamKind::Domain(model)),
                ParamRole::Comodel => Some(ParamKind::Field(FieldParam::Comodel)),
                ParamRole::InverseField => Some(ParamKind::Field(FieldParam::InverseField)),
                ParamRole::Selection => Some(ParamKind::Field(FieldParam::ModelMethod)),
            };
        }
        // Field classes take most of their parameters in `**kwargs`: known by their name only
        let name = keyword_name?;
        if !SymbolTable::is_field_class(session, callable_sym) {
            return None;
        }
        FieldParam::of_kwarg_name(session.sync_odoo.version, name).map(ParamKind::Field)
    }

    /// Whether `string` is where the `value` given to a parameter of this kind names something.
    fn is_in_value(self, value: &Expr, string: &ExprStringLiteral) -> bool {
        match self {
            // Any string of it, e.g. the field path of a leaf
            ParamKind::Domain(_) => true,
            ParamKind::Field(field_param) => field_param.is_in_value(value, string),
        }
    }

    /// Whether the keyword argument `name` can be of a kind, known before evaluating the callee: the
    /// marked parameters have a role by their name, see `ParamRole`, the others of field classes are
    /// known by their name.
    fn can_be_keyword(session: &SessionInfo, name: &str, value: &Expr, string: &ExprStringLiteral) -> bool {
        ParamRole::of_param_name(name).is_some()
            || FieldParam::of_kwarg_name(session.sync_odoo.version, name).is_some_and(|field_param| field_param.is_in_value(value, string))
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

/// What `string`, in an argument of `call_expr`, resolves to, over all the callables of the call.
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
    // Skip a keyword argument that cannot be of a kind, without evaluating the callee
    if keyword_name.is_some_and(|name| !ParamKind::can_be_keyword(session, name, value, string)) {
        return None;
    }
    // e.g. an override and its base method
    find_callables(session, scope, call_expr).into_iter()
        .filter_map(|(callable, callable_sym)| {
            resolve_for_callable(session, scope, from_module, string, call_expr, argument, &callable, callable_sym, pick)
        })
        .reduce(StringResolution::merge)
}

/// What `string`, in the `argument` of `call_expr` holding it, resolves to for one of its callables.
fn resolve_for_callable(
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
        && session.st()[function_key].is_property {
        return None;
    }
    if let Some(decorator) = api_decorator(session, callable_sym) {
        return StringResolution::from_members(find_decorator_arg_symbols(session, scope, from_module, string, decorator, pick));
    }
    // Call parameter symbol corresponding to the argument
    let param = find_call_param(session, from_module, callable, callable_sym, call_expr, arg_index);
    let model = model_called_on(session, callable);
    match ParamKind::of(session, callable_sym, param, keyword_name, model).filter(|kind| kind.is_in_value(value, string)) {
        Some(ParamKind::Domain(Some(model))) => {
            StringResolution::from_members(field_paths::find_nested_fields(session, model, from_module, string, pick))
        },
        Some(ParamKind::Field(field_param)) => {
            resolve_field_param(session, scope, from_module, string, call_expr, field_param, pick)
        },
        Some(ParamKind::Domain(None)) | None => None,
    }
}
