use oxc::diagnostics::OxcDiagnostic;

use crate::{S, constants::EXTENSION_NAME};

pub fn oxc_diagnostic_to_lsp_diagnostic(diag: &OxcDiagnostic, uri: &lsp_types::Uri) -> Option<lsp_types::Diagnostic> {
    let labels = diag.labels.as_ref()?;
    let first_label = labels.first().unwrap();
    let range = lsp_types::Range {
        start: lsp_types::Position { line: first_label.offset() as u32, character: 0 },
        end: lsp_types::Position { line: first_label.offset() as u32 + first_label.len() as u32, character: 0 },
    };
    let related_information = labels.iter().skip(1)
        .filter_map(|label| {
            let msg = label.label()?.to_string();
            let start = label.offset();
            let end   = start + label.len();
            Some(lsp_types::DiagnosticRelatedInformation {
                location: lsp_types::Location {
                    uri: uri.clone(),
                    range: lsp_types::Range {
                        start: lsp_types::Position { line: start as u32, character: 0 },
                        end:   lsp_types::Position { line: end   as u32, character: 0 },
                    },
                },
                message: msg,
            })
        })
        .collect::<Vec<_>>();
    let message = match first_label.label() {
        Some(label) => format!("{} - {}", diag.message.clone(), label),
        None => diag.message.clone().to_string(),
    };
    Some(lsp_types::Diagnostic {
        range: range,
        severity: Some(match diag.severity {
            oxc::diagnostics::Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
            oxc::diagnostics::Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            oxc::diagnostics::Severity::Advice => lsp_types::DiagnosticSeverity::INFORMATION,
        }),
        code: Some(lsp_types::NumberOrString::String(format!("OLSoxc{}", diag.code))),
        code_description: None,
        source: Some(S!(EXTENSION_NAME)),
        message: message,
        related_information: if related_information.is_empty() { None } else { Some(related_information) },
        tags: None,
        data: None,
    })
}