
use lsp_types::{Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag, Location, NumberOrString, Range, Uri};
use ruff_text_size::TextRange;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::{constants::EXTENSION_NAME, core::file_mgr::NoqaInfo, S};

/// A simplified version of lsp_types::Diagnostic, with important differences:
/// - range is stored as byte offsets (so, encoding-agnostic)
/// - related information applies to same file only
#[derive(Debug, Clone)]
pub struct Diag {
    /// byte offsets
    range: TextRange,
    severity: DiagnosticSeverity,
    code: String,
    message: String,
    related_info: Vec<RelatedInfo>,
    tags: Vec<DiagnosticTag>,
}

/// Equivalent to lsp_types::DiagnosticRelatedInformation, but:
/// - applies to same file only (so, no uri)
/// - range is a byte offset
#[derive(Debug, Clone)]
pub struct RelatedInfo {
    range: TextRange,
    message: String,
}

impl Diag {
    pub fn into_lsp(self, uri: &Uri, to_lsp_range: impl Fn(TextRange) -> Range) -> Diagnostic {
        let Self { range, severity, code, message, related_info, tags } = self;
        Diagnostic {
            range: to_lsp_range(range),
            severity: Some(severity),
            code: Some(NumberOrString::String(code)),
            source: Some(EXTENSION_NAME.to_string()),
            message,
            related_information: if related_info.is_empty() {
                None
            } else {
                Some(
                    related_info.into_iter()
                        .map(|r| r.into_lsp(uri, &to_lsp_range))
                        .collect()
                )
            },
            tags: if tags.is_empty() { None } else { Some(tags) },
            code_description: None,
            data: None,
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn add_tag(&mut self, tag: DiagnosticTag) {
        self.tags.push(tag);
    }

    pub fn set_range(&mut self, range: TextRange) {
        self.range = range;
    }
}

impl RelatedInfo {
    pub fn into_lsp(self, uri: &Uri, to_lsp_range: impl Fn(TextRange) -> Range) -> DiagnosticRelatedInformation {
        let Self { range, message } = self;
        DiagnosticRelatedInformation {
            location: Location {
                uri: uri.clone(),
                range: to_lsp_range(range),
            },
            message, 
        }
    }
}

#[macro_export]
macro_rules! diagnostic_codes {
    (
        $(
            $(#[$meta:meta])* $name:ident , $default_setting:expr , $msg:expr
        ),* $(,)?
    ) => {
        use serde::{Deserialize, Serialize};
        use schemars::JsonSchema;
        use super::{DiagnosticInfo, DiagnosticSetting};

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, JsonSchema)]
        pub enum DiagnosticCode {
            $(
                #[doc = $msg] $(#[$meta])* $name,
            )*
        }

        #[derive(JsonSchema)]
        #[allow(non_snake_case)]
        #[schemars(deny_unknown_fields)]
        pub struct SchemaDiagnosticCodes {
            $(
                #[doc = $msg] pub $name: Option<DiagnosticSetting>,
            )*
        }

        impl std::str::FromStr for DiagnosticCode {
            type Err = ();
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $( stringify!($name) => Ok(DiagnosticCode::$name), )*
                    _ => Err(())
                }
            }
        }

        impl std::fmt::Display for DiagnosticCode {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $( DiagnosticCode::$name => write!(f, stringify!($name)), )*
                }
            }
        }

        pub static DIAGNOSTIC_INFOS: std::sync::LazyLock<$crate::utils::HashMap<DiagnosticCode, DiagnosticInfo>> = std::sync::LazyLock::new(|| $crate::utils::HashMap::from_iter([
            $( (DiagnosticCode::$name, DiagnosticInfo { default_setting: $default_setting, template: $msg }), )*
        ]));
    }
}

// Import the code list from a separate file
#[path = "diagnostic_codes_list.rs"]
mod diagnostic_codes_list;
pub use diagnostic_codes_list::*;


pub static DEFAULT_DIAGNOSTIC: LazyLock<Diagnostic> = LazyLock::new(|| Diagnostic {
    source: Some(S!(EXTENSION_NAME)),
    ..Default::default()
});

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub enum DiagnosticSetting {
    Error,
    Warning,
    Info,
    Hint,
    Disabled
}

/// Central info for each diagnostic code
pub struct DiagnosticInfo {
    pub default_setting: DiagnosticSetting,
    pub template: &'static str,
}

use crate::threads::SessionInfo;

/// Get the current severity for a code (default or overridden) using the session's config
pub fn get_severity(
    code: DiagnosticCode,
    session: &SessionInfo,
) -> Option<DiagnosticSeverity> {
    let setting = session.sync_odoo.config.diagnostic_settings().get(&code).cloned()
        .unwrap_or(DIAGNOSTIC_INFOS[&code].default_setting);
    match setting {
        DiagnosticSetting::Error => Some(DiagnosticSeverity::ERROR),
        DiagnosticSetting::Warning => Some(DiagnosticSeverity::WARNING),
        DiagnosticSetting::Info => Some(DiagnosticSeverity::INFORMATION),
        DiagnosticSetting::Hint => Some(DiagnosticSeverity::HINT),
        DiagnosticSetting::Disabled => None,
    }
}

/// Format the message for a code with named parameters
pub fn format_message(code: DiagnosticCode, params:&[&str]) -> String {
    let template = DIAGNOSTIC_INFOS[&code].template;
    let mut msg = template.to_string();
    for (i, value) in params.iter().enumerate() {
        let placeholder = format!("{{{}}}", i);
        msg = msg.replace(&placeholder, value);
    }
    msg
}

/// Create a diagnostic, using the session's config for overrides
/// returns None if the diagnostic is disabled by the session's noqa config or user config
pub fn create_diagnostic(
    session: &SessionInfo,
    code: DiagnosticCode,
    params: &[&str],
) -> Option<Diagnostic> {
    match &session.current_noqa {
        NoqaInfo::All => return None,
        NoqaInfo::Codes(codes) => if codes.contains(&code.to_string()) {return None},
        NoqaInfo::None => {}
    }
    let severity = match get_severity(code, session) {
        None => return None,
        severity => severity,
    };
    Some(Diagnostic {
        severity,
        message: format_message(code, params),
        code: Some(lsp_types::NumberOrString::String(code.to_string())),
        ..DEFAULT_DIAGNOSTIC.clone()
    })
}

pub fn create_diag(
    session: &SessionInfo,
    code: DiagnosticCode,
    params: &[&str],
    range: TextRange,
) -> Option<Diag> {
    let code_string = code.to_string();
    match &session.current_noqa {
        NoqaInfo::All => return None,
        NoqaInfo::Codes(codes) => if codes.contains(&code_string) {return None},
        NoqaInfo::None => {}
    }
    let severity = get_severity(code, session)?;
    Some(Diag {
        range,
        severity,
        message: format_message(code, params),
        code: code_string,
        related_info: Vec::new(),
        tags: Vec::new(),
    })
}

// temp
impl From<Diagnostic> for Diag {
    fn from(d: Diagnostic) -> Self {
        Self {
            range: TextRange::new(d.range.start.line.into(), d.range.end.line.into()),
            severity: d.severity.unwrap(),
            message: d.message,
            code: match d.code {
                Some(NumberOrString::String(s)) => s,
                _ => String::default(),
            },
            tags: d.tags.unwrap_or_default(),
            related_info: d.related_information.unwrap_or_default().into_iter()
                .map(|r| RelatedInfo {
                    range: TextRange::new(r.location.range.start.line.into(), r.location.range.end.line.into()),
                    message: r.message,
                })
                .collect(),
        }
    }
}
