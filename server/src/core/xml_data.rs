use std::{cell::RefCell, rc::Rc};

use crate::{constants::OYarn, core::symbols::symbol::Symbol};


#[derive(Debug, Clone)]
pub enum XmlData {
    RECORD(XmlDataRecord),
    MENUITEM(XmlDataMenuItem),
    TEMPLATE(XmlDataTemplate),
    DELETE(XmlDataDelete),
    ACT_WINDOW(XmlDataActWindow),
    REPORT(XmlDataReport),
}

#[derive(Debug, Clone)]
pub struct XmlDataRecord {
    pub xml_symbol: Rc<RefCell<Symbol>>,
    pub model: OYarn,
    pub xml_id: Option<OYarn>,
}

#[derive(Debug, Clone)]
pub struct XmlDataMenuItem {
    pub xml_symbol: Rc<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
}

#[derive(Debug, Clone)]
pub struct XmlDataTemplate {
    pub xml_symbol: Rc<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
}

#[derive(Debug, Clone)]
pub struct XmlDataDelete {
    pub xml_symbol: Rc<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
    pub model: OYarn,
}

#[derive(Debug, Clone)]
pub struct XmlDataActWindow {
    pub xml_symbol: Rc<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
    pub name: OYarn,
    pub res_model: OYarn,
}

#[derive(Debug, Clone)]
pub struct XmlDataReport {
    pub xml_symbol: Rc<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
    pub name: OYarn,
    pub model: OYarn,
    pub string: OYarn,
}

impl XmlData {

    pub fn set_symbol(&mut self, xml_symbol: Rc<RefCell<Symbol>>) {
        match self {
            XmlData::RECORD(ref mut record) => {
                record.xml_symbol = xml_symbol;
            },
            XmlData::MENUITEM(ref mut menu_item) => {
                menu_item.xml_symbol = xml_symbol;
            },
            XmlData::TEMPLATE(ref mut template) => {
                template.xml_symbol = xml_symbol;
            },
            XmlData::DELETE(ref mut delete) => {
                delete.xml_symbol = xml_symbol;
            },
            XmlData::ACT_WINDOW(ref mut act_window) => {
                act_window.xml_symbol = xml_symbol;
            },
            XmlData::REPORT(ref mut report) => {
                report.xml_symbol = xml_symbol;
            },
        }
    }

    pub fn get_symbol(&self) -> Option<Rc<RefCell<Symbol>>> {
        match self {
            XmlData::RECORD(ref record) => {
                Some(record.xml_symbol.clone())
            },
            XmlData::MENUITEM(ref menu_item) => {
                Some(menu_item.xml_symbol.clone())
            },
            XmlData::TEMPLATE(ref template) => {
                Some(template.xml_symbol.clone())
            },
            XmlData::DELETE(ref delete) => {
                Some(delete.xml_symbol.clone())
            },
            XmlData::ACT_WINDOW(ref act_window) => {
                Some(act_window.xml_symbol.clone())
            },
            XmlData::REPORT(ref report) => {
                Some(report.xml_symbol.clone())
            },
        }
    }
}