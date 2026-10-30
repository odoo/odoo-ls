use lsp_types::Diagnostic;
use ruff_python_ast::ExprStringLiteral;
use ruff_text_size::Ranged;
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::evaluation_utils::DeepFieldEvalWalker;
use crate::core::file_mgr::FileMgr;
use crate::core::symbols::symbol_keys::{ClassKey, ModuleKey};
use crate::features::ast_utils::AstUtils;
use crate::features::features_utils::OrderItem;
use crate::threads::SessionInfo;

/// Diagnostics of an order on the model of `class`, e.g. `_order = "name desc"`: its empty items,
/// then the field, property and keywords of each item.
pub(super) fn validate_order(
    session: &mut SessionInfo,
    class: ClassKey,
    maybe_from_module: Option<ModuleKey>,
    order_string: &ExprStringLiteral,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    let order = order_string.value.to_str();
    if order.is_empty() {
        return diagnostics; // an empty order is valid
    }
    let mut reported_comma = None;
    for item in OrderItem::order_items(order) {
        if item.text.trim().is_empty() {
            // `name,,id` or a trailing comma: point at the comma after the item, before it if last,
            // at the start of the string if it has no comma (`' '`). Two empty items can share a
            // comma (`,`): report it once.
            let comma = if item.end() < order.len() { item.end() } else { item.start.saturating_sub(1) };
            if reported_comma != Some(comma) {
                push_order_diagnostic(session, &mut diagnostics, DiagnosticCode::OLS03029, &["empty item"], order_string, comma, comma + 1);
                reported_comma = Some(comma);
            }
            continue;
        }
        check_order_item(session, &mut diagnostics, class, maybe_from_module, order_string, item);
    }
    diagnostics
}

/// Validation for one item of an order, e.g. `partner_id.id desc`: its field must
/// exist, and a many2one only accepts `.id` as property.
fn check_order_item(
    session: &mut SessionInfo,
    diagnostics: &mut Vec<Diagnostic>,
    class: ClassKey,
    maybe_from_module: Option<ModuleKey>,
    order_string: &ExprStringLiteral,
    item: OrderItem,
) {
    check_order_keywords(session, diagnostics, order_string, &item);
    let Some((field_name, start, end)) = item.field() else {
        // The first word starts with a property, e.g. `.id`
        if let Some((word, start)) = item.first_word() {
            let detail = format!("missing field before `{word}`");
            push_order_diagnostic(session, diagnostics, DiagnosticCode::OLS03029, &[&detail], order_string, start, start + word.len());
        }
        return;
    };
    // `parent_id.`: Odoo needs a property after the dot
    if let Some((word, word_start)) = item.first_word()
        && word.ends_with('.')
    {
        let dot = word_start + word.len() - 1;
        push_order_diagnostic(session, diagnostics, DiagnosticCode::OLS03029, &["missing property after `.`"], order_string, dot, dot + 1);
    }
    let mut walker = DeepFieldEvalWalker::new(class.into(), maybe_from_module);
    let symbols = walker.get_model_fields(session, class.into(), field_name);
    if symbols.is_empty() {
        push_order_diagnostic(session, diagnostics, DiagnosticCode::OLS03014, &[], order_string, start, end);
    } else if let Some((property_name, prop_start, prop_end)) = item.property()
        // Other field types may accept properties (e.g. Properties fields): not checked
        && walker.last_field_is_many2one(session)
        && property_name != "id"
    {
        let detail = format!("only `.id` is allowed, not `.{property_name}`");
        push_order_diagnostic(session, diagnostics, DiagnosticCode::OLS03029, &[&detail], order_string, prop_start, prop_end);
    }
}

/// Validation for the keywords after the field of an order item: the first one not allowed
/// where it is (e.g. `dsc`, or `desc` after `nulls first`), a `nulls` without `first` or `last`,
/// and more than one space in `nulls first` or `nulls last`.
fn check_order_keywords(session: &SessionInfo, diagnostics: &mut Vec<Diagnostic>, order_string: &ExprStringLiteral, item: &OrderItem) {
    let order = order_string.value.to_str();
    let keywords = item.keywords();
    let words: Vec<&str> = keywords.iter().map(|&(keyword, _, _)| keyword).collect();
    for (index, &(keyword, start, end)) in keywords.iter().enumerate() {
        let allowed = OrderItem::next_keywords(&words[..index]);
        if !allowed.contains(&keyword.to_ascii_lowercase().as_str()) {
            let detail = match allowed {
                [] => format!("unexpected `{keyword}`"),
                _ => {
                    let expected = allowed.iter().map(|allowed| format!("`{allowed}`")).collect::<Vec<_>>().join(" or ");
                    format!("expected {expected}, got `{keyword}`")
                },
            };
            push_order_diagnostic(session, diagnostics, DiagnosticCode::OLS03029, &[&detail], order_string, start, end);
            return;
        }
        if index > 0 && words[index - 1].eq_ignore_ascii_case("nulls") {
            let (_, nulls_start, nulls_end) = keywords[index - 1];
            if &order[nulls_end..start] != " " {
                let detail = format!("`nulls {keyword}` takes a single space");
                push_order_diagnostic(session, diagnostics, DiagnosticCode::OLS03029, &[&detail], order_string, nulls_start, end);
                return;
            }
        }
    }
    if let Some(&(keyword, start, end)) = keywords.last()
        && keyword.eq_ignore_ascii_case("nulls")
    {
        let detail = format!("`{keyword}` must be followed by `first` or `last`");
        push_order_diagnostic(session, diagnostics, DiagnosticCode::OLS03029, &[&detail], order_string, start, end);
    }
}

/// Report `code` on `order_string[start..end]`, or on the whole string if that piece can't be
/// found in the file.
fn push_order_diagnostic(
    session: &SessionInfo,
    diagnostics: &mut Vec<Diagnostic>,
    code: DiagnosticCode,
    params: &[&str],
    order_string: &ExprStringLiteral,
    start: usize,
    end: usize,
) {
    let Some(diagnostic_base) = create_diagnostic(session, code, params) else {
        return;
    };
    let range = AstUtils::range_in_file(order_string, start, end).unwrap_or(order_string.range());
    diagnostics.push(Diagnostic {
        range: FileMgr::textRange_to_temporary_Range(&range),
        ..diagnostic_base
    });
}
