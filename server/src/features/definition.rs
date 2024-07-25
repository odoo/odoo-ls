use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};
use ruff_text_size::TextRange;
use lsp_types::{GotoDefinitionResponse, Location, Range};

use crate::constants::SymType;
use crate::core::evaluation::AnalyzeAstResult;
use crate::core::file_mgr::{FileMgr, FileInfo};
use crate::core::symbols::symbol::Symbol;
use crate::threads::SessionInfo;
use crate::features::ast_utils::AstUtils;
use crate::utils::PathSanitizer as _;



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
        if analyse_ast_result.symbols.len() == 0 {
            return None;
        }
        let mut links = vec![];
        for sym in analyse_ast_result.symbols.iter() {
            let sym_ref = sym.symbol.get_symbol(session, &mut None, &mut vec![]);
            let loc_sym = sym_ref.0.get_localized_symbol();
            if loc_sym.is_none() {
                continue;
            }
            let symbol = sym_ref.0.get_symbol();
            let file = symbol.borrow().get_file();
            if let Some(file) = file {
                for path in file.upgrade().unwrap().borrow().paths.iter() {
                    match symbol.borrow().sym_type {
                        SymType::PACKAGE => {
                            links.push(Location{
                                uri: FileMgr::pathname2uri(&PathBuf::from(path).join("__init__.py").sanitize()),
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
        }
        Some(GotoDefinitionResponse::Array(links))
    }
}