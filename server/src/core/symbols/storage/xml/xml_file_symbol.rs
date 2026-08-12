use lsp_types::Diagnostic;
use roxmltree::Error;

use crate::constants::MissingDataSource;
use crate::core::symbols::storage::dependency_mgr::{DependenciesTable, DependentsTable};
use crate::core::symbols::symbol_keys::{ModuleKey, XmlDataKey};
use crate::{core::diagnostics::DiagnosticCode, threads::SessionInfo};
use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::file_mgr::{FileInfo, NoqaInfo}, oyarn};
use crate::utils::HashSet;
use crate::utils::HashMap;

#[derive(Debug)]
pub struct XmlFileSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    pub (in crate::core::symbols::storage) current_build_step: BuildSteps,
    pub (in crate::core::symbols::storage) build_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_models: HashMap<OYarn, BuildSteps>,
    pub not_found_data_ids: HashMap<MissingDataSource, BuildSteps>,
    pub (in crate::core::symbols) in_workspace: bool,
    pub self_import: bool,
    pub dependencies: DependenciesTable,
    pub dependents: DependentsTable,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    parent: ModuleKey,
    pub(in crate::core::symbols::storage) data_symbols: HashSet<XmlDataKey>,
}

impl XmlFileSymbol {

    pub fn new(name: &str, path: &str, parent: ModuleKey, is_external: bool) -> Self {
        Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            is_external,
            parent,
            current_build_step: BuildSteps::ARCH,
            build_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            not_found_models: HashMap::default(),
            not_found_data_ids: HashMap::default(),
            data_symbols: HashSet::default(),
            in_workspace: false,
            self_import: false,
            dependencies: DependenciesTable::default(),
            dependents: DependentsTable::default(),
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        }
    }

    pub fn parent(&self) -> ModuleKey {
        self.parent
    }

    pub fn data_symbols(&self) -> &HashSet<XmlDataKey> {
        &self.data_symbols
    }

}

impl XmlFileSymbol {
    pub fn build_syntax_diagnostics(session: &SessionInfo, diagnostics: &mut Vec<Diagnostic>, file_info: &mut FileInfo, doc_error: &Error) {
        let offset = file_info.position_to_offset(doc_error.pos().row -1, doc_error.pos().col -1, session.sync_odoo.encoding);
        if let Some(diagnostic) = crate::core::diagnostics::create_diagnostic(session, DiagnosticCode::OLS05000, &[&doc_error.to_string()]) {
            diagnostics.push(lsp_types::Diagnostic {
                range: lsp_types::Range::new(lsp_types::Position::new(offset as u32, 0), lsp_types::Position::new(offset as u32 + 1, 0)),
                ..diagnostic.clone()
            });
        }
    }

}
