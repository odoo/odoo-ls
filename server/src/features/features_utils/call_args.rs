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

use super::api_decorators::{classify_api_decorator, find_decorator_arg_symbols};
use super::field_params::{resolve_field_param, FieldParam};
use super::field_paths;
use super::string_resolution::{SegmentPick, StringResolution};

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

/// The parameter an argument is given to, see `find_call_param`.
/// The parameter symbol as an upgraded variable key, and its role if it has one.
///
/// Thin copiable type, that holds what we want from [crate::core::symbols::function_symbol::Argument]
#[derive(Clone, Copy)]
pub struct CallParam {
    pub symbol: VariableKey,
    pub role: Option<ParamRole>,
}

/// Find the Call Parameter for the argument at `arg_index` in `call_expr`.
/// `arg_index` counts the keyword arguments after the positional ones.
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
    FunctionSymbol::get_indexed_arg_in_call(
        session.st(),
        func_key,
        call_expr,
        arg_index as u32,
        is_on_instance,
    )
    .and_then(|func_arg| {
        Some(CallParam {
            symbol: func_arg.symbol.upgrade(session.st())?,
            role: func_arg.role,
        })
    })
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

/// A call argument with a special meaning in odoo context, e.g. a domain or a field parameter.
/// Computed for each call, from the `ParamRole` of its parameter, or from its keyword name for field classes.
/// e.g. a domain on `res.partner` in `self.env['res.partner'].search([...])`
#[derive(Clone, Copy)]
pub enum SpecialArg {
    /// A domain on the model the method is called on, e.g. `search(domain=[('name', '=', 'x')])`.
    /// If the model is unknown, only the domain structure can be validated.
    Domain(Option<ModelSymbolKey>),
    /// A parameter of a field class, e.g. `fields.Char(related='partner_id.name')`
    Field(FieldParam),
}

impl SpecialArg {
    /// Classifies the call argument as a special argument, based on the parameter, keyword name, and model.
    pub fn classify_call_argument(
        session: &SessionInfo,
        callable_sym: SymbolKey,
        param: Option<CallParam>,
        keyword_name: Option<&str>,
        model: Option<ModelSymbolKey>,
    ) -> Option<Self> {
        if let Some(param) = param {
            // The parameter is declared, so use its role
            return match param.role? {
                ParamRole::Domain => Some(SpecialArg::Domain(model)),
                ParamRole::Comodel => Some(SpecialArg::Field(FieldParam::Comodel)),
                ParamRole::InverseField => Some(SpecialArg::Field(FieldParam::InverseField)),
                ParamRole::Selection => Some(SpecialArg::Field(FieldParam::ModelMethod)),
            };
        }
        // Field classes take most parameters through `**kwargs`, so use the keyword name
        let name = keyword_name?;
        if !SymbolTable::is_field_class(session, callable_sym) {
            return None;
        }
        FieldParam::from_kwarg_name(session.sync_odoo.version, name).map(SpecialArg::Field)
    }

    /// Whether `string` is in the correct expected position in the `value` expression
    /// of the current SpecialArg.
    fn is_in_value(self, value: &Expr, string: &ExprStringLiteral) -> bool {
        match self {
            // Any string inside the domain, e.g. the field path of a leaf
            SpecialArg::Domain(_) => true,
            SpecialArg::Field(field_param) => field_param.is_in_value(value, string),
        }
    }

    /// Cheap check if the given `name` can be a keyword argument with a special value in odoo context.
    fn can_be_special_keyword(
        session: &SessionInfo,
        name: &str,
        value: &Expr,
        string: &ExprStringLiteral,
    ) -> bool {
        ParamRole::of_param_name(name).is_some()
            || FieldParam::from_kwarg_name(session.sync_odoo.version, name)
                .is_some_and(|field_param| field_param.is_in_value(value, string))
    }
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
    // Skip a keyword argument that cannot be special, without evaluating the callee
    if keyword_name
        .is_some_and(|name| !SpecialArg::can_be_special_keyword(session, name, value, string))
    {
        return None;
    }
    // A call can have several callables, e.g. an override and its base method
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
    let model = model_called_on(session, callable);
    match SpecialArg::classify_call_argument(session, callable_sym, param, keyword_name, model).filter(|special_arg| special_arg.is_in_value(value, string)) {
        Some(SpecialArg::Domain(Some(model))) => {
            StringResolution::from_members(field_paths::find_nested_fields(session, model, from_module, string, pick))
        },
        Some(SpecialArg::Field(field_param)) => {
            resolve_field_param(session, scope, from_module, string, call_expr, field_param, pick)
        },
        Some(SpecialArg::Domain(None)) | None => None,
    }
}
