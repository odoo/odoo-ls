use lsp_server::Message;
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, PublishDiagnosticsParams};
use lsp_types::notification::{Notification, PublishDiagnostics};
use odoo_ls_server::S;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::{HashMap, HashSet};

pub fn get_diagnostics_for_path(session: &mut SessionInfo, path: &str) -> Vec<Diagnostic> {
    let mut res = vec![];
    while let Some(msg) = session._consume_message() {
        if let Message::Notification(n) = msg
            && n.method == PublishDiagnostics::METHOD {
                let params: PublishDiagnosticsParams = serde_json::from_value(n.params).expect("Unable to parse PublishDiagnosticsParams");
                let params_path = FileMgr::uri2pathname(params.uri.as_str());
                if params_path == path {
                    res.extend(params.diagnostics);
                }
            }
    }
    res
}

pub fn get_diagnostics_for_paths(session: &mut SessionInfo, paths: &[String]) -> HashMap<String, Vec<Diagnostic>> {
    let mut res = HashMap::default();
    while let Some(msg) = session._consume_message() {
        if let Message::Notification(n) = msg
            && n.method == PublishDiagnostics::METHOD {
                let params: PublishDiagnosticsParams = serde_json::from_value(n.params).expect("Unable to parse PublishDiagnosticsParams");
                let params_path = FileMgr::uri2pathname(params.uri.as_str());
                if paths.contains(&params_path) {
                    // publishDiagnostics carries the whole file each time, so the last one wins
                    res.insert(params_path, params.diagnostics);
                }
            }
    }
    res
}

pub fn get_diagnostics_test_comments(session: &mut SessionInfo, path: &str) -> Vec<(u32, Vec<String>)> {
    let file_mgr = session.file_mgr();
    let file_info = file_mgr.get_file_info(&S!(path)).expect("File info not found");
    session.file_mgr()[file_info].diag_test_comments.clone()
}

pub fn diag_on_line(diagnostics: &[lsp_types::Diagnostic], line: u32) -> Vec<&lsp_types::Diagnostic> {
    diagnostics.iter().filter(|d| d.range.start.line <= line && d.range.end.line >= line).collect()
}

/**
 * Verify that the given diagnostics match the expected diagnostics from doc_diag, generated from comments in the source code
 */
#[track_caller]
pub fn verify_diagnostics_against_doc(
    diagnostics: &[Diagnostic],
    doc_diag: &[(u32, Vec<String>)]
) {
    // Build a map from line to set of diagnostic codes found in diagnostics
    let mut diags: HashMap<u32, Vec<&Diagnostic>> = HashMap::default();
    for diag in diagnostics {
        let line = diag.range.start.line;
        diags.entry(line).or_default().push(diag);
    }

    // Check expected codes and unexpected codes in a single loop
    for (line, expected_codes) in doc_diag {
        let found_codes = diags.get(line);
        assert!(found_codes.is_some(), "No diagnostics found on line {}. {} {} expected", line + 1, expected_codes.join(", "), if expected_codes.len() > 1 { "were" } else { "was" });

        let found_codes = found_codes.unwrap();
        // Check that all expected codes are present
        for code in expected_codes {
            assert!(
                found_codes.iter().any(|d| match &d.code {
                    Some(NumberOrString::String(c)) => c == code,
                    Some(NumberOrString::Number(n)) => &n.to_string() == code,
                    None => false,
                }),
                "Expected diagnostic code '{}' on line {}, but not found",
                code,
                line + 1
            );
        }

        // Check that no unexpected codes are present
        for code in found_codes.iter().map(|d| match &d.code {
            Some(NumberOrString::String(c)) => c.clone(),
            Some(NumberOrString::Number(n)) => n.to_string(),
            None => panic!("Diagnostic code is None"),
        }) {
            assert!(
                expected_codes.contains(&code),
                "Unexpected diagnostic code '{}' on line {}",
                code,
                line + 1
            );
        }
    }

    // Also check for diagnostics on lines not in doc_diag
    let expected_lines: HashSet<u32> = doc_diag.iter().map(|(l, _)| *l).collect();
    for line in diags.keys() {
        assert!(
            expected_lines.contains(line),
            "Unexpected diagnostics on line {}: {}",
            line + 1,
            diags.get(line).unwrap().iter().map(|d| match &d.code {
                Some(NumberOrString::String(c)) => S!("(") + c.as_str() + ") - " + d.message.as_str(),
                Some(NumberOrString::Number(n)) => S!("(") + n.to_string().as_str() + ") - " + d.message.as_str(),
                None => "None".to_string(),
            }).collect::<Vec<String>>().join(", "),
        );
    }
}

#[track_caller]
pub fn check_xml_diagnostic(diagnostics: &[Diagnostic], ols_code: &str, line: u32, severity: DiagnosticSeverity) {
    let line_diagnostics = diag_on_line(diagnostics, line);
    assert_eq!(
        line_diagnostics.len(), 1,
        "Expected exactly 1 diagnostic at line {line}, found {}: {:?}",
        line_diagnostics.len(),
        line_diagnostics
    );
    let diag = &line_diagnostics[0];
    let code = match &diag.code {
        Some(NumberOrString::String(code)) => code,
        Some(NumberOrString::Number(num)) => panic!("Unexpected numeric code: {num}"),
        None => panic!("Diagnostic code is None"),
    };
    assert_eq!(code, &S!(ols_code), "Expected code {ols_code} at line {line}, got {code}");
    assert!(diag.severity.is_some_and(|s| s == severity), "Expected severity {severity:?} for {ols_code} at line {line}");
}
