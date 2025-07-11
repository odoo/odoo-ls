use std::{cell::RefCell, collections::HashMap, hash::Hash, rc::Rc};

use lsp_types::{Diagnostic, Position, Range};
use tracing::info;

use crate::{constants::BuildSteps, core::{entry_point::{EntryPoint, EntryPointType}, symbols::symbol::Symbol, xml_data::{XmlData, XmlDataActWindow, XmlDataDelete, XmlDataMenuItem, XmlDataRecord, XmlDataReport, XmlDataTemplate}}, threads::SessionInfo};



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

    pub fn validate(&mut self, session: &mut SessionInfo) {
        let module = self.xml_symbol.borrow().find_module().unwrap();
        for xml_ids in self.xml_symbol.borrow().as_xml_file_sym().xml_ids.values() {
            for xml_id in xml_ids.iter() {
                self.validate_xml_id(session, &module, xml_id);
            }
        }
    }

    pub fn validate_xml_id(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, data: &XmlData) {
        let path = data.get_file_symbol().unwrap().upgrade().unwrap().borrow().paths()[0].clone();
        let mut file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&path).unwrap();
        let mut diagnostics = vec![];
        match data {
            XmlData::RECORD(xml_data_record) => self.validate_record(session, module, xml_data_record, &mut diagnostics),
            XmlData::MENUITEM(xml_data_menu_item) => self.validate_menu_item(session, module, xml_data_menu_item, &mut diagnostics),
            XmlData::TEMPLATE(xml_data_template) => self.validate_template(session, module, xml_data_template, &mut diagnostics),
            XmlData::DELETE(xml_data_delete) => self.validate_delete(session, module, xml_data_delete, &mut diagnostics),
            XmlData::ACT_WINDOW(xml_data_act_window) => self.validate_act_window(session, module, xml_data_act_window, &mut diagnostics),
            XmlData::REPORT(xml_data_report) => self.validate_report(session, module, xml_data_report, &mut diagnostics),
        }
        file_info.borrow_mut().update_validation_diagnostics(HashMap::from([(BuildSteps::VALIDATION, diagnostics)]));
    }

    fn validate_record(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_record: &XmlDataRecord, diagnostics: &mut Vec<Diagnostic>) {
        let mut model_ok = false;
        let model = session.sync_odoo.models.get(&xml_data_record.model.0).cloned();
        if let Some(model) = model {
            if !model.borrow().get_main_symbols(session, Some(module.clone())).is_empty() {
                model_ok = true;
            }
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

    fn validate_menu_item(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_menu_item: &XmlDataMenuItem, diagnostics: &mut Vec<Diagnostic>) {
        
    }

    fn validate_template(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_template: &XmlDataTemplate, diagnostics: &mut Vec<Diagnostic>) {
        
    }

    fn validate_delete(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_delete: &XmlDataDelete, diagnostics: &mut Vec<Diagnostic>) {
        
    }

    fn validate_act_window(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_act_window: &XmlDataActWindow, diagnostics: &mut Vec<Diagnostic>) {
        
    }

    fn validate_report(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_report: &XmlDataReport, diagnostics: &mut Vec<Diagnostic>) {
        
    }
}