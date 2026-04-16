use std::ops::Range;
use crate::{constants::OYarn, core::symbols::symbol_keys::{SourceFileKey, SymbolKey, Wk, XmlFileKey}};
use crate::core::symbols::storage::SymbolTable;

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
    pub symbol: Wk<SymbolKey>,
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
    pub file_symbol: Wk<XmlFileKey>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct XmlDataTemplate {
    pub file_symbol: Wk<XmlFileKey>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct XmlDataAsset {
    pub file_symbol: Wk<XmlFileKey>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct XmlDataDelete {
    pub file_symbol: Wk<XmlFileKey>,
    pub xml_id: Option<OYarn>,
    pub range: Range<usize>,
    pub model: OYarn,
}

impl OdooDataRecord {

    pub fn get_file_symbol(&self, symbol_table: &SymbolTable) -> Option<SourceFileKey> {
        let symbol = self.symbol.upgrade(symbol_table)?;
        symbol_table.get_file(symbol)
    }
}
impl OdooData {

    pub fn set_file_symbol(&mut self, xml_symbol: XmlFileKey) {
        let xml_weak: Wk<XmlFileKey> = xml_symbol.into();
        match self {
            OdooData::RECORD(record) => {
                record.symbol = xml_weak.map_into();
            },
            OdooData::MENUITEM(menu_item) => {
                menu_item.file_symbol = xml_weak;
            },
            OdooData::TEMPLATE(template) => {
                template.file_symbol = xml_weak;
            },
            OdooData::DELETE(delete) => {
                delete.file_symbol = xml_weak;
            },
            OdooData::ASSET(asset) => {
                asset.file_symbol = xml_weak;
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

    pub fn get_xml_id(&self) -> Option<OYarn> {
        match self {
            OdooData::RECORD(r) => r.xml_id.clone(),
            OdooData::MENUITEM(m) => m.xml_id.clone(),
            OdooData::TEMPLATE(t) => t.xml_id.clone(),
            OdooData::DELETE(d) => d.xml_id.clone(),
            OdooData::ASSET(a) => a.xml_id.clone(),
        }
    }

    pub fn get_xml_file_symbol(&self, symbol_table: &SymbolTable) -> Option<XmlFileKey> {
        let file_symbol = self.get_file_symbol(symbol_table)?;
        let symbol = file_symbol.upgrade(symbol_table)?;
        if let SourceFileKey::XmlFile(xml_file_key) = symbol {
            return Some(xml_file_key);
        }
        None
    }

    /* Warning: the returned symbol can of a different type than an XML_SYMBOL */
    pub fn get_symbol(&self) -> Wk<SymbolKey> {
        match self {
            OdooData::RECORD(record) => {
                record.symbol
            },
            OdooData::MENUITEM(menu_item) => {
                menu_item.file_symbol.map_into()
            },
            OdooData::TEMPLATE(template) => {
                template.file_symbol.map_into()
            },
            OdooData::DELETE(delete) => {
                delete.file_symbol.map_into()
            },
             OdooData::ASSET(asset) => {
                asset.file_symbol.map_into()
            }
        }
    }

    /* Warning: the returned symbol can of a different type than an XML_SYMBOL */
    pub fn get_file_symbol(&self, symbol_table: &SymbolTable) -> Option<Wk<SourceFileKey>> {
        match self {
            OdooData::RECORD(record) => {
                record.get_file_symbol(symbol_table).map(|file| file.into())
            }, 
            OdooData::MENUITEM(menu_item) => {
                Some(menu_item.file_symbol.map_into())
            },
            OdooData::TEMPLATE(template) => {
                Some(template.file_symbol.map_into())
            },
            OdooData::DELETE(delete) => {
                Some(delete.file_symbol.map_into())
            },
            OdooData::ASSET(asset) => {
                Some(asset.file_symbol.map_into())
            }
        }
    }
}
