//! Hooks for XML/CSV data file events.

use crate::core::symbols::symbol_keys::{SourceFileKey, XmlRecordKey};
use crate::threads::SessionInfo;
use once_cell::sync::Lazy;

// ============================================================================
// Record Creation Hooks
// ============================================================================

pub type RecordCreationHookFn =
    fn(session: &mut SessionInfo, source_file: SourceFileKey, record: XmlRecordKey);

/// A hook that triggers when records of specific models are created.
pub struct RecordCreationHook {
    /// Model names this hook applies to (e.g., ["res.lang", "res.country"])
    pub models: Vec<&'static str>,
    pub func: RecordCreationHookFn,
}

/// Registry of all record creation hooks
#[allow(non_upper_case_globals)]
static record_creation_hooks: Lazy<Vec<RecordCreationHook>> = Lazy::new(|| {
    vec![
        // Hook: Track res.lang records for language validation
        RecordCreationHook {
            models: vec!["res.lang"],
            func: |session, source_file, record_key| {
                // Collect valid "code" field key
                let Some(&field_key) = session.st()[record_key].fields().get("code") else {
                    return;
                };
                let Some(text) = session.st()[field_key].text.clone() else {
                    return;
                };
                session.sync_odoo.add_language(&text, source_file);
            },
        },
    ]
});

/// Dispatch to all matching hooks when a record is created.
/// Called from xml_arch_builder.rs and csv_arch_builder.rs.
pub fn on_record_creation(
    session: &mut SessionInfo,
    source_file: SourceFileKey,
    record: XmlRecordKey,
) {
    let model_name = session.st()[record].model.0.clone();
    for hook in record_creation_hooks.iter() {
        // Check if hook applies to this model
        if hook.models.iter().any(|m| model_name == *m) {
            (hook.func)(session, source_file, record);
        }
    }
}

// ============================================================================
// Data File Unload Hooks
// ============================================================================

pub type FileUnloadHookFn = fn(session: &mut SessionInfo, file: SourceFileKey);

/// A hook that triggers when a data file (XML/CSV) symbol is unloaded.
pub struct FileUnloadHook {
    pub func: FileUnloadHookFn,
}

/// Registry of file unload hooks
#[allow(non_upper_case_globals)]
static file_unload_hooks: Lazy<Vec<FileUnloadHook>> = Lazy::new(|| {
    vec![FileUnloadHook {
        // Hook: Remove language codes when data file is unloaded
        func: |session, file| {
            session.sync_odoo.remove_language_source(file);
        },
    }]
});

/// Dispatch to all hooks when data file (XML/CSV) is unloaded
pub fn on_file_unload(session: &mut SessionInfo, file: SourceFileKey) {
    for hook in file_unload_hooks.iter() {
        (hook.func)(session, file);
    }
}
