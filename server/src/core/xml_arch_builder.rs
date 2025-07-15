use std::{cell::RefCell, rc::Rc};

use lsp_types::{Diagnostic};
use roxmltree::Node;
use tracing::{warn};
use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{diagnostics::{create_diagnostic, DiagnosticCode}, entry_point::EntryPointType}, threads::SessionInfo, Sy};

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
    }

    pub fn on_operation_creation(
        &self,
        session: &mut SessionInfo,
        id: Option<String>,
        node: &Node,
        diagnostics: &mut Vec<Diagnostic>,
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
            let xml_module_bw = xml_module.borrow();
            let already_existing = xml_module_bw.as_module_package().xml_ids.get(&Sy!(id.clone())).cloned();
            drop(xml_module_bw);
            let mut found_one = false;
            if let Some(existing) = already_existing {
                //Check that it exists a main xml_id
                for s in existing.iter() {
                    if Rc::ptr_eq(&s, &xml_module) {
                        found_one = true;
                        break;
                    }
                }
            } else {
                xml_module.borrow_mut().as_module_package_mut().xml_ids.insert(Sy!(id.clone()), PtrWeakHashSet::new());
            }
            if !found_one && !Rc::ptr_eq(&xml_module, &module) {
                // no diagnostic to create.
            }
            xml_module.borrow_mut().as_module_package_mut().xml_ids.get_mut(&Sy!(id)).unwrap().insert(self.xml_symbol.clone());
        }
    }
}