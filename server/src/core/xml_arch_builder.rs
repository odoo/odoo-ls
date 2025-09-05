use std::{cell::RefCell, rc::Rc};

use lsp_types::Diagnostic;
use roxmltree::{Attribute, Node};
use tracing::warn;
use weak_table::PtrWeakHashSet;

use crate::core::{diagnostics::{create_diagnostic, DiagnosticCode}, odoo::SyncOdoo};
use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{entry_point::EntryPointType, xml_data::OdooData}, threads::SessionInfo, Sy};

use super::{file_mgr::FileInfo, symbols::{symbol::Symbol}};

/*
Struct made to load RelaxNG Odoo schemas and add hooks and specific OdooLS behavior on particular nodes.
*/
pub struct XmlArchBuilder {
    pub is_in_main_ep: bool,
    pub xml_symbol: Rc<RefCell<Symbol>>,
}

impl XmlArchBuilder {

    pub fn new(xml_symbol: Rc<RefCell<Symbol>>) -> Self {
        Self {
            is_in_main_ep: false,
            xml_symbol
        }
    }

    pub fn load_arch(&mut self, session: &mut SessionInfo, file_info: &mut FileInfo, node: &Node) {
        let mut diagnostics = vec![];
        self.xml_symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        let ep = self.xml_symbol.borrow().get_entry();
        if let Some(ep) = ep {
            self.is_in_main_ep = ep.borrow().typ == EntryPointType::MAIN || ep.borrow().typ == EntryPointType::ADDON;
        }
        self.load_odoo_openerp_data(session, node, &mut diagnostics);
        self.xml_symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::DONE);
        file_info.replace_diagnostics(BuildSteps::ARCH, diagnostics);
        session.sync_odoo.add_to_validations(self.xml_symbol.clone());
    }

    pub fn on_operation_creation(
        &self,
        session: &mut SessionInfo,
        id: Option<String>,
        node: &Node,
        mut xml_data: OdooData,
        diagnostics: &mut Vec<Diagnostic>
    ) {
        if !self.is_in_main_ep {
            return;
        }
        if let Some(id) = id {
            let module = self.xml_symbol.borrow().find_module();
            if module.is_none() {
                warn!("Module not found for id: {}", id);
                return;
            }
            let module = module.unwrap();
            let id_split = id.split(".").collect::<Vec<&str>>();
            if id_split.len() > 2 {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05051, &[&id]) {
                    diagnostics.push(lsp_types::Diagnostic {
                        range: lsp_types::Range {
                            start: lsp_types::Position::new(node.range().start as u32, 0),
                            end: lsp_types::Position::new(node.range().end as u32, 0),
                        },
                        ..diagnostic.clone()
                    });
                }
                return;
            }
            let id = id_split.last().unwrap().to_string();
            let mut xml_module = module.clone();
            if id_split.len() == 2 {
                let module_name = Sy!(id_split.first().unwrap().to_string());
                if let Some(m) = session.sync_odoo.modules.get(&module_name) {
                    xml_module = m.upgrade().unwrap();
                }
            }
            xml_data.set_file_symbol(&self.xml_symbol);
            xml_module.borrow_mut().as_module_package_mut().xml_id_locations.entry(Sy!(id.clone())).or_insert(PtrWeakHashSet::new()).insert(self.xml_symbol.clone());
            self.xml_symbol.borrow_mut().as_xml_file_sym_mut().xml_ids.entry(Sy!(id.clone())).or_insert(vec![]).push(xml_data);
        }
    }

    pub fn get_group_ids(&self, session: &mut SessionInfo, xml_id: &str, attr: &Attribute, diagnostics: &mut Vec<Diagnostic>) -> Vec<OdooData> {
        let xml_ids = SyncOdoo::get_xml_ids(session, &self.xml_symbol, xml_id, &attr.range(), diagnostics);
        let mut res = vec![];
        for data in xml_ids.iter() {
            match data {
                OdooData::RECORD(r) => {
                    if r.model.0 == "res.groups" {
                        res.push(data.clone());
                    }
                },
                _ => {}
            }
        }
        res
    }
}