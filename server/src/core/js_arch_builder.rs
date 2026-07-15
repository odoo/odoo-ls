use crate::utils::HashMap;

use crate::threads::SessionInfo;
use oxc::ast::ast::{Class, Expression, Program, PropertyDefinition, PropertyKey};
use oxc::ast_visit::{Visit, walk};
use ruff_text_size::{TextRange, TextSize};

/// How an OWL component class is exported from its module — decides how the OWL virtual
/// doc can name it. Computed from the module's export entries (not the class-declaration
/// prefix, which misses `class Foo {}` … `export { Foo };`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsExportKind {
    /// Exported under its own name ⇒ `import { Foo } from "./stem"`.
    Named,
    /// The module's default export ⇒ `import Foo from "./stem"`.
    Default,
    /// Not exported, or exported under a different name ⇒ the doc needs a shim. The safe
    /// default: a needless shim only costs a copy, a wrong import silently makes `@this` `any`.
    None,
}

/// A byte span (surrounding quotes excluded) plus the template name string value found in a
/// `static template = "..."` assignment. `range` is in **byte offsets** over the JS source;
/// consumers turn it into an LSP range with the encoding-aware
/// [`FileInfo::text_range_to_range`](crate::core::file_mgr::FileInfo::text_range_to_range).
#[derive(Debug, Clone)]
pub struct JsTemplateRef {
    /// Byte range of the xml_id string content (surrounding quotes excluded).
    pub range: TextRange,
    /// The template name string value (e.g. `"sale.form_view"`).
    pub t_name: String,
    /// The name of the enclosing class, if any.
    pub class_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ComponentDescriptor {
    pub class_name: String,
    pub file_path: String,
    /// Byte offset of the class-name identifier — the go-to-definition target when
    /// navigating from a template back to its component.
    pub class_name_byte: u32,
    /// Name of the class this one `extends`, when it is a plain identifier. Matched by
    /// name against other descriptors to build the subclass graph for inheritance-aware
    /// find-references; aliased imports are not resolved.
    pub super_class_name: Option<String>,
    /// How this class is exported — direct import vs shim for the OWL virtual doc.
    pub export_kind: JsExportKind,
}

fn get_key_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(ident) => Some(ident.name.to_string()),
        PropertyKey::StringLiteral(lit) => Some(lit.value.to_string()),
        _ => None,
    }
}

/// Walks an OXC AST and collects every OWL `template` string assignment plus a
/// [`ComponentDescriptor`] per named class.
///
/// Detected patterns:
/// - `static template = "module.name"` (class property definition)
struct JSArchBuilderVisitor<'e> {
    file_path: String,
    pub refs: Vec<JsTemplateRef>,
    class_stack: Vec<String>,
    pub descriptors: Vec<ComponentDescriptor>,
    /// Local class name → how the module exports it. Missing ⇒ [`JsExportKind::None`].
    exports: &'e HashMap<String, JsExportKind>,
}

impl<'e> JSArchBuilderVisitor<'e> {
    fn new(file_path: String, exports: &'e HashMap<String, JsExportKind>) -> Self {
        Self {
            file_path,
            refs: vec![],
            class_stack: vec![],
            descriptors: vec![],
            exports,
        }
    }
}

impl<'a, 'e> Visit<'a> for JSArchBuilderVisitor<'e> {
    fn visit_class(&mut self, it: &Class<'a>) {
        // Anonymous classes carry no usable identity (nothing can reference them by name).
        if let Some(id) = it.id.as_ref() {
            let name = id.name.to_string();
            let export_kind = self.exports.get(name.as_str()).copied().unwrap_or(JsExportKind::None);
            self.class_stack.push(name.clone());
            self.descriptors.push(ComponentDescriptor {
                class_name: name,
                file_path: self.file_path.clone(),
                class_name_byte: id.span.start,
                super_class_name: match it.super_class.as_ref() {
                    Some(Expression::Identifier(sid)) => Some(sid.name.to_string()),
                    _ => None,
                },
                export_kind,
            });
        }
        walk::walk_class(self, it);
        if it.id.is_some() {
            self.class_stack.pop();
        }
    }

    /// Catch `static template = "..."` inside a class body.
    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        if it.r#static {
            let key_name = get_key_name(&it.key);
            if key_name.as_deref() == Some("template") {
                if let Some(Expression::StringLiteral(lit)) = &it.value {
                    let content_start = lit.span.start + 1;
                    let content_end = lit.span.end.saturating_sub(1);
                    self.refs.push(JsTemplateRef {
                        range: TextRange::new(
                            TextSize::new(content_start),
                            TextSize::new(content_end),
                        ),
                        t_name: lit.value.to_string(),
                        class_name: self.class_stack.last().cloned(),
                    });
                    return; // no need to recurse into value
                }
            }
        }
        walk::walk_property_definition(self, it);
    }
}

pub fn visit_file(
    program: &Program<'_>,
    file_path: &str,
    exports: &HashMap<String, JsExportKind>,
) -> (Vec<JsTemplateRef>, Vec<ComponentDescriptor>) {
    let mut visitor = JSArchBuilderVisitor::new(file_path.to_string(), exports);
    visitor.visit_program(program);
    (visitor.refs, visitor.descriptors)
}

pub fn build(
    session: &mut SessionInfo,
    template_refs: &[JsTemplateRef],
    component_descriptors: &[ComponentDescriptor],
) {
    for descriptor in component_descriptors {
        session.sync_odoo.component_descriptors.insert(descriptor.class_name.clone(), descriptor.clone());
    }

    // Template→declaring classes. Which one wins is decided at query time, by
    // `component_for_template`: a super-chain can cross files not built yet.
    for template_ref in template_refs {
        let Some(class_name) = &template_ref.class_name else { continue };
        let classes = session.sync_odoo.js_component_by_template
            .entry(template_ref.t_name.clone())
            .or_default();
        if !classes.contains(class_name) {
            classes.push(class_name.clone());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use oxc::allocator::Allocator;
    use oxc::parser::Parser;
    use oxc::span::SourceType;

    fn descriptors_of(src: &str, exports: &[(&str, JsExportKind)]) -> Vec<ComponentDescriptor> {
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(std::path::Path::new("/mod/foo.js")).unwrap_or_default();
        let ret = Parser::new(&allocator, src, source_type).parse();
        let program = allocator.alloc(ret.program);
        let map: HashMap<String, JsExportKind> = exports.iter().map(|(n, k)| (n.to_string(), *k)).collect();
        visit_file(program, "/mod/foo.js", &map).1
    }

    #[test]
    fn visit_file_stamps_each_descriptor_with_its_export_kind() {
        // Only the names present in the exports map are exported; the rest default to `None`
        // (the shim path). This is the capture the doc's `import` line depends on.
        let descs = descriptors_of(
            "class Foo {}\nclass Bar {}\nclass Baz {}",
            &[("Foo", JsExportKind::Named), ("Baz", JsExportKind::Default)],
        );
        let kind = |name: &str| descs.iter().find(|d| d.class_name == name).unwrap().export_kind;
        assert_eq!(kind("Foo"), JsExportKind::Named);
        assert_eq!(kind("Bar"), JsExportKind::None);
        assert_eq!(kind("Baz"), JsExportKind::Default);
    }

}
