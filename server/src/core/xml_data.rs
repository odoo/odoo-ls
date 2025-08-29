use std::{cell::RefCell, ops::Range, rc::{Rc, Weak}};

use ruff_text_size::TextRange;

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
            OdooData::RECORD(ref mut record) => {
                record.file_symbol = Rc::downgrade(xml_symbol);
            },
            OdooData::MENUITEM(ref mut menu_item) => {
                menu_item.file_symbol = Rc::downgrade(xml_symbol);
            },
            OdooData::TEMPLATE(ref mut template) => {
                template.file_symbol = Rc::downgrade(xml_symbol);
            },
            OdooData::DELETE(ref mut delete) => {
                delete.file_symbol = Rc::downgrade(xml_symbol);
            },
        }
    }

    pub fn get_range(&self) -> Range<usize> {
        match self {
            OdooData::RECORD(ref record) => record.range.clone(),
            OdooData::MENUITEM(ref menu_item) => menu_item.range.clone(),
            OdooData::TEMPLATE(ref template) => template.range.clone(),
            OdooData::DELETE(ref delete) => delete.range.clone(),
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
            OdooData::RECORD(ref record) => {
                Some(record.file_symbol.clone())
            },
            OdooData::MENUITEM(ref menu_item) => {
                Some(menu_item.file_symbol.clone())
            },
            OdooData::TEMPLATE(ref template) => {
                Some(template.file_symbol.clone())
            },
            OdooData::DELETE(ref delete) => {
                Some(delete.file_symbol.clone())
            }
        }
    }
}