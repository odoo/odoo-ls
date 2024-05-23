use std::{cell::RefCell, rc::Rc};
use ruff_text_size::TextRange;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::GotoDefinitionResponse;

use crate::core::evaluation::AnalyzeAstResult;
use crate::core::file_mgr::FileInfo;
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

        Ok(None)
    }
}