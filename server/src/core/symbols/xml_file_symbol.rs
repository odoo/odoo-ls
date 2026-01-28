use lsp_types::Diagnostic;
use roxmltree::Error;
use weak_table::PtrWeakHashSet;

use crate::{core::diagnostics::DiagnosticCode, threads::SessionInfo};
use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{file_mgr::{FileInfo, NoqaInfo}, model::Model, xml_data::OdooData}, oyarn};
use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use super::{symbol::Symbol, symbol_mgr::SectionRange};

#[derive(Debug)]
pub struct XmlFileSymbol {
    pub name: OYarn,
    pub path: String,
    pub is_external: bool,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub arch_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_models: HashMap<OYarn, BuildSteps>,
    pub xml_ids: HashMap<OYarn, Vec<OdooData>>,
    in_workspace: bool,
    pub self_import: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>,
    pub dependents: Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<OYarn, Vec<Rc<RefCell<Symbol>>>>,
}

impl XmlFileSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        let res = Self {
            name: oyarn!("{}", name),
            path,
            is_external,
            weak_self: None,
            parent: None,
            arch_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            not_found_models: HashMap::new(),
            xml_ids: HashMap::new(),
            in_workspace: false,
            self_import: false,
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: vec![],
            dependents: vec![],
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        };
        res
    }

    pub fn add_symbol(&mut self, content: &Rc<RefCell<Symbol>>, section: u32) {
        let sections = self.symbols.entry(content.borrow().name().clone()).or_insert_with(|| HashMap::new());
        let section_vec = sections.entry(section).or_insert_with(|| vec![]);
        section_vec.push(content.clone());
    }

    pub fn get_dependencies(&self, step: usize, level: usize) -> Option<&PtrWeakHashSet<Weak<RefCell<Symbol>>>>
    {
        self.dependencies.get(step)?.get(level)?.as_ref()
    }

    pub fn get_all_dependencies(&self, step: usize) -> Option<&Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>
    {
        self.dependencies.get(step)
    }

    pub fn get_dependents(&self, level: usize, step: usize) -> Option<&PtrWeakHashSet<Weak<RefCell<Symbol>>>>
    {
        self.dependents.get(level)?.get(step)?.as_ref()
    }

    pub fn get_all_dependents(&self, level: usize) -> Option<&Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>
    {
        self.dependents.get(level)
    }

    pub fn dependencies(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &self.dependencies
    }

    pub fn dependencies_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &mut self.dependencies
    }

    pub fn set_in_workspace(&mut self, in_workspace: bool) {
        self.in_workspace = in_workspace;
        if in_workspace {
            self.dependencies= vec![
                vec![ //ARCH
                    None //ARCH
                ],
                vec![ //ARCH_EVAL
                    None, //ARCH,
                    None, //ARCH_EVAL
                ],
                vec![
                    None, // ARCH
                    None, //ARCH_EVAL
                    None, //VALIDATIOn
                ]
            ];
            self.dependents = vec![
                vec![ //ARCH
                    None, //ARCH
                    None, //ARCH_EVAL
                    None, //VALIDATION
                ],
                vec![ //ARCH_EVAL
                    None, //ARCH_EVAL
                    None //VALIDATION
                ],
                vec![ //VALIDATION
                    None //VALIDATION
                ]
            ];
        }
    }

    pub fn dependents(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &self.dependents
    }

    pub fn dependents_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &mut self.dependents
    }

    pub fn is_in_workspace(&self) -> bool {
        self.in_workspace
    }

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