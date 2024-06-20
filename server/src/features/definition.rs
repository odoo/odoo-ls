use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};
use ruff_text_size::TextRange;
use lsp_types::{GotoDefinitionResponse, Location, Range};

use crate::constants::SymType;
use crate::core::evaluation::AnalyzeAstResult;
use crate::core::file_mgr::{FileMgr, FileInfo};
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::threads::SessionInfo;
use crate::features::ast_utils::AstUtils;



pub struct DefinitionFeature {}

impl DefinitionFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let (analyse_ast_result, range): (AnalyzeAstResult, Option<TextRange>) = AstUtils::get_symbols(session, file_symbol, file_info, offset as u32);
        if analyse_ast_result.symbol.is_none() {
            return None;
        }
        let mut links = vec![];
        let sym = analyse_ast_result.symbol.as_ref().unwrap().symbol.get_symbol(session, &mut None, &mut vec![]).0.upgrade();
        if sym.is_none() {
            return None;
        }
        let sym = sym.as_ref().unwrap().borrow();
        let file = sym.get_in_parents(&vec![SymType::FILE, SymType::PACKAGE], true);
        if let Some(file) = file {
            for path in file.upgrade().unwrap().borrow().paths.iter() {
                match sym.sym_type {
                    SymType::PACKAGE => {
                        links.push(Location{
                            uri: FileMgr::pathname2uri(&PathBuf::from(path).join("__init__.py").to_str().unwrap().to_string()),
                            range: Range::default()
                        });
                    },
                    _ => {
                        links.push(Location{
                            uri: FileMgr::pathname2uri(path),
                            range: session.sync_odoo.get_file_mgr().borrow_mut().text_range_to_range(session, path, &sym.range.unwrap())
                        });
                    }
                }
            }
        }
        Some(GotoDefinitionResponse::Array(links))
    }
}