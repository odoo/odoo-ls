use std::{cell::RefCell, ops::Range, rc::{Rc, Weak}};

use ruff_text_size::TextRange;

use crate::{constants::{OYarn, SymType}, core::symbols::symbol::Symbol};


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
    pub file_symbol: Weak<RefCell<Symbol>>,
    pub model: (OYarn, Range<usize>),
    pub xml_id: Option<OYarn>,
}

#[derive(Debug, Clone)]
pub struct XmlDataMenuItem {
    pub file_symbol: Weak<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
}

#[derive(Debug, Clone)]
pub struct XmlDataTemplate {
    pub file_symbol: Weak<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
}

#[derive(Debug, Clone)]
pub struct XmlDataDelete {
    pub file_symbol: Weak<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
    pub model: OYarn,
}

#[derive(Debug, Clone)]
pub struct XmlDataActWindow {
    pub file_symbol: Weak<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
    pub name: OYarn,
    pub res_model: OYarn,
}

#[derive(Debug, Clone)]
pub struct XmlDataReport {
    pub file_symbol: Weak<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
    pub name: OYarn,
    pub model: OYarn,
    pub string: OYarn,
}

impl XmlData {

    pub fn set_file_symbol(&mut self, xml_symbol: &Rc<RefCell<Symbol>>) {
        match self {
            XmlData::RECORD(ref mut record) => {
                record.file_symbol = Rc::downgrade(xml_symbol);
            },
            XmlData::MENUITEM(ref mut menu_item) => {
                menu_item.file_symbol = Rc::downgrade(xml_symbol);
            },
            XmlData::TEMPLATE(ref mut template) => {
                template.file_symbol = Rc::downgrade(xml_symbol);
            },
            XmlData::DELETE(ref mut delete) => {
                delete.file_symbol = Rc::downgrade(xml_symbol);
            },
            XmlData::ACT_WINDOW(ref mut act_window) => {
                act_window.file_symbol = Rc::downgrade(xml_symbol);
            },
            XmlData::REPORT(ref mut report) => {
                report.file_symbol = Rc::downgrade(xml_symbol);
            },
        }
    }

    pub fn get_xml_file_symbol(&self) -> Option<Rc<RefCell<Symbol>>> {
        let file_symbol = self.get_file_symbol()?;
        if let Some(symbol) = file_symbol.upgrade() {
            if symbol.borrow().typ() == SymType::XML_FILE {
                return Some(symbol);
            }
        }
        None
    }

    /* Warning: the returned symbol can of a different type than an XML_SYMBOL */
    pub fn get_file_symbol(&self) -> Option<Weak<RefCell<Symbol>>> {
        match self {
            XmlData::RECORD(ref record) => {
                Some(record.file_symbol.clone())
            },
            XmlData::MENUITEM(ref menu_item) => {
                Some(menu_item.file_symbol.clone())
            },
            XmlData::TEMPLATE(ref template) => {
                Some(template.file_symbol.clone())
            },
            XmlData::DELETE(ref delete) => {
                Some(delete.file_symbol.clone())
            },
            XmlData::ACT_WINDOW(ref act_window) => {
                Some(act_window.file_symbol.clone())
            },
            XmlData::REPORT(ref report) => {
                Some(report.file_symbol.clone())
            },
        }
    }
}