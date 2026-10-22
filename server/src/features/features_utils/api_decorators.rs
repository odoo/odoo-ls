use ruff_python_ast::ExprStringLiteral;
use ruff_text_size::TextRange;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol_keys::{ModuleKey, SymbolKey};
use crate::tree::OYarnExt;

use crate::threads::SessionInfo;

use super::field_paths;
use super::string_resolution::SegmentPick;

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

/// Symbols of `string` given to `decorator`, e.g. `@api.depends('partner_id.name')`.
pub(super) fn find_decorator_arg_symbols(
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
