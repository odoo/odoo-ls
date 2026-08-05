use crate::utils::HashMap;

use crate::threads::SessionInfo;
use oxc::ast::ast::{ArrowFunctionExpression, BindingPattern, Class, Expression, Function, FunctionType, MethodDefinition, MethodDefinitionKind, Program, PropertyDefinition, PropertyKey, VariableDeclarator};
use crate::Sy;
use crate::constants::OYarn;
use lsp_types::SymbolKind;
use oxc::ast_visit::{Visit, walk};
use oxc::span::{GetSpan, Span};
use oxc::syntax::scope::ScopeFlags;
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

/// A named declaration worth offering in workspace symbols: a class, a top-level function or a
/// class member. `range` is the **byte range of the declared name** (not of the whole
/// declaration); consumers turn it into an LSP range with the encoding-aware
/// [`FileInfo::text_range_to_range`](crate::core::file_mgr::FileInfo::text_range_to_range).
///
/// Variables are deliberately absent — the Python walk skips them too, and they drown the
/// picker. A top-level `const` bound to a function or a class expression is not a variable in
/// that sense and IS collected.
#[derive(Debug, Clone)]
pub struct JsDeclaration {
    pub name: OYarn,
    pub kind: SymbolKind,
    /// Byte range of the declared name.
    pub range: TextRange,
    /// Name of the enclosing class, if any.
    pub container: Option<OYarn>,
}

fn span_to_range(span: Span) -> TextRange {
    TextRange::new(TextSize::new(span.start), TextSize::new(span.end))
}

fn get_key_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(ident) => Some(ident.name.to_string()),
        PropertyKey::StringLiteral(lit) => Some(lit.value.to_string()),
        _ => None,
    }
}

/// Walks an OXC AST and collects every OWL `template` string assignment, a
/// [`ComponentDescriptor`] per named class, and every [`JsDecl`].
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
    pub decls: Vec<JsDeclaration>,
    /// Enclosing class names, for [`JsDecl::container`]. Distinct from `class_stack`: it also
    /// carries the binding name of an anonymous `const X = class {…}`.
    container_stack: Vec<OYarn>,
    /// How many function/arrow bodies enclose the node being visited. Only 0 is module scope.
    fn_depth: usize,
}

impl<'e> JSArchBuilderVisitor<'e> {
    fn new(file_path: String, exports: &'e HashMap<String, JsExportKind>) -> Self {
        Self {
            file_path,
            refs: vec![],
            class_stack: vec![],
            descriptors: vec![],
            exports,
            decls: vec![],
            container_stack: vec![],
            fn_depth: 0,
        }
    }

    fn push_decl(&mut self, name: OYarn, kind: SymbolKind, span: Span) {
        let container = self.container_stack.last().cloned();
        self.decls.push(JsDeclaration { name, kind, range: span_to_range(span), container });
    }
}

impl<'a, 'e> Visit<'a> for JSArchBuilderVisitor<'e> {
    fn visit_class(&mut self, it: &Class<'a>) {
        // Anonymous classes carry no usable identity (nothing can reference them by name).
        if let Some(id) = it.id.as_ref() {
            let name = id.name.to_string();
            let export_kind = self.exports.get(name.as_str()).copied().unwrap_or(JsExportKind::None);
            self.class_stack.push(name.clone());
            // Declared before the class becomes the container, so it is not its own container.
            let interned = Sy!(name.clone());
            self.push_decl(interned.clone(), SymbolKind::CLASS, id.span);
            self.container_stack.push(interned);
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
            self.container_stack.pop();
        }
    }

    /// Class members. A getter/setter is reported as a property: it reads as one at the call
    /// site, which is how someone searching for it thinks of it.
    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        // `constructor` is the same name in every class — pure noise in a name search.
        if it.kind != MethodDefinitionKind::Constructor && let Some(name) = get_key_name(&it.key) {
            let kind = match it.kind {
                MethodDefinitionKind::Get | MethodDefinitionKind::Set => SymbolKind::PROPERTY,
                _ => SymbolKind::METHOD,
            };
            self.push_decl(Sy!(name), kind, it.key.span());
        }
        walk::walk_method_definition(self, it);
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        // Function *expressions* are reached through their binding (a declarator, a method, an
        // object property), so only declarations are named here.
        if it.r#type == FunctionType::FunctionDeclaration && let Some(id) = it.id.as_ref() {
            self.push_decl(Sy!(id.name.to_string()), SymbolKind::FUNCTION, id.span);
        }
        self.fn_depth += 1;
        walk::walk_function(self, it, flags);
        self.fn_depth -= 1;
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.fn_depth += 1;
        walk::walk_arrow_function_expression(self, it);
        self.fn_depth -= 1;
    }

    /// A module-scope `const` bound to a function or a class expression — Odoo declares much of
    /// its API this way (`export const patch = (obj) => {…}`). Anything else bound to a `const`
    /// is data, and stays out. Nested bindings (a helper closure inside a method) are skipped:
    /// that is the noise this gate exists for.
    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        let mut pushed_container = false;
        if self.fn_depth == 0 && let BindingPattern::BindingIdentifier(id) = &it.id {
            match &it.init {
                // A named class expression is reported by `visit_class` under its own name.
                Some(Expression::ClassExpression(class)) if class.id.is_none() => {
                    let interned = Sy!(id.name.to_string());
                    self.push_decl(interned.clone(), SymbolKind::CLASS, id.span);
                    self.container_stack.push(interned);
                    pushed_container = true;
                }
                Some(Expression::ArrowFunctionExpression(_)) | Some(Expression::FunctionExpression(_)) => {
                    self.push_decl(Sy!(id.name.to_string()), SymbolKind::FUNCTION, id.span);
                }
                _ => {}
            }
        }
        walk::walk_variable_declarator(self, it);
        if pushed_container {
            self.container_stack.pop();
        }
    }

    /// Catch `static template = "..."` inside a class body.
    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        if it.r#static {
            let key_name = get_key_name(&it.key);
            if key_name.as_deref() == Some("template")
            && let Some(Expression::StringLiteral(lit)) = &it.value {
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
        walk::walk_property_definition(self, it);
    }
}

