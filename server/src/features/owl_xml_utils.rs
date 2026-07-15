/// Attribute names whose value is a QWeb template *name*: `t-name` declares one, `t-call` /
/// `t-inherit` reference one. Drives both the `type` semantic tokens and the template-name
/// Definition dispatch (`definition.rs::find_template_name_attr`).
pub const TEMPLATE_NAME_ATTRS: &[&str] = &["t-name", "t-call", "t-inherit"];

/// An OWL directive attribute whose *entire* value is a single JS expression (fed straight
/// to OWL's `compileExpr`): the whole value splices as one `return (EXPR)`. Excludes the
/// interpolation directives ([`is_owl_interp_attr`]) and the name/binding directives
/// (`t-name`, `t-set`, `t-as`, `t-custom-ref`).
pub fn is_owl_expression_attr(name: &str) -> bool {
    matches!(
        name,
        "t-if" | "t-elif" | "t-out" | "t-esc" | "t-value" | "t-key" | "t-model"
            | "t-foreach" | "t-props" | "t-component"
            | "t-att" | "t-ref" | "t-tag" | "t-call-context" | "t-log"
    ) || name.starts_with("t-att-")
        || name.starts_with("t-on-")
}

/// An OWL directive attribute whose value is a *string interpolation*: literal text mixed
/// with `{{ … }}` / `#{ … }` chunks, each an embedded JS expression. One value yields
/// several expressions, at each chunk's inner range. A static `t-call` has no chunk and is
/// handled by the template-name navigation instead.
pub fn is_owl_interpolation_attr(name: &str) -> bool {
    name.starts_with("t-attf-") || name == "t-call"
}

/// Whether an XML element is an OWL sub-component invocation, mirroring OWL's compiler
/// rule: a capitalized tag (`<ChangeLine/>`) or a dynamic `<t t-component="...">`.
pub fn tag_is_component(node: roxmltree::Node) -> bool {
    node.has_attribute("t-component")
        || node.tag_name().name().chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// The class-name reference carried by a *static* component tag (`<ChangeLine/>`), as
/// `(xml_byte_offset, name_len)`; `None` for plain tags and the dynamic `<t t-component>`
/// form (whose class reference is the `t-component` expression). The name begins one byte
/// past the opening `<` (XML forbids whitespace there).
pub fn component_tag_name_range(node: roxmltree::Node) -> Option<(usize, usize)> {
    let name = node.tag_name().name();
    if !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    Some((node.range().start + 1, name.len()))
}

/// Whether a *component* attribute is a prop whose value is a JS expression. OWL compiles
/// every non-directive attribute on a component (incl. `class`/`style` and the `.bind` /
/// `.signal` / `.alike` suffixes) in the parent's context; the sole exception is the
/// `.translate` suffix, a plain string.
pub fn is_prop_expr_attr(name: &str) -> bool {
    !name.starts_with("t-") && !name.ends_with(".translate")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_tag_name_range_targets_static_components() {
        // Capitalized tags (incl. dotted names) are static component references; plain tags and
        // the dynamic `<t t-component>` form (lowercase `t`) are not.
        let xml = r#"<t><ChangeLine/><Foo.Bar/><div/><t t-component="x"/></t>"#;
        let document = roxmltree::Document::parse(xml).unwrap();
        for node in document.descendants().filter(|n| n.is_element()) {
            let name = node.tag_name().name();
            match name {
                "ChangeLine" | "Foo.Bar" => {
                    let (off, len) = component_tag_name_range(node).expect("static component");
                    // The recorded range slices back to exactly the tag name in the XML.
                    assert_eq!(&xml[off..off + len], name);
                }
                _ => assert!(component_tag_name_range(node).is_none(), "{name} is not static"),
            }
        }
    }
}
