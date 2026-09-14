use oxc::{allocator::Allocator, parser::Parser, span::SourceType};

use odoo_ls_server::{core::js_arch_builder::{ComponentDescriptor, visit_file}, utils::HashMap};

/// Parse `source` as a `.js` file and run the OWL arch-builder visitor on it,
/// mirroring what `FileInfo::build_js_ast` does before semantic analysis.
///
/// Each template ref carries a byte `range` covering only the string content
/// (surrounding quotes excluded), the xml_id, and the enclosing class name.
fn visit(source: &str) -> Vec<ComponentDescriptor> {
    let path = "owl_component.js";
    let source_type = SourceType::from_path(path).unwrap_or_default();
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    assert!(ret.errors.is_empty(), "unexpected parse errors: {:?}", ret.errors);
    let program = allocator.alloc(ret.program);
    // No exported-symbol map needed here: these tests only assert template/class detection.
    let (descriptors, _decls) = visit_file(program, path, &HashMap::default(), &HashMap::default());
    descriptors
}


/// `static template = "..."` inside a class body should be picked up as a `JsTemplateRef`,
/// with the range covering only the string content (quotes excluded).
#[test]
fn test_static_template_is_detected() {
    let source = "export class Counter extends Component {\n    static template = \"module_owl.Counter\";\n}\n";
    let components = visit(source);
    let refs = components.iter().filter_map(|d| d.template.as_ref()).collect::<Vec<_>>();
    assert_eq!(components.len(), 1, "expected exactly one component, got {:?}", components);
    assert_eq!(refs.len(), 1, "expected exactly one template ref, got {:?}", refs);
    assert_eq!(refs[0].t_name, "module_owl.Counter");
    assert_eq!(components[0].class_name, "Counter");
    // range should exclude the surrounding quotes
    let start: usize = refs[0].range.start().to_usize();
    let end: usize = refs[0].range.end().to_usize();
    assert_eq!(&source[start..end], "module_owl.Counter");
}

/// Single-quoted template strings should be detected the same way as double-quoted ones.
#[test]
fn test_static_template_single_quotes() {
    let source = "export class Counter extends Component {\n    static template = 'module_owl.Counter';\n}\n";
    let components = visit(source);
    let refs = components.iter().filter_map(|d| d.template.as_ref()).collect::<Vec<_>>();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].t_name, "module_owl.Counter");
}

/// A `template` property that isn't `static`, or isn't a plain string literal, must not be
/// mistaken for an OWL template reference.
#[test]
fn test_non_static_or_non_literal_template_is_ignored() {
    let source = "export class Counter extends Component {\n    template = \"not.static\";\n    static template = someHelper();\n}\n";
    let components = visit(source);
    assert_eq!(components.len(), 1, "expected exactly one component, got {:?}", components);
    let refs = components.iter().filter_map(|d| d.template.as_ref()).collect::<Vec<_>>();
    assert!(refs.is_empty(), "expected no template refs, got {:?}", refs);
}

/// Multiple OWL components in the same file should each produce their own template ref
#[test]
fn test_multiple_components_in_one_file() {
    let source = concat!(
        "export class Counter extends Component {\n",
        "    static template = \"module_owl.Counter\";\n",
        "}\n",
        "export class Display extends Component {\n",
        "    static template = \"module_owl.Display\";\n",
        "}\n",
    );
    let components = visit(source);
    assert_eq!(components.len(), 2);
    assert_eq!(components[0].class_name, "Counter");
    assert_eq!(components[0].template.as_ref().unwrap().t_name, "module_owl.Counter");
    assert_eq!(components[1].class_name, "Display");
    assert_eq!(components[1].template.as_ref().unwrap().t_name, "module_owl.Display");
}

/// A nested anonymous class is not a member of its enclosing class, so it must not
/// contribute that class's template.
#[test]
fn test_nested_anonymous_class_template_does_not_leak_outward() {
    let source = concat!(
        "class Counter extends Component {\n",
        "    static template = \"module_owl.Counter\";\n",
        "    make() { return class extends Component { static template = \"module_owl.Inner\"; }; }\n",
        "}\n",
    );
    let components = visit(source);
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].class_name, "Counter");
    assert_eq!(components[0].template.as_ref().unwrap().t_name, "module_owl.Counter");
}
