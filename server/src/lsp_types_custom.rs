use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ConfigDiagnosticMessage {
    pub level: ConfigDiagnosticMessageLevel,
    pub message: String,
}

pub enum ConfigDiagnosticAction {
    REPLACE, // Replace all existing diagnostics with the new ones
    EXTEND, // Extend the existing diagnostics with the new ones (add new diagnostics, keep existing ones)
}

impl ConfigDiagnosticAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigDiagnosticAction::REPLACE => "replace",
            ConfigDiagnosticAction::EXTEND => "extend",
        }
    }
}

#[derive(Debug)]
pub enum ConfigDiagnosticMessageLevel {
    INFO = 0,
    WARNING = 1,
    ERROR = 2,
}

impl Serialize for ConfigDiagnosticMessageLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(match self {
            ConfigDiagnosticMessageLevel::INFO => 0,
            ConfigDiagnosticMessageLevel::WARNING => 1,
            ConfigDiagnosticMessageLevel::ERROR => 2,
        })
    }
}