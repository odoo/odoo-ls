use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};
use ruff_text_size::{TextRange, TextSize};
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
        let (analyse_ast_result, _range): (AnalyzeAstResult, Option<TextRange>) = AstUtils::get_symbols(session, file_symbol, file_info, offset as u32);
        if analyse_ast_result.evaluations.len() == 0 {
            return None;
        }
        let mut links = vec![];
        let mut evaluations = analyse_ast_result.evaluations.clone();
        let mut index = 0;
        while index < evaluations.len() {
            let eval = evaluations[index].clone();
            let sym_ref = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let loc_sym = sym_ref.weak.upgrade();
            if loc_sym.is_none() {
                index += 1;
                continue;
            }
            let symbol =loc_sym.unwrap();
            let file = symbol.borrow().get_file();
            if let Some(file) = file {
                //if the symbol is at the given offset, let's take the next evaluation instead
                if Rc::ptr_eq(&file.upgrade().unwrap(), &file_symbol) && symbol.borrow().has_range() && symbol.borrow().range().contains(TextSize::new(offset as u32)) {
                    evaluations.remove(index);
                    let symbol = symbol.borrow();
                    let sym_eval = symbol.evaluations();
                    if let Some(sym_eval) = sym_eval.clone() {
                        evaluations = [evaluations.clone(), sym_eval.clone()].concat();
                    }
                    continue;
                }
                for path in file.upgrade().unwrap().borrow().paths().iter() {
                    links.push(
                        match symbol.borrow().typ() {
                            SymType::PACKAGE => Location{
                                uri: FileMgr::pathname2uri(&PathBuf::from(path).join(format!("__init__.py{}", symbol.borrow().as_package().i_ext())).sanitize()),
                                range: Range::default()
                            },
                            SymType::FILE => Location{
                                uri: FileMgr::pathname2uri(path),
                                range: Range::default()
                            },
                            _ => {
                                let range = symbol.borrow().range().clone();
                                Location{
                                    uri: FileMgr::pathname2uri(path),
                                    range: session.sync_odoo.get_file_mgr().borrow_mut().text_range_to_range(session, path, &range)
                                }
                            }
                        }
                    );
                }
            }
            index += 1;
        }
        Some(GotoDefinitionResponse::Array(links))
    }
}