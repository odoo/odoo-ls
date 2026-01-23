use std::ops::Range;
use crate::{constants::OYarn, core::symbols::symbol_keys::{SymbolKey, Weak, XmlFileKey}};
use crate::core::symbols::symbol_table::SymbolTable;

#[derive(Debug, Clone)]
pub enum OdooData {
    RECORD(OdooDataRecord),
    MENUITEM(XmlDataMenuItem),
    TEMPLATE(XmlDataTemplate),
    ASSET(XmlDataAsset),
    DELETE(XmlDataDelete),
}

#[derive(Debug, Clone)]
pub struct OdooDataRecord {
    pub file_symbol: Weak<SymbolKey>,
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
    pub file_symbol: Weak<SymbolKey>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct XmlDataTemplate {
    pub file_symbol: Weak<SymbolKey>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct XmlDataAsset {
    pub file_symbol: Weak<SymbolKey>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct XmlDataDelete {
    pub file_symbol: Weak<SymbolKey>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
    pub model: OYarn,
}

impl OdooData {

    pub fn set_file_symbol(&mut self, xml_symbol: XmlFileKey) {
        let file_symbol: Weak<SymbolKey> = xml_symbol.into();
        match self {
            OdooData::RECORD(record) => {
                record.file_symbol = file_symbol;
            },
            OdooData::MENUITEM(menu_item) => {
                menu_item.file_symbol = file_symbol;
            },
            OdooData::TEMPLATE(template) => {
                template.file_symbol = file_symbol;
            },
            OdooData::DELETE(delete) => {
                delete.file_symbol = file_symbol;
            },
            OdooData::ASSET(asset) => {
                asset.file_symbol = file_symbol;
            }
        }
    }

    pub fn get_range(&self) -> Range<usize> {
        match self {
            OdooData::RECORD(record) => record.range.clone(),
            OdooData::MENUITEM(menu_item) => menu_item.range.clone(),
            OdooData::TEMPLATE(template) => template.range.clone(),
            OdooData::DELETE(delete) => delete.range.clone(),
            OdooData::ASSET(asset) => asset.range.clone(),
        }
    }

    pub fn get_xml_file_symbol(&self, symbol_table: &SymbolTable) -> Option<XmlFileKey> {
        let file_symbol = self.get_file_symbol()?;
        let symbol = file_symbol.upgrade(symbol_table)?;
        if let SymbolKey::XmlFile(xml_file_key) = symbol {
            return Some(xml_file_key);
        }
        None
    }

    /* Warning: the returned symbol can of a different type than an XML_SYMBOL */
    pub fn get_file_symbol(&self) -> Option<Weak<SymbolKey>> {
        match self {
            OdooData::RECORD(record) => {
                Some(record.file_symbol)
            },
            OdooData::MENUITEM(menu_item) => {
                Some(menu_item.file_symbol)
            },
            OdooData::TEMPLATE(template) => {
                Some(template.file_symbol)
            },
            OdooData::DELETE(delete) => {
                Some(delete.file_symbol)
            },
            OdooData::ASSET(asset) => {
                Some(asset.file_symbol)
            }
        }
    }
}
