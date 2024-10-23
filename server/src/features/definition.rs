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
        if analyse_ast_result.evaluations.len() == 0 {
            return None;
        }
        let mut links = vec![];
        for eval in analyse_ast_result.evaluations.iter() {
            let sym_ref = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let loc_sym = sym_ref.0.upgrade();
            if loc_sym.is_none() {
                continue;
            }
            let symbol =loc_sym.unwrap();
            let file = symbol.borrow().get_file();
            if let Some(file) = file {
                for path in file.upgrade().unwrap().borrow().paths().iter() {
                    match symbol.borrow().typ() {
                        SymType::PACKAGE => {
                            links.push(Location{
                                uri: FileMgr::pathname2uri(&PathBuf::from(path).join(format!("__init__.py{}", symbol.borrow().as_package().i_ext())).sanitize()),
                                range: Range::default()
                            });
                        },
                        _ => {
                            let range = if eval.range.is_some() {
                                eval.range.unwrap().clone()
                            } else {
                                let get_sym = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
                                if let Some(eval_sym) = get_sym.0.upgrade() {
                                    eval_sym.borrow().range().clone()
                                } else {
                                    continue;
                                }
                            };
                            links.push(Location{
                                uri: FileMgr::pathname2uri(path),
                                range: session.sync_odoo.get_file_mgr().borrow_mut().text_range_to_range(session, path, &range)
                            });
                        }
                    }
                }
            }
        }
        Some(GotoDefinitionResponse::Array(links))
    }
}