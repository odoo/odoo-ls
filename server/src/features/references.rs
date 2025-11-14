use std::{cell::RefCell, path::PathBuf, rc::Rc};

use lsp_types::{Location, Range};

use crate::{constants::SymType, core::{file_mgr::{FileInfo, FileMgr}, symbols::symbol::Symbol}, features::xml_ast_utils::{XmlAstResult, XmlAstUtils}, threads::SessionInfo, utils::PathSanitizer};



pub struct ReferenceFeature {

}

impl ReferenceFeature {
    pub fn get_references(_session: &mut SessionInfo, _file_symbol: &Rc<RefCell<Symbol>>, _file_info: &Rc<RefCell<FileInfo>>, _line: u32, _character: u32) -> Option<Vec<Location>> {
        // Implementation for getting references
        None
    }

    pub fn get_references_xml(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Vec<Location>> {
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
        let document = roxmltree::Document::parse(&data);
        if let Ok(document) = document {
            let root = document.root_element();
            let (symbols, _range) = XmlAstUtils::get_symbols(session, file_symbol, root, offset, false);
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
                                let range = match s.borrow().typ() {
                                    SymType::PACKAGE(_) | SymType::FILE | SymType::NAMESPACE | SymType::DISK_DIR => Range::default(),
                                    _ => session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &full_path, &s.borrow().range()),
                                };
                                links.push(Location{uri: FileMgr::pathname2uri(&full_path), range});
                            }
                        }
                    },
                    XmlAstResult::XML_DATA(xml_file_symbol, range) => {
                        for path in xml_file_symbol.borrow().paths().iter() {
                            let full_path = match xml_file_symbol.borrow().typ() {
                                SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", xml_file_symbol.borrow().as_package().i_ext())).sanitize(),
                                _ => path.clone()
                            };
                            let range = match xml_file_symbol.borrow().typ() {
                                SymType::PACKAGE(_) | SymType::FILE | SymType::NAMESPACE | SymType::DISK_DIR => Range::default(),
                                _ => session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &full_path, &range),
                            };
                            links.push(Location{uri: FileMgr::pathname2uri(&full_path), range: range});
                        }
                    }
                }
            }
            return Some(links);
        }
        None
    }

    pub fn get_references_csv(_session: &mut SessionInfo, _file_symbol: &Rc<RefCell<Symbol>>, _file_info: &Rc<RefCell<FileInfo>>, _line: u32, _character: u32) -> Option<Vec<Location>> {
        None
    }
}