use crate::threads::SessionInfo;

/// Whether a template *reference* (a `t-call` / `t-inherit` value, or a JS `static
/// template` string) resolves to at least one declared template — the same `js_templates`
/// lookup go-to-definition uses, so a reference is highlighted exactly when Definition
/// would navigate from it. Backend QWeb views (xml_id-referenced) do not resolve here.
pub fn template_reference_resolves(session: &SessionInfo, name: &str) -> bool {
    // Interpolated names can't be resolved statically (the validator skips them too).
    if name.contains("{{") || name.contains("#{") {
        return false;
    }
    session
        .sync_odoo
        .js_templates
        .get(name)
        .is_some_and(|templates| !templates.is_empty(&session.sync_odoo.symbol_table))
}
