use ruff_python_ast::{Expr, ExprCall, ExprStringLiteral};
use ruff_text_size::{Ranged, TextRange};
use crate::core::symbols::symbol_keys::{ModuleKey, SymbolKey};

use crate::threads::SessionInfo;

use super::field_paths;
use super::string_resolution::SegmentPick;

/// What a string given to a parameter of a field class names.
#[derive(Clone, Copy)]
pub(super) enum FieldParam {
    /// A field path on the model, e.g. `related='partner_id.name'`
    FieldPath,
    /// A field of the comodel, e.g. `inverse_name='partner_id'`
    ComodelField,
    /// A method of the model, e.g. `compute='_compute_name'`
    ModelMethod,
}

impl FieldParam {
    /// What `string`, in the `value` of the field class parameter `name`, names.
    pub(super) fn of(session: &SessionInfo, name: &str, value: &Expr, string: &ExprStringLiteral) -> Option<Self> {
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

pub(super) fn find_field_param_symbols(
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
