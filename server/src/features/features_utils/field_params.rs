use ruff_python_ast::{Expr, ExprCall, ExprStringLiteral};
use ruff_text_size::Ranged;
use crate::core::symbols::symbol_keys::{ModuleKey, SymbolKey};
use crate::odoo_version::OdooVersion;

use crate::threads::SessionInfo;

use super::field_paths;
use super::model_members;
use super::string_resolution::{find_model_symbols, SegmentPick, StringResolution};

/// What a string given to a parameter of a field class names.
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
    /// The kind of `name`, a parameter the field classes take in `**kwargs`. The ones they declare
    /// (`comodel_name`, `inverse_name`, `selection`) are marked instead, see `ParamRole`.
    pub(super) fn of_kwarg_name(version: OdooVersion, name: &str) -> Option<Self> {
        match name {
            "related" => Some(FieldParam::Related),
            "depends" => Some(FieldParam::Depends),
            "compute" | "group_expand" | "inverse" | "search" => Some(FieldParam::ModelMethod),
            "compute_sql" if version >= (19, 1) => Some(FieldParam::ModelMethod),
            "init_storage" if version >= (20, 0) => Some(FieldParam::ModelMethod),
            _ => None,
        }
    }

    /// Whether `string` is where the `value` of this parameter names something.
    pub(super) fn is_in_value(self, value: &Expr, string: &ExprStringLiteral) -> bool {
        match self {
            // The field paths are the items of the value, e.g. `depends=['partner_id']`
            FieldParam::Depends => match value {
                Expr::List(list) => list.elts.iter().any(|item| item.range() == string.range),
                Expr::Tuple(tuple) => tuple.elts.iter().any(|item| item.range() == string.range),
                _ => false,
            },
            // Otherwise the value is the string itself:
            // in order to not trigger on an item of `selection=[('key', 'Label')]`
            _ => matches!(value, Expr::StringLiteral(value_string) if value_string.range == string.range),
        }
    }
}

/// The comodel given to a relational field class, e.g. `res.partner` in
/// `fields.One2many('res.partner', 'parent_id')`: its first argument, or its `comodel_name`.
pub fn field_comodel_name(call_expr: &ExprCall) -> Option<&str> {
    match call_expr.arguments.args.first() {
        Some(Expr::StringLiteral(expr)) => Some(expr.value.to_str()),
        _ => call_expr.arguments.keywords.iter()
            .find(|kw| kw.arg.as_ref().is_some_and(|arg| arg.id == "comodel_name"))
            .and_then(|kw| match &kw.value {
                Expr::StringLiteral(expr) => Some(expr.value.to_str()),
                _ => None
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
    let main_syms = session.model_mgr()[model].get_main_symbols(session, from_module).collect::<Vec<_>>();
    main_syms.iter().flat_map(|&main_sym| model_members::find_fields(session, main_sym, from_module, field_value)).collect()
}

/// What `string`, given to the parameter `field_param` of a field class, resolves to.
pub(super) fn resolve_field_param(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    field_param: FieldParam,
    pick: SegmentPick,
) -> Option<StringResolution> {
    let members = match field_param {
        // A model, not members of one
        FieldParam::Comodel => {
            let models = find_model_symbols(session, string.value.to_str(), from_module);
            return (!models.is_empty()).then_some(StringResolution::Model(models));
        },
        // Fields or methods of the model the field is declared in
        FieldParam::Related | FieldParam::Depends => {
            let model = model_members::enclosing_model(session, scope)?;
            field_paths::find_nested_fields(session, model, from_module, string, pick)
        },
        FieldParam::InverseField => find_inverse_name_field_symbol(session, from_module, string.value.to_str(), call_expr)
            .into_iter().map(|symbol| (symbol, string.range)).collect(),
        FieldParam::ModelMethod => {
            let model = model_members::enclosing_model(session, scope)?;
            model_members::find_methods(session, model, from_module, string.value.to_str())
                .into_iter().map(|symbol| (symbol, string.range)).collect()
        },
    };
    StringResolution::from_members(members)
}
