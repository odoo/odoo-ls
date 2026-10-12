use std::{env, fs};
use std::path::PathBuf;

use lsp_types::{Diagnostic, NumberOrString};
use odoo_ls_server::utils::PathSanitizer;

use crate::setup::setup::*;

/// (line, code) pairs expected from the `# expect: code, code` comments of a file
fn expected_from_markers(source: &str) -> Vec<(u32, String)> {
    let mut expected = vec![];
    for (line, text) in source.lines().enumerate() {
        if let Some((_, codes)) = text.split_once("# expect:") {
            expected.extend(codes.split(',').map(|code| (line as u32, code.trim().to_string())));
        }
    }
    expected.sort();
    expected
}

/// (line, code) pairs of the PyLint diagnostics
fn pylint_diagnostics(diagnostics: &[Diagnostic]) -> Vec<(u32, String)> {
    let mut result: Vec<(u32, String)> = diagnostics.iter().filter_map(|d| match &d.code {
        Some(NumberOrString::String(code)) if code.starts_with("pylint_") => Some((d.range.start.line, code.clone())),
        _ => None,
    }).collect();
    result.sort();
    result
}

fn assert_markers(source: &str, diagnostics: &[Diagnostic]) {
    let expected = expected_from_markers(source);
    let found = pylint_diagnostics(diagnostics);
    let missing: Vec<_> = expected.iter().filter(|e| !found.contains(e)).map(|(l, c)| format!("{c} on line {}", l + 1)).collect();
    let unexpected: Vec<_> = found.iter().filter(|f| !expected.contains(f)).map(|(l, c)| format!("{c} on line {}", l + 1)).collect();
    assert!(missing.is_empty() && unexpected.is_empty(), "missing: {missing:?}\nunexpected: {unexpected:?}");
}

/// The syntactic PyLint checks, on a standalone file.
/// The file is copied outside of the `tests` directory, as the gettext checks skip test files.
#[test]
fn test_pylint_checks() {
    let (mut odoo, config) = setup_server(false);
    let mut session = create_init_session(&mut odoo, config);
    let source_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/python/diagnostics/pylint_checks.py");
    let source = fs::read_to_string(&source_path).expect("unable to read pylint_checks.py");
    let dir = env::temp_dir().join(format!("odools_pylint_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pylint_checks.py");
    fs::write(&path, &source).unwrap();
    let path = path.sanitize();
    prepare_custom_entry_point(&mut session, &path);
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    let _ = fs::remove_dir_all(&dir);

    assert_markers(&source, &diagnostics);
    let redefinition = diagnostics.iter().find(|d| d.message.starts_with("method already defined line"));
    assert!(redefinition.is_some(), "function-redefined should name the kind and line of the first definition");
    let unreachable = diagnostics.iter().find(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "pylint_W0101")).unwrap();
    assert_eq!(unreachable.tags, Some(vec![lsp_types::DiagnosticTag::UNNECESSARY]));
}

/// raise-unlink-override needs Odoo to know which classes are models
#[test]
fn test_pylint_raise_unlink_override() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/addons/module_for_diagnostics/models/pylint_unlink.py").sanitize();
    let source = fs::read_to_string(&path).expect("unable to read pylint_unlink.py");
    let diagnostics = get_diagnostics_for_path(&mut session, &path);
    assert_markers(&source, &diagnostics);
}
