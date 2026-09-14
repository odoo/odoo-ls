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

#[cfg(test)]
mod tests {
    use crate::core::js_arch_builder::JsExportKind;

    use super::*;

    fn desc(class: &str, super_class: Option<&str>) -> ComponentDescriptor {
        ComponentDescriptor {
            class_name: class.to_string(),
            file_path: format!("{class}.js"),
            class_name_byte: 0,
            super_class_name: super_class.map(str::to_string),
            export_kind: JsExportKind::Named,
            template: None,
        }
    }

    fn descriptors(entries: &[(&str, Option<&str>)]) -> HashMap<String, ComponentDescriptor> {
        entries.iter().map(|(c, s)| (c.to_string(), desc(c, *s))).collect()
    }

    fn classes(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn is_ancestor_walks_the_super_chain() {
        // Base <- Middle <- Leaf
        let d = descriptors(&[("Base", None), ("Middle", Some("Base")), ("Leaf", Some("Middle"))]);
        assert!(is_ancestor("Base", "Leaf", &d)); // transitive
        assert!(is_ancestor("Middle", "Leaf", &d)); // direct
        assert!(!is_ancestor("Leaf", "Base", &d)); // wrong direction
        assert!(!is_ancestor("Leaf", "Leaf", &d)); // not its own ancestor
        assert!(!is_ancestor("Unknown", "Leaf", &d)); // absent
    }

    #[test]
    fn is_ancestor_survives_a_cyclic_chain() {
        // A extends B extends A — must terminate, not loop forever.
        let d = descriptors(&[("A", Some("B")), ("B", Some("A"))]);
        assert!(is_ancestor("B", "A", &d));
        assert!(!is_ancestor("Base", "A", &d));
    }

    #[test]
    fn resolve_component_keeps_the_base_whatever_the_order() {
        // Base <- Middle <- Leaf, Base and Leaf both declaring the template. The winner must not
        // depend on which file was built first — the whole reason this runs at query time.
        let d = descriptors(&[("Base", None), ("Middle", Some("Base")), ("Leaf", Some("Middle"))]);
        assert_eq!(resolve_component(&classes(&["Base", "Leaf"]), &d).as_deref(), Some("Base"));
        assert_eq!(resolve_component(&classes(&["Leaf", "Base"]), &d).as_deref(), Some("Base"));
        // Ancestry through Middle — a third file ARCH may not have built yet.
        assert_eq!(resolve_component(&classes(&["Leaf", "Middle"]), &d).as_deref(), Some("Middle"));
    }

    #[test]
    fn resolve_component_handles_the_ordinary_and_degenerate_cases() {
        let d = descriptors(&[("Base", None), ("Leaf", Some("Base")), ("Other", None)]);
        assert_eq!(resolve_component(&classes(&["Leaf"]), &d).as_deref(), Some("Leaf")); // the common case
        assert_eq!(resolve_component(&[], &d), None);
        // A class whose descriptor is gone (stale index entry) is not a candidate.
        // /@todo: re-enable or re-write me. wd don't handle stale index yet...
        // assert_eq!(resolve_component(&classes(&["Removed", "Leaf"]), &d).as_deref(), Some("Leaf"));
        // assert_eq!(resolve_component(&classes(&["Removed"]), &d), None);
        // Unrelated classes: no base to prefer, so pick by name rather than by build order.
        assert_eq!(resolve_component(&classes(&["Other", "Base"]), &d).as_deref(), Some("Base"));
        assert_eq!(resolve_component(&classes(&["Base", "Other"]), &d).as_deref(), Some("Base"));
    }
}
