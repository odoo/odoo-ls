use std::{cell::RefCell, collections::HashMap, hash::Hash, path::PathBuf, rc::Rc};

use lsp_types::{Diagnostic, Position, Range};
use tracing::{info, trace};

use crate::{constants::{BuildSteps, SymType, DEBUG_STEPS}, core::{entry_point::{EntryPoint, EntryPointType}, file_mgr::FileInfo, odoo::SyncOdoo, symbols::symbol::Symbol, xml_data::{XmlData, XmlDataActWindow, XmlDataDelete, XmlDataMenuItem, XmlDataRecord, XmlDataReport, XmlDataTemplate}}, threads::SessionInfo};



pub struct XmlValidator {
    pub xml_symbol: Rc<RefCell<Symbol>>,
    pub is_in_main_ep: bool,
}

impl XmlValidator {

    pub fn new(entry: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>) -> Self {
        let is_in_main_ep = entry.borrow().typ == EntryPointType::MAIN || entry.borrow().typ == EntryPointType::ADDON;
        Self {
            xml_symbol: symbol,
            is_in_main_ep,
        }
    }

    fn get_file_info(&mut self, odoo: &mut SyncOdoo) -> Rc<RefCell<FileInfo>> {
        let file_symbol = self.xml_symbol.borrow();
        let mut path = file_symbol.paths()[0].clone();
        let file_info_rc = odoo.get_file_mgr().borrow().get_file_info(&path).expect("File not found in cache").clone();
        file_info_rc
    }

    pub fn validate(&mut self, session: &mut SessionInfo) {
        if DEBUG_STEPS {
            trace!("Validating XML File {}", self.xml_symbol.borrow().name());
        }
        let module = self.xml_symbol.borrow().find_module().unwrap();
        let mut dependencies = vec![];
        for xml_ids in self.xml_symbol.borrow().as_xml_file_sym().xml_ids.values() {
            for xml_id in xml_ids.iter() {
                self.validate_xml_id(session, &module, xml_id, &mut dependencies);
            }
        }
        for mut dep in dependencies.iter_mut() {
            self.xml_symbol.borrow_mut().add_dependency(&mut dep.borrow_mut(), BuildSteps::VALIDATION, BuildSteps::ARCH_EVAL);
        }
        let file_info = self.get_file_info(&mut session.sync_odoo);
        file_info.borrow_mut().publish_diagnostics(session);
    }

    pub fn validate_xml_id(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, data: &XmlData, dependencies: &mut Vec<Rc<RefCell<Symbol>>>) {
        let path = data.get_file_symbol().unwrap().upgrade().unwrap().borrow().paths()[0].clone();
        let mut file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&path).unwrap();
        let mut diagnostics = vec![];
        match data {
            XmlData::RECORD(xml_data_record) => self.validate_record(session, module, xml_data_record, &mut diagnostics, dependencies),
            XmlData::MENUITEM(xml_data_menu_item) => self.validate_menu_item(session, module, xml_data_menu_item, &mut diagnostics, dependencies),
            XmlData::TEMPLATE(xml_data_template) => self.validate_template(session, module, xml_data_template, &mut diagnostics, dependencies),
            XmlData::DELETE(xml_data_delete) => self.validate_delete(session, module, xml_data_delete, &mut diagnostics, dependencies),
            XmlData::ACT_WINDOW(xml_data_act_window) => self.validate_act_window(session, module, xml_data_act_window, &mut diagnostics, dependencies),
            XmlData::REPORT(xml_data_report) => self.validate_report(session, module, xml_data_report, &mut diagnostics, dependencies),
        }
        file_info.borrow_mut().update_validation_diagnostics(HashMap::from([(BuildSteps::VALIDATION, diagnostics)]));
    }

    fn validate_record(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_record: &XmlDataRecord, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>) {
        let mut model_ok = false;
        let model = session.sync_odoo.models.get(&xml_data_record.model.0).cloned();
        if let Some(model) = model {
            self.xml_symbol.borrow_mut().add_model_dependencies(&model);
            for main_sym in model.borrow().get_main_symbols(session, Some(module.clone())).iter() {
                model_ok = true;
                dependencies.push(main_sym.borrow().get_file().unwrap().upgrade().unwrap());
            }
        } else {
            //TODO register to not_found_models
        }
        if !model_ok {
            diagnostics.push(Diagnostic {
                range: Range::new(Position::new(xml_data_record.model.1.start.try_into().unwrap(), 0), Position::new(xml_data_record.model.1.end.try_into().unwrap(), 0)),
                severity: Some(lsp_types::DiagnosticSeverity::ERROR),
                message: format!("Model '{}' not found in module '{}'", xml_data_record.model.0, module.borrow().name()),
                source: Some("OdooLS".to_string()),
                ..Default::default()
            });
            info!("Model '{}' not found in module '{}'", xml_data_record.model.0, module.borrow().name());
        }
    }

    fn validate_menu_item(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_menu_item: &XmlDataMenuItem, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>) {
        
    }

    fn validate_template(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_template: &XmlDataTemplate, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>) {
        
    }

    fn validate_delete(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_delete: &XmlDataDelete, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>) {
        
    }

    fn validate_act_window(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_act_window: &XmlDataActWindow, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>) {
        
    }

    fn validate_report(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_report: &XmlDataReport, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>) {
        
    }
}