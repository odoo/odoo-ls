use std::{ffi::OsStr, path::Path};

use lsp_types::{Diagnostic, Position, Range};
use tracing::info;

use crate::{Sy, constants::{BuildSteps, DEBUG_STEPS, DiagnosticSource, OYarn}, core::{diagnostics::{DiagnosticCode, create_diagnostic}, file_mgr::FileInfo, symbols::{ModuleSymbol, symbol_keys::ModuleKey}}, threads::SessionInfo, utils::PathSanitizer};



impl ModuleSymbol {

    pub fn validate_manifest(module_key: ModuleKey, session: &mut SessionInfo){
        let module = &session.st()[module_key];
        let module_path = module.path.clone();
        if DEBUG_STEPS {
            info!("VALIDATION - MANIFEST: {}", module_path);
        }
        let data_paths = module.data.clone();
        let root_path = module.root_path.clone();
        let mut diagnostics = vec![];
        for (data_url, data_range) in data_paths.iter() {
            // validate csv file names, check that their models exist
            let path = Path::new(&module_path).join(data_url);
            if path.extension().unwrap_or_default() != "csv" || !path.exists(){
                continue;
            }
            let Some(model_name) = path.file_stem().and_then(OsStr::to_str).map(|n| Sy!(n.to_string())) else {
                continue;
            };
            let maybe_model = session.model_mgr().get_model(&model_name);
            let model_exists = maybe_model.map(|m| m.has_symbols(session.st())).unwrap_or(false);
            if !model_exists {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05056, &[&model_name]) {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(data_range.start().to_u32(), 0), Position::new(data_range.end().to_u32(), 0)),
                        ..diagnostic.clone()
                    });
                }
                session.st_mut()[module_key].not_found_models.insert(model_name.clone(), BuildSteps::VALIDATION);
                let main_entry = session.sync_odoo.get_main_entry();
                session.ep_mgr_mut()[main_entry].not_found_symbols_for_models.insert(module_key.into());
            }
        }
        let manifest_path = Path::new(&root_path).join("__manifest__.py");
        let manifest_file_info = session.file_mgr().get_file_info(&manifest_path.sanitize_cow()).expect("file not found in cache");
        session.file_mgr_mut()[manifest_file_info].replace_diagnostics(DiagnosticSource::PY_VALIDATION, diagnostics);
        FileInfo::publish_diagnostics(session, manifest_file_info);
    }
}
