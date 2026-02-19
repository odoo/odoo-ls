use lsp_types::{GotoDefinitionResponse, LocationLink, Range};
use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};

use crate::constants::SymType;
use crate::core::file_mgr::{FileInfo, FileMgr};
use crate::core::symbols::symbol::Symbol;
use crate::features::goto_utils::{GotoRequest, GotoUtils};
use crate::features::xml_ast_utils::{XmlAstResult, XmlAstUtils};
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;

pub struct DefinitionFeature {}

impl DefinitionFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let definitions_sources = GotoUtils::get_symbols(session, GotoRequest::Definition, file_symbol, file_info, line, character);
        let mut links = vec![];
        for def in definitions_sources.iter() {
            links.extend(GotoUtils::goto_source_to_location(session, def));
        }
        Some(GotoDefinitionResponse::Link(links))
    }

    pub fn get_location_xml(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
        let document = roxmltree::Document::parse(&data);
        if let Ok(document) = document {
            let root = document.root_element();
            let (symbols, link_range) = XmlAstUtils::get_symbols(session, file_symbol, root, offset, true);
            if symbols.is_empty() {
                return None;
            }
            let mut links = vec![];
            for xml_result in symbols.iter() {
                match xml_result {
                    crate::features::xml_ast_utils::XmlAstResult::SYMBOL(s) => {
                        if let Some(file) = s.borrow().get_file() {
                            for path in file.upgrade().unwrap().borrow().paths().iter() {
                                let full_path = match file.upgrade().unwrap().borrow().typ() {
                                    SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", file.upgrade().unwrap().borrow().as_package().i_ext())).sanitize(),
                                    _ => path.clone()
                                };
                                let range = if s.borrow().has_range() {
                                    session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &full_path, &s.borrow().range())
                                } else {
                                    Range::default()
                                };
                                let link_range = if link_range.is_some() {
                                    Some(session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), link_range.as_ref().unwrap()))
                                } else {
                                    None
                                };
                                links.push(LocationLink{
                                    origin_selection_range: link_range,
                                    target_uri: FileMgr::pathname2uri(&full_path),
                                    target_range: range,
                                    target_selection_range: range
                                });
                            }
                        }
                    },
                    XmlAstResult::XML_DATA(xml_file_symbol, range) => {
                        let file = xml_file_symbol.borrow().get_file(); //in case of XML_DATA coming from a python class
                        if let Some(file) = file {
                            if let Some(file) = file.upgrade() {
                                for path in file.borrow().paths().iter() {
                                    let full_path = match file.borrow().typ() {
                                        SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", file.borrow().as_package().i_ext())).sanitize(),
                                        _ => path.clone()
                                    };
                                    let range = match file.borrow().typ() {
                                        SymType::PACKAGE(_) | SymType::FILE | SymType::NAMESPACE | SymType::DISK_DIR => Range::default(),
                                        _ => session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &full_path, &range),
                                    };
                                    let link_range = if link_range.is_some() {
                                        Some(session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), link_range.as_ref().unwrap()))
                                    } else {
                                        None
                                    };
                                    links.push(LocationLink{
                                        origin_selection_range: link_range,
                                        target_uri: FileMgr::pathname2uri(&full_path),
                                        target_range: range,
                                        target_selection_range: range
                                    });
                                }
                            }
                        }
                    }
                }
            }
            return Some(GotoDefinitionResponse::Link(links));
        }
        None
    }

    pub fn get_location_csv(_session: &mut SessionInfo,
        _file_symbol: &Rc<RefCell<Symbol>>,
        _file_info: &Rc<RefCell<FileInfo>>,
        _line: u32,
        _character: u32
    ) -> Option<GotoDefinitionResponse> {
        None
    }

}
