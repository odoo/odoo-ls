use itertools::Itertools;
use ruff_python_ast::ExprStringLiteral;
use ruff_text_size::TextRange;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol_keys::{ModelSymbolKey, ModuleKey, SourceFileKey, SymbolKey, XmlId};
use crate::features::ast_utils::StringContext;
use crate::threads::SessionInfo;

use super::call_args::find_argument_symbols;
use super::order::{find_order_fields, order_model_class};

/// What a model-related string literal resolves to. Shared by go-to-definition and
/// semantic tokens.
pub enum StringResolution {
    /// Field/method symbols with the sub-range each covers (dotted paths → one per segment)
    Members(Vec<(SymbolKey, TextRange)>),
    Model(Vec<ModelSymbolKey>),
    Module(ModuleKey),
    XmlId(Vec<XmlId>),
}

impl StringResolution {
    /// Members, if there are any
    pub(super) fn from_members(members: Vec<(SymbolKey, TextRange)>) -> Option<Self> {
        (!members.is_empty()).then_some(StringResolution::Members(members))
    }

    /// Add what another callee resolves the same string to. A string names a single kind of
    /// thing: a resolution of another kind is dropped, the first callee wins.
    pub(super) fn merge(self, other: Self) -> Self {
        match (self, other) {
            (StringResolution::Members(members), StringResolution::Members(others)) =>
                StringResolution::Members(members.into_iter().chain(others).unique().collect()),
            (StringResolution::Model(models), StringResolution::Model(others)) =>
                StringResolution::Model(models.into_iter().chain(others).unique().collect()),
            (resolution, _) => resolution,
        }
    }
}

/// Which dotted segments of a field path to return: the one under a byte offset
/// (definition/hover) or every segment (semantic tokens).
#[derive(Clone, Copy)]
pub enum SegmentPick {
    Cursor(usize),
    All,
}

/// What a string refers to given its syntactic context: field/method symbols with the sub-range
/// each covers, or a model.
pub(super) fn resolve_string_in_context(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    string: &ExprStringLiteral,
    string_ctx: &StringContext,
    pick: SegmentPick,
) -> Option<StringResolution> {
    let from_module = session.st().find_module(file_symbol);
    // Any byte inside the string works for the scope.
    let offset = string.range.start().to_usize() + 1;
    let scope = session.st().get_scope_symbol(file_symbol, offset as u32, false);
    match string_ctx {
        StringContext::CallArgument(call_expr) => {
            find_argument_symbols(session, scope, from_module, string, call_expr, pick)
        },
        StringContext::ModelOrder => {
            let model_class = order_model_class(session, scope)?;
            StringResolution::from_members(find_order_fields(session, model_class, from_module, string, pick))
        },
    }
}

/// The symbols of the model `name`, as seen from `from_module`.
pub(super) fn find_model_symbols(session: &SessionInfo, name: &str, from_module: Option<ModuleKey>) -> Vec<ModelSymbolKey> {
    let Some(model) = session.model_mgr().get_model_key(name) else {
        return vec![];
    };
    session.model_mgr()[model].get_model_symbols(session.st(), from_module).collect()
}

/// Resolve a model-related string to its symbols/category, in this order:
/// what its context names (field/method or model) → module → model → xml id.
/// `SegmentPick::All` returns every dotted segment (tokens) instead of just
/// the one under the cursor.
pub fn resolve_string_symbols(
    session: &mut SessionInfo,
    file_symbol: SourceFileKey,
    file_path: &str,
    string: &ExprStringLiteral,
    string_ctx: Option<&StringContext>,
    pick: SegmentPick,
) -> Option<StringResolution> {
    let from_module = session.st().find_module(file_symbol);
    let string_val = string.value.to_str();
    if let Some(string_ctx) = string_ctx {
        if let Some(resolution) = resolve_string_in_context(session, file_symbol, string, string_ctx, pick) {
            return Some(resolution);
        }
        // The `_order` of a model only holds fields: not a module, a model or an XML id. Anywhere
        // else, `_order` is a variable like any other.
        if matches!(string_ctx, StringContext::ModelOrder) {
            let scope = session.st().get_scope_symbol(file_symbol, string.range.start().to_u32() + 1, false);
            if order_model_class(session, scope).is_some() {
                return None;
            }
        }
    }
    if let SourceFileKey::Module(module_key) = file_symbol && file_path.ends_with("__manifest__.py") {
        let dir_alias = {
            let module_symbol = &session.st()[module_key];
            (module_symbol.module_name.as_str() == string_val).then(|| module_symbol.dir_name.clone())
        };
        let lookup = dir_alias.as_ref().map(|d| d.as_str()).unwrap_or(string_val);
        if let Some(module) = session.sync_odoo.modules.get(lookup).copied().and_then(|m| m.upgrade(session.st())) {
            return Some(StringResolution::Module(module));
        }
    }
    let model_syms = find_model_symbols(session, string_val, from_module);
    if !model_syms.is_empty() {
        return Some(StringResolution::Model(model_syms));
    }
    let xml_ids = SyncOdoo::get_xml_ids(session, file_symbol, string_val, &std::ops::Range{start: 0, end: 0}, &mut vec![]);
    let xml_syms: Vec<_> = xml_ids.iter_valid(session.st()).collect();
    if !xml_syms.is_empty() {
        return Some(StringResolution::XmlId(xml_syms));
    }
    None
}
