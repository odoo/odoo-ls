use crate::core::symbols::symbol_keys::{ModuleKey, SymbolKey};
use crate::odoo_version::OdooVersion;
use ruff_python_ast::{Expr, ExprCall, ExprStringLiteral};
use ruff_text_size::Ranged;

use crate::threads::SessionInfo;

use super::field_paths;
use super::model_members;
use super::string_resolution::{SegmentPick, StringResolution, find_model_symbols};

/// Special type of parameter of a model field, indicating what kind of value it expects in odoo context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldParam {
    /// The model of a relational field, e.g. `comodel_name='res.partner'`
    Comodel,
    /// The field path the field is related to, e.g. `related='partner_id.name'`
    Related,
    /// A field path the field depends on, e.g. `depends=['partner_id.name']`
    Depends,
    /// The inverse field of a One2many on its comodel, e.g. `inverse_name='partner_id'`
    InverseField,
    /// A method of the model, e.g. `compute='_compute_name'`
    ModelMethod,
}

impl FieldParam {
    /// Classifies the field parameter from the given keyword argument name, and the odoo version
    pub(super) fn from_kwarg_name(version: OdooVersion, name: &str) -> Option<Self> {
        match name {
            "related" => Some(FieldParam::Related),
            "depends" => Some(FieldParam::Depends),
            "compute" | "group_expand" | "inverse" | "search" => Some(FieldParam::ModelMethod),
            "compute_sql" if version >= (19, 1) => Some(FieldParam::ModelMethod),
            "init_storage" if version >= (20, 0) => Some(FieldParam::ModelMethod),
            _ => None,
        }
    }

    /// Whether `string` is in the correct expected position in the `value` expression.
    /// e.g. for `depends=['partner_id']`, the string `'partner_id'` is in the correct position.
    pub(super) fn is_in_value(self, value: &Expr, string: &ExprStringLiteral) -> bool {
        match self {
            FieldParam::Depends => match value {
                Expr::List(list) => list.elts.iter().any(|item| item.range() == string.range),
                Expr::Tuple(tuple) => tuple.elts.iter().any(|item| item.range() == string.range),
                _ => false,
            },
            // Otherwise the value is the string itself,
            // so the items of e.g. `selection=[('key', 'Label')]` are not matched
            _ => {
                matches!(value, Expr::StringLiteral(value_string) if value_string.range == string.range)
            }
        }
    }
}

/// The comodel given to a relational field class.
/// It is its first argument, or its `comodel_name` keyword argument.
/// e.g. `res.partner` in `fields.One2many('res.partner', 'parent_id')`
pub fn field_comodel_name(call_expr: &ExprCall) -> Option<&str> {
    match call_expr.arguments.args.first() {
        Some(Expr::StringLiteral(expr)) => Some(expr.value.to_str()),
        _ => call_expr
            .arguments
            .keywords
            .iter()
            .find(|kw| kw.arg.as_ref().is_some_and(|arg| arg.id == "comodel_name"))
            .and_then(|kw| match &kw.value {
                Expr::StringLiteral(expr) => Some(expr.value.to_str()),
                _ => None,
            }),
    }
}

fn find_inverse_name_field_symbol(
    session: &mut SessionInfo,
    from_module: Option<ModuleKey>,
    field_value: &str,
    call_expr: &ExprCall,
) -> Vec<SymbolKey> {
    let Some(model_name) = field_comodel_name(call_expr) else {
        return vec![];
    };
    let Some(model) = session.model_mgr().get_model_key(model_name) else {
        return vec![];
    };
    let main_syms = session.model_mgr()[model]
        .get_main_symbols(session, from_module)
        .collect::<Vec<_>>();
    main_syms
        .iter()
        .flat_map(|&main_sym| {
            model_members::find_fields(session, main_sym, from_module, field_value)
        })
        .collect()
}

/// Resolve `string` given to the field class parameter `field_param`.
pub(super) fn resolve_field_param(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    field_param: FieldParam,
    pick: SegmentPick,
) -> Option<StringResolution> {
    let member_results = match field_param {
        FieldParam::Comodel => {
            let models = find_model_symbols(session, string.value.to_str(), from_module);
            // Models, not members, returned directly.
            return (!models.is_empty()).then_some(StringResolution::Model(models));
        }
        FieldParam::Related | FieldParam::Depends => {
            let model = model_members::enclosing_model(session, scope)?;
            field_paths::find_nested_fields(session, model, from_module, string, pick)
        }
        FieldParam::InverseField => {
            find_inverse_name_field_symbol(session, from_module, string.value.to_str(), call_expr)
                .into_iter()
                .map(|symbol| (symbol, string.range))
                .collect()
        }
        FieldParam::ModelMethod => {
            let model = model_members::enclosing_model(session, scope)?;
            model_members::find_methods(session, model, from_module, string.value.to_str())
                .into_iter()
                .map(|symbol| (symbol, string.range))
                .collect()
        }
    };
    StringResolution::from_members(member_results)
}
