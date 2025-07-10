
use lsp_types::{Diagnostic, DiagnosticSeverity};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::{constants::EXTENSION_NAME, core::file_mgr::NoqaInfo, S};

#[macro_export]
macro_rules! diagnostic_codes {
    (
        $(
            $(#[$meta:meta])* $name:ident , $msg:expr
        ),* $(,)?
    ) => {
        use serde::{Deserialize, Serialize};
        use super::{DiagnosticInfo, DiagnosticSetting};

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
        pub enum DiagnosticCode {
            $(
                $(#[$meta])* $name,
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

        pub static DIAGNOSTIC_INFOS: once_cell::sync::Lazy<std::collections::HashMap<DiagnosticCode, DiagnosticInfo>> = once_cell::sync::Lazy::new(|| std::collections::HashMap::from([
            $( (DiagnosticCode::$name, DiagnosticInfo { default_setting: $msg.0, template: $msg.1 }), )*
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

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
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
    let setting = session.sync_odoo.config.diagnostic_settings.get(&code).cloned()
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
