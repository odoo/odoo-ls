use oxc::{allocator::Allocator, parser::Parser, span::SourceType};

use odoo_ls_server::{core::js_arch_builder::{ComponentDescriptor, JsTemplateRef, visit_file}, utils::HashMap};

/// Parse `source` as a `.js` file and run the OWL arch-builder visitor on it,
/// mirroring what `FileInfo::build_js_ast` does before semantic analysis.
///
/// Each template ref carries a byte `range` covering only the string content
/// (surrounding quotes excluded), the xml_id, and the enclosing class name.
fn visit(source: &str) -> (Vec<JsTemplateRef>, Vec<ComponentDescriptor>) {
    let path = "owl_component.js";
    let source_type = SourceType::from_path(path).unwrap_or_default();
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    assert!(ret.errors.is_empty(), "unexpected parse errors: {:?}", ret.errors);
    let program = allocator.alloc(ret.program);
    // No exported-symbol map needed here: these tests only assert template/class detection.
    visit_file(program, path, &HashMap::default())
}


/// `static template = "..."` inside a class body should be picked up as a `JsTemplateRef`,
/// with the range covering only the string content (quotes excluded).
#[test]
fn test_static_template_is_detected() {
    let source = "export class Counter extends Component {\n    static template = \"module_owl.Counter\";\n}\n";
    let (refs, _) = visit(source);
    assert_eq!(refs.len(), 1, "expected exactly one template ref, got {:?}", refs);
    assert_eq!(refs[0].t_name, "module_owl.Counter");
    assert_eq!(refs[0].class_name, Some("Counter".to_string()));
    // range should exclude the surrounding quotes
    let start: usize = refs[0].range.start().to_usize();
    let end: usize = refs[0].range.end().to_usize();
    assert_eq!(&source[start..end], "module_owl.Counter");
}

/// Single-quoted template strings should be detected the same way as double-quoted ones.
#[test]
fn test_static_template_single_quotes() {
    let source = "export class Counter extends Component {\n    static template = 'module_owl.Counter';\n}\n";
    let (refs, _) = visit(source);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].t_name, "module_owl.Counter");
}

/// A `template` property that isn't `static`, or isn't a plain string literal, must not be
/// mistaken for an OWL template reference.
#[test]
fn test_non_static_or_non_literal_template_is_ignored() {
    let source = "export class Counter extends Component {\n    template = \"not.static\";\n    static template = someHelper();\n}\n";
    let (refs, _) = visit(source);
    assert!(refs.is_empty(), "expected no template refs, got {:?}", refs);
}

/// Multiple OWL components in the same file should each produce their own template ref,
/// correctly attributed to their enclosing class.
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
    let (refs, descriptors) = visit(source);
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].t_name, "module_owl.Counter");
    assert_eq!(refs[0].class_name, Some("Counter".to_string()));
    assert_eq!(refs[1].t_name, "module_owl.Display");
    assert_eq!(refs[1].class_name, Some("Display".to_string()));
    let names: Vec<_> = descriptors.iter().map(|d| d.class_name.clone()).collect();
    assert!(names.contains(&"Counter".to_string()));
    assert!(names.contains(&"Display".to_string()));
}
