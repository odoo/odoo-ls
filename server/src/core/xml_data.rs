use std::{cell::RefCell, ops::Range, rc::{Rc, Weak}};

use crate::{constants::{OYarn, SymType}, core::symbols::symbol::Symbol};


#[derive(Debug, Clone)]
pub enum OdooData {
    RECORD(OdooDataRecord),
    MENUITEM(XmlDataMenuItem),
    TEMPLATE(XmlDataTemplate),
    DELETE(XmlDataDelete),
}

#[derive(Debug, Clone)]
pub struct OdooDataRecord {
    pub file_symbol: Weak<RefCell<Symbol>>,
    pub model: (OYarn, Range<usize>),
    pub xml_id: Option<OYarn>,
    pub fields: Vec<OdooDataField>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct OdooDataField {
    pub name: OYarn,
    pub range: Range<usize>,
    pub text: Option<String>,
    pub ref_key: Option<(String, Range<usize>)>,
    pub text_range: Option<Range<usize>>,
}

#[derive(Debug, Clone)]
pub struct XmlDataMenuItem {
    pub file_symbol: Weak<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct XmlDataTemplate {
    pub file_symbol: Weak<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct XmlDataDelete {
    pub file_symbol: Weak<RefCell<Symbol>>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
    pub model: OYarn,
}

impl OdooData {

    pub fn set_file_symbol(&mut self, xml_symbol: &Rc<RefCell<Symbol>>) {
        match self {
            OdooData::RECORD(record) => {
                record.file_symbol = Rc::downgrade(xml_symbol);
            },
            OdooData::MENUITEM(menu_item) => {
                menu_item.file_symbol = Rc::downgrade(xml_symbol);
            },
            OdooData::TEMPLATE(template) => {
                template.file_symbol = Rc::downgrade(xml_symbol);
            },
            OdooData::DELETE(delete) => {
                delete.file_symbol = Rc::downgrade(xml_symbol);
            },
        }
    }

    pub fn get_range(&self) -> Range<usize> {
        match self {
            OdooData::RECORD(record) => record.range.clone(),
            OdooData::MENUITEM(menu_item) => menu_item.range.clone(),
            OdooData::TEMPLATE(template) => template.range.clone(),
            OdooData::DELETE(delete) => delete.range.clone(),
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
            OdooData::RECORD(record) => {
                Some(record.file_symbol.clone())
            },
            OdooData::MENUITEM(menu_item) => {
                Some(menu_item.file_symbol.clone())
            },
            OdooData::TEMPLATE(template) => {
                Some(template.file_symbol.clone())
            },
            OdooData::DELETE(delete) => {
                Some(delete.file_symbol.clone())
            }
        }
    }
}