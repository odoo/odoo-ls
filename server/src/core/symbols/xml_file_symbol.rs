use lsp_types::Diagnostic;
use roxmltree::Error;
use weak_table::PtrWeakHashSet;

use crate::core::symbols::symbol_table::SymbolKey;
use crate::core::symbols::dependency_mgr::Buildable;
use crate::weak_hash_set::WeakSet;
use crate::{core::diagnostics::DiagnosticCode, threads::SessionInfo};
use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{file_mgr::{FileInfo, NoqaInfo}, model::Model, xml_data::OdooData}, oyarn};
use std::{cell::RefCell, collections::HashMap, rc::Weak};

#[derive(Debug)]
pub struct XmlFileSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    // pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<SymbolKey>,
    pub arch_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_models: HashMap<OYarn, BuildSteps>,
    pub xml_ids: HashMap<OYarn, Vec<OdooData>>,
    pub (super) in_workspace: bool,
    pub self_import: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: Vec<Vec<Option<WeakSet<SymbolKey>>>>,
    pub dependents: Vec<Vec<Option<WeakSet<SymbolKey>>>>,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    // @arena: these does not seem to be used anywhere
    // pub sections: Vec<SectionRange>,
    // pub symbols: HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    // pub ext_symbols: HashMap<OYarn, Vec<Rc<RefCell<Symbol>>>>,
}

impl XmlFileSymbol {

    // @arena: parent could be of type PackageKey 
    pub fn new(name: &str, path: &str, parent: SymbolKey, is_external: bool) -> Self {
        let res = Self {
            name: oyarn!("{}", name),
            path: path.to_string(),
            is_external,
            // weak_self: None,
            parent: Some(parent),
            arch_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            not_found_models: HashMap::new(),
            xml_ids: HashMap::new(),
            in_workspace: false,
            self_import: false,
            // sections: vec![],
            // symbols: HashMap::new(),
            // ext_symbols: HashMap::new(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: vec![],
            dependents: vec![],
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        };
        res
    }

    // @arena: dead code?
    // pub fn add_symbol(&mut self, content: &Rc<RefCell<Symbol>>, section: u32) {
    //     let sections = self.symbols.entry(content.borrow().name().clone()).or_insert_with(|| HashMap::new());
    //     let section_vec = sections.entry(section).or_insert_with(|| vec![]);
    //     section_vec.push(content.clone());
    // }

}

impl Buildable for XmlFileSymbol {
    fn build_status(&self, step: BuildSteps) -> BuildStatus {
        match step {
            BuildSteps::SYNTAX => panic!(),
            BuildSteps::ARCH => self.arch_status,
            BuildSteps::ARCH_EVAL => self.arch_status,
            BuildSteps::VALIDATION => self.validation_status,
        }
    }
    fn set_build_status(&mut self, step: BuildSteps, status: BuildStatus) {
        match step {
            BuildSteps::SYNTAX => panic!(),
            BuildSteps::ARCH => self.arch_status = status,
            BuildSteps::ARCH_EVAL => {},
            BuildSteps::VALIDATION => self.validation_status = status,
        }
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