//! Hooks for XML/CSV data file events.

use std::cell::RefCell;
use std::rc::Rc;

use crate::core::symbols::symbol::Symbol;
use crate::core::xml_data::OdooDataRecord;
use crate::threads::SessionInfo;
use once_cell::sync::Lazy;

// ============================================================================
// Record Creation Hooks
// ============================================================================

pub type RecordCreationHookFn =
    fn(session: &mut SessionInfo, source_file: &Rc<RefCell<Symbol>>, record: &OdooDataRecord);

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
            func: |session, source_file, record| {
                // Find the "code" field and extract its value
                let Some(code_field) = record.fields.iter().find(|f| f.name == "code") else {
                    return;
                };
                let Some(lang_code) = code_field.text.as_ref() else {
                    return;
                };
                session.sync_odoo.add_language(lang_code, source_file);
            },
        },
    ]
});

/// Dispatch to all matching hooks when a record is created.
/// Called from xml_arch_builder.rs and csv_arch_builder.rs.
pub fn on_record_creation(
    session: &mut SessionInfo,
    source_file: &Rc<RefCell<Symbol>>,
    record: &OdooDataRecord,
) {
    let model_name = record.model.0.as_str();
    for hook in record_creation_hooks.iter() {
        // Check if hook applies to this model
        if hook.models.iter().any(|m| *m == model_name) {
            (hook.func)(session, source_file, record);
        }
    }
}

// ============================================================================
// Data File Unload Hooks
// ============================================================================

pub type FileUnloadHookFn = fn(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>);

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
pub fn on_file_unload(session: &mut SessionInfo, file: &Rc<RefCell<Symbol>>) {
    for hook in file_unload_hooks.iter() {
        (hook.func)(session, file);
    }
}
