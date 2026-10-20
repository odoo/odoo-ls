use std::path::PathBuf;

use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};
use odoo_ls_server::utils::PathSanitizer;

use crate::{setup::setup::*, test_utils::diag_on_line};

/// OLS03014 and OLS03029 on `_order`: each case is a class of `model_order.py`, found by its
/// `_order` line. An invalid `_order` gets exactly one Error, pointing at the given text.
#[test]
fn test_ols03029_model_order() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("addons")
        .join("module_for_diagnostics")
        .join("models")
        .join("model_order.py")
        .sanitize();
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let lines: Vec<String> = std::fs::read_to_string(&path).unwrap().lines().map(str::to_string).collect();

    // (the `_order` statement as written, then the expected code and the text it points at, if invalid)
    let cases: &[(&str, Option<(&str, &str)>)] = &[
        ("_order = 'name desc, id'", None),
        ("_order = 'name DESC NULLS LAST'", None),
        ("_order = 'parent_id.id desc'", None),
        ("_order = 'na' 'me'", None),
        (r#"_order = "name,\tid""#, None),
        ("_order = ''", None),
        ("_order = 'unknown_field'", Some(("OLS03014", "unknown_field"))),
        // Quotes and `:func` are not supported: looked up as written
        (r#"_order = '"name"'"#, Some(("OLS03014", r#""name""#))),
        ("_order = 'name:month desc'", Some(("OLS03014", "name:month"))),
        ("_order = 'name dsc'", Some(("OLS03029", "dsc"))),
        ("_order = 'name nulls'", Some(("OLS03029", "nulls"))),
        ("_order = 'name nulls firts'", Some(("OLS03029", "firts"))),
        ("_order = 'name nulls first desc'", Some(("OLS03029", "desc"))),
        ("_order = 'name nulls  first'", Some(("OLS03029", "nulls  first"))),
        ("_order = 'name,,id'", Some(("OLS03029", ","))),
        ("_order = 'name,'", Some(("OLS03029", ","))),
        // Two empty items around one comma: reported once
        ("_order = ','", Some(("OLS03029", ","))),
        // No comma: the empty item is reported at the start of the string
        ("_order = ' '", Some(("OLS03029", " "))),
        // Properties of other field types than many2one are not checked
        ("_order = 'name.id'", None),
        ("_order = 'parent_id.name'", Some(("OLS03029", "name"))),
        ("_order = '.id desc'", Some(("OLS03029", ".id"))),
        // A property is required after the dot, whatever the field
        ("_order = 'parent_id.'", Some(("OLS03029", "."))),
        ("_order = 'parent_id. desc'", Some(("OLS03029", "."))),
        ("_order = 'name.'", Some(("OLS03029", "."))),
        // Annotated assignment: validated like the plain one
        ("_order: str = 'unknown_field'", Some(("OLS03014", "unknown_field"))),
        // A constant is not validated: no diagnostic on `_order`, nor on the constant's string
        ("_order = ORDER", None),
        ("ORDER = 'unknown_field'", None),
        // `# noqa: OLS03014` on the class: only the wrong direction is reported
        ("_order = 'unknown_noqa_field dsc'", Some(("OLS03029", "dsc"))),
    ];
    let mut failures = vec![];
    for &(statement, expected) in cases {
        let Some(line) = lines.iter().position(|text| text.trim() == statement) else {
            failures.push(format!("{statement}: no such line in model_order.py"));
            continue;
        };
        let text = &lines[line];
        let found = diag_on_line(&diagnostics, line as u32);
        let as_expected = match expected {
            None => found.is_empty(),
            Some((code, pointed)) => matches!(found.as_slice(), [diagnostic]
                if matches!(&diagnostic.code, Some(NumberOrString::String(c)) if c == code)
                && diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && pointed_text(diagnostic, text) == Some(pointed)),
        };
        if !as_expected {
            let found: Vec<_> = found.iter().map(|diagnostic| describe(diagnostic, text)).collect();
            failures.push(format!("{statement}: expected {expected:?}, got {found:?}"));
        }
    }
    assert!(failures.is_empty(), "_order diagnostics:\n{}", failures.join("\n"));
}

/// Text of `line` under a diagnostic on that line only.
fn pointed_text<'a>(diagnostic: &Diagnostic, line: &'a str) -> Option<&'a str> {
    let range = diagnostic.range;
    if range.start.line != range.end.line {
        return None;
    }
    line.get(range.start.character as usize..range.end.character as usize)
}

fn describe(diagnostic: &Diagnostic, line: &str) -> String {
    let code = match &diagnostic.code {
        Some(NumberOrString::String(code)) => code.clone(),
        other => format!("{other:?}"),
    };
    format!("{code} {:?} on {:?}: {}", diagnostic.severity, pointed_text(diagnostic, line), diagnostic.message)
}
