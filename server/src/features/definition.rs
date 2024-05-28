use std::{cell::RefCell, rc::Rc};
use ruff_text_size::TextRange;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Range};

use crate::constants::SymType;
use crate::core::evaluation::AnalyzeAstResult;
use crate::core::file_mgr::{FileMgr, FileInfo};
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::features::ast_utils::AstUtils;



pub struct DefinitionFeature {}

impl DefinitionFeature {

    pub fn get_location(odoo: &mut SyncOdoo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Result<Option<GotoDefinitionResponse>> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let (analyse_ast_result, range): (AnalyzeAstResult, Option<TextRange>) = AstUtils::get_symbols(odoo, file_symbol, file_info, offset as u32);
        if analyse_ast_result.symbol.is_none() {
            return Ok(None);
        }
        let mut links = vec![];
        let sym = analyse_ast_result.symbol.as_ref().unwrap().symbol.get_symbol(odoo, &mut None, &mut vec![]).0.upgrade();
        let sym = sym.as_ref().unwrap().borrow();
        let file = sym.get_in_parents(&vec![SymType::FILE], true);
        if let Some(file) = file {
            for path in file.upgrade().unwrap().borrow().paths.iter() {
                let file_info = FileMgr::get_file_info(&odoo.get_file_mgr().borrow(), path);
                links.push(Location{
                    uri: FileMgr::pathname2uri(path),
                    range: Range{
                        start: file_info.borrow().offset_to_position(sym.range.unwrap().start().to_usize()),
                        end: file_info.borrow().offset_to_position(sym.range.unwrap().end().to_usize())
                    }
                });
            }
        }
        Ok(Some(GotoDefinitionResponse::Array(links)))
    }
}