pub fn visit_file(
    program: &Program<'_>,
    file_path: &str,
    exports: &HashMap<String, JsExportKind>,
) -> (Vec<JsTemplateRef>, Vec<ComponentDescriptor>, Vec<JsDeclaration>) {
    let mut visitor = JSArchBuilderVisitor::new(file_path.to_string(), exports);
    visitor.visit_program(program);
    (visitor.refs, visitor.descriptors, visitor.decls)
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

    fn visit(src: &str, exports: &[(&str, JsExportKind)]) -> (Vec<JsTemplateRef>, Vec<ComponentDescriptor>, Vec<JsDeclaration>) {
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(std::path::Path::new("/mod/foo.js")).unwrap_or_default();
        let ret = Parser::new(&allocator, src, source_type).parse();
        let program = allocator.alloc(ret.program);
        let map: HashMap<String, JsExportKind> = exports.iter().map(|(n, k)| (n.to_string(), *k)).collect();
        visit_file(program, "/mod/foo.js", &map)
    }

    fn descriptors_of(src: &str, exports: &[(&str, JsExportKind)]) -> Vec<ComponentDescriptor> {
        visit(src, exports).1
    }

    fn decls_of(src: &str) -> Vec<JsDeclaration> {
        visit(src, &[]).2
    }

    /// (name, kind, container) — the triple a workspace-symbol result is built from.
    fn shape(decls: &[JsDeclaration]) -> Vec<(&str, SymbolKind, Option<&str>)> {
        decls.iter().map(|d| (d.name.as_str(), d.kind, d.container.as_ref().map(|c| c.as_str()))).collect()
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

    #[test]
    fn decls_collect_classes_members_and_functions() {
        let decls = decls_of(
            r#"
            export class ListRenderer extends Component {
                static template = "web.ListRenderer";
                constructor() { super(); }
                setup() { this.x = 1; }
                get rowCount() { return 0; }
                set rowCount(v) {}
                static makeKey(r) { return r.id; }
            }
            export function formatDate(d) { return d; }
            "#,
        );
        assert_eq!(shape(&decls), vec![
            ("ListRenderer", SymbolKind::CLASS, None),
            ("setup", SymbolKind::METHOD, Some("ListRenderer")),
            ("rowCount", SymbolKind::PROPERTY, Some("ListRenderer")),
            ("rowCount", SymbolKind::PROPERTY, Some("ListRenderer")),
            ("makeKey", SymbolKind::METHOD, Some("ListRenderer")),
            ("formatDate", SymbolKind::FUNCTION, None),
        ]);
    }

    #[test]
    fn decls_take_function_valued_consts_but_no_data_consts() {
        let decls = decls_of(
            r#"
            export const patch = (obj, ext) => {};
            const legacy = function (a) { return a; };
            const Anon = class extends Component { onClick() {} };
            const Named = class Inner { m() {} };
            const FIELD_TYPES = { char: 1 };
            const RE = /ab+c/;
            export const LIMIT = 80;
            "#,
        );
        assert_eq!(shape(&decls), vec![
            ("patch", SymbolKind::FUNCTION, None),
            ("legacy", SymbolKind::FUNCTION, None),
            // The anonymous class takes its binding's name, and owns its members.
            ("Anon", SymbolKind::CLASS, None),
            ("onClick", SymbolKind::METHOD, Some("Anon")),
            // A *named* class expression is reported once, under its own name.
            ("Inner", SymbolKind::CLASS, None),
            ("m", SymbolKind::METHOD, Some("Inner")),
        ]);
    }

    #[test]
    fn decls_skip_locals_nested_in_a_function_body() {
        // The noise gate: only module-scope bindings are declarations. Object-literal methods
        // (the registry idiom) are function expressions, not class members — also out.
        let decls = decls_of(
            r#"
            export function useThing() {
                const helper = () => {};
                const Inner = class {};
                return { start() {} };
            }
            registry.category("services").add("thing", { start() {} });
            "#,
        );
        assert_eq!(shape(&decls), vec![("useThing", SymbolKind::FUNCTION, None)]);
    }

    #[test]
    fn decl_range_covers_the_name_only() {
        let src = "class ListRenderer {}";
        let decls = decls_of(src);
        let range = decls[0].range;
        assert_eq!(&src[usize::from(range.start())..usize::from(range.end())], "ListRenderer");
    }
}
