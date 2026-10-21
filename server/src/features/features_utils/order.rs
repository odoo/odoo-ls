use ruff_python_ast::ExprStringLiteral;
use ruff_text_size::{TextRange, TextSize};
use crate::core::evaluation_utils::DeepFieldEvalWalker;
use crate::core::symbols::symbol_keys::{ClassKey, ModuleKey, SymbolKey};
use crate::features::ast_utils::AstUtils;
use crate::threads::SessionInfo;

use super::order_item;
use super::string_resolution::SegmentPick;

/// The model whose `_order` is written in `scope`: only in the body of a model class, not in
/// one of its methods, nor in a class that is not a model.
pub fn order_model_class(session: &SessionInfo, scope: SymbolKey) -> Option<ClassKey> {
    let SymbolKey::Class(class) = scope else {
        return None;
    };
    session.st()[class]._model.is_some().then_some(class)
}

/// Fields of `_order = "name desc, partner_id.id"`, with their range in the file. The property
/// of a field (`.id`) is a field of its comodel.
pub(super) fn find_order_fields(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    let Some(parent_class) = order_model_class(session, scope) else {
        return vec![];
    };
    let cursor = match pick {
        SegmentPick::Cursor(offset) => {
            let Some(cursor) = AstUtils::index_in_string(string, TextSize::new(offset as u32)) else {
                return vec![];
            };
            Some(cursor)
        },
        SegmentPick::All => None,
    };
    let mut members = vec![];
    for item in order_item::OrderItem::order_items(string.value.to_str()) {
        // Only the item under the cursor is resolved
        if cursor.is_some_and(|cursor| !item.contains(cursor)) {
            continue;
        }
        // The field on the model, then its property on the comodel
        let mut walker = DeepFieldEvalWalker::new(parent_class.into(), from_module);
        for (name, start, end) in [item.field(), item.property()].into_iter().flatten() {
            let Some(model) = walker.get_model_symbol(session) else {
                break;
            };
            let symbols = walker.get_model_fields(session, model, name);
            if cursor.is_some_and(|cursor| cursor < start || end < cursor) {
                continue;
            }
            let range = match (AstUtils::range_in_file(string, start, end), pick) {
                (Some(range), _) => range,
                // Not found in the file (escapes, split between parts): no token, but hover and definition still work
                (None, SegmentPick::Cursor(_)) => string.range,
                (None, SegmentPick::All) => continue,
            };
            members.extend(symbols.into_iter().map(|symbol| (symbol, range)));
        }
    }
    members
}
