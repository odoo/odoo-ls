use ruff_python_ast::ExprStringLiteral;
use ruff_text_size::{TextRange, TextSize};
use crate::core::evaluation_utils::DeepFieldEvalWalker;
use crate::core::symbols::SymbolTable;
use crate::core::symbols::symbol_keys::{ClassKey, ModelSymbolKey, ModuleKey, SymbolKey};
use crate::features::ast_utils::AstUtils;

use crate::constants::SymType;
use crate::threads::SessionInfo;

use super::string_resolution::SegmentPick;

fn find_nested_fields(
    session: &mut SessionInfo,
    base_symbol: ModelSymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    if let SymbolKey::Class(class) = base_symbol.into()
    && session.st()[class]._model.is_none() {
        return vec![];
    }
    // Fields of the path, as (start, end) indexes in the string
    let field_name = string.value.to_str();
    let mut segments = vec![];
    let mut segment_start = 0;
    for name in field_name.split(".") {
        segments.push((segment_start, segment_start + name.len()));
        segment_start += name.len() + 1;
    }
    let cursor_index = match pick {
        SegmentPick::All => None,
        SegmentPick::Cursor(offset) => {
            let Some(cursor) = AstUtils::index_in_string(string, TextSize::new(offset as u32)) else {
                return vec![]; // Offset is in between implicitly concatenated string parts, or string contains escape sequences
            };
            // The cursor belongs to a field up to the dot after it
            let Some(idx) = segments.iter().position(|&(start, end)| start <= cursor && cursor <= end) else {
                return vec![];
            };
            Some(idx)
        }
    };
    let mut results = vec![];
    let mut deep_field_walker = DeepFieldEvalWalker::new(base_symbol.into(), from_module);
    for (idx, field_sub_name) in field_name.split(".").map(|x| x.to_string()).enumerate() {
        let Some(base_symbol) = deep_field_walker.get_model_symbol(session) else {
            break;
        };
        let field_symbols = deep_field_walker.get_model_fields(session, base_symbol, &field_sub_name);
        let (start, end) = segments[idx];
        let segment_range = AstUtils::range_in_file(string, start, end);
        match pick {
            SegmentPick::All => {
                // Not found in the file (escapes, split between parts): do nothing
                if let Some(segment_range) = segment_range {
                    results.extend(field_symbols.into_iter().map(|f| (f, segment_range)));
                }
            },
            SegmentPick::Cursor(_) if cursor_index == Some(idx) => {
                let segment_range = segment_range.unwrap_or(string.range);
                return field_symbols.into_iter().map(|f| (f, segment_range)).collect();
            },
            SegmentPick::Cursor(_) => {},
        }
    }
    results
}

/// The model class `scope` is written in, if that class is a model.
pub(super) fn enclosing_model_class(session: &SessionInfo, scope: SymbolKey) -> Option<ClassKey> {
    let Some(SymbolKey::Class(class)) = session.st().get_in_parents(scope, &[SymType::CLASS], true) else {
        return None;
    };
    session.st()[class]._model.is_some().then_some(class)
}

pub(super) fn find_nested_fields_in_class(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    pick: SegmentPick,
) ->  Vec<(SymbolKey, TextRange)>{
    let Some(parent_class) = enclosing_model_class(session, scope) else {
        return vec![];
    };
    find_nested_fields(session, parent_class.into(), from_module, string, pick)
}

pub(super) fn find_field_in_enclosing_model(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    field_name: &str,
) -> Vec<SymbolKey> {
    let Some(parent_class) = enclosing_model_class(session, scope) else {
        return vec![];
    };
    find_simple_field_in_class(session, parent_class.into(), from_module, field_name)
}

pub(super) fn find_method_in_enclosing_model(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    method_name: &str,
) -> Vec<SymbolKey> {
    let Some(parent_class) = enclosing_model_class(session, scope) else {
        return vec![];
    };
    find_simple_method_in_class(session, parent_class.into(), from_module, method_name)
}

/// Fields of `string` in a domain on `model`.
pub(super) fn find_domain_param_symbols(
    session: &mut SessionInfo,
    model: ModelSymbolKey,
    string: &ExprStringLiteral,
    from_module: Option<ModuleKey>,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    find_nested_fields(session, model, from_module, string, pick)
}

pub(super) fn find_simple_field_in_class(
    session: &mut SessionInfo,
    parent_class: ModelSymbolKey,
    from_module: Option<ModuleKey>,
    field_name: &str,
) -> Vec<SymbolKey> {
    SymbolTable::get_member_symbol(session, parent_class.into(), field_name, from_module, false, true, false, true, false).0
}

fn find_simple_method_in_class(
    session: &mut SessionInfo,
    parent_class: ModelSymbolKey,
    from_module: Option<ModuleKey>,
    field_name: &str,
) -> Vec<SymbolKey> {
    SymbolTable::get_member_symbol(session, parent_class.into(), field_name, from_module, false, false, true, true, false).0
}
