use ruff_text_size::TextRange;
use lsp_types::{Hover, HoverContents, MarkupContent, Range};
use tracing::warn;
use weak_table::traits::WeakElement;
use crate::core::evaluation::AnalyzeAstResult;
use crate::core::file_mgr::FileInfo;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;
use std::path::PathBuf;
use std::rc::Rc;
use crate::core::symbols::symbol::MainSymbol;
use crate::constants::*;
use crate::features::ast_utils::AstUtils;
use crate::S;
use std::cell::RefCell;

pub struct HoverFeature {}

impl HoverFeature {

    pub fn get_hover(session: &mut SessionInfo, file_symbol: &Rc<RefCell<MainSymbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let (analyse_ast_result, range): (AnalyzeAstResult, Option<TextRange>) = AstUtils::get_symbols(session, file_symbol, file_info, offset as u32);
        let evals = analyse_ast_result.symbols;
        if evals.is_empty() {
            return None;
        };
        let eval = &evals[0]; //TODO handle more evaluations
        let sym_ref = eval.symbol.get_symbol(session, &mut None, &mut vec![]).0;
        if sym_ref.is_expired() {
            warn!("symbol expired");
            return None;
        }
        let type_refs = MainSymbol::follow_ref(&sym_ref, session, &mut None, true, false, &mut vec![]);
        let type_ref = &type_refs[0].0; //TODO handle more evaluations
        let type_sym = type_ref.get_symbol();
        let type_loc_sym = type_ref.get_localized_symbol().unwrap();
        let mut type_str = S!("Any");
        if &sym_ref != type_ref && (type_loc_sym.borrow().loc_sym_type != SymType::VARIABLE || type_loc_sym.borrow().is_type_alias()) {
            type_str = type_ref.get_symbol().borrow().name.clone();
        }
        if analyse_ast_result.factory.is_some() && analyse_ast_result.effective_sym.is_some() {
            type_str = MainSymbol::follow_ref(&analyse_ast_result.effective_sym.unwrap().upgrade().unwrap().borrow().to_symbol_ref(), session, &mut None, true, false, &mut vec![])[0].0.get_symbol().borrow().name.clone();
        }
        let mut type_sym_name = type_loc_sym.borrow().loc_sym_type.to_string().to_lowercase();
        if type_loc_sym.borrow().is_import_variable && MainSymbol::next_refs(session, &type_ref, &mut None, &mut vec![]).len() > 0 {
            let next_ref = &MainSymbol::next_refs(session, &type_ref, &mut None, &mut vec![])[0];
            type_sym_name = next_ref.0.get_symbol().borrow().sym_type.to_string().to_lowercase();
        }
        if type_loc_sym.borrow().is_type_alias() {
            type_sym_name = S!("type alias");
            let mut type_alias_ref = MainSymbol::next_refs(session, &type_ref, &mut None, &mut vec![]);
            if type_alias_ref.len() > 0 {
                if &type_alias_ref[0].0 != type_ref {
                    let type_alias_ref = MainSymbol::follow_ref(&type_alias_ref[0].0, session, &mut None, true, false, &mut vec![]);
                    if type_alias_ref.len() > 0 {
                        type_str = type_alias_ref[0].0.get_symbol().borrow().name.clone();
                    }
                }
            }
        }
        if type_loc_sym.borrow().loc_sym_type == SymType::FUNCTION {
            if type_loc_sym.borrow()._function.as_ref().unwrap().is_property {
                type_sym_name = S!("property");
            } else {
                type_sym_name = S!("method");
            }
        }
        // BLOCK 1: (type) **name** -> infered_type
        let mut value: String = HoverFeature::build_block_1(&type_loc_sym, &type_sym_name, &type_str);
        // BLOCK 2: useful links
        if type_str != S!("Any") && type_str != S!("constant") {
            let paths = &type_sym.borrow().paths;
            if paths.len() > 0 {
                let mut path = PathBuf::new();//TODO FileMgr::pathname2uri(paths.first().unwrap());
                if type_sym.borrow().sym_type == SymType::PACKAGE {
                    path = PathBuf::from(path).join("__init__.py");
                }
                value += "  \n***  \n";
                value += format!("See also: [{}]({}#{})  \n", type_sym.borrow().name.as_str(), path.sanitize(), type_loc_sym.borrow().range.start().to_usize()).as_str();
            }
        }
        // BLOCK 3: documentation
        if type_loc_sym.borrow().doc_string.is_some() {
            value = value + "  \n***  \n" + type_loc_sym.borrow().doc_string.as_ref().unwrap();
        }
        let range = Some(Range {
            start: file_info.borrow().offset_to_position(range.unwrap().start().to_usize()),
            end: file_info.borrow().offset_to_position(range.unwrap().end().to_usize())
        });
        return Some(Hover { contents:
            HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: value
            }),
            range: range
        });
    }

    fn build_block_1(loc_sym: &Rc<RefCell<MainSymbol>>, type_sym: &String, infered_type: &String) -> String {
        let loc_sym = loc_sym.borrow();
        let mut value = S!("```python  \n");
        value += &format!("({}) ", type_sym);
        if loc_sym.loc_sym_type == SymType::FUNCTION && !loc_sym._function.as_ref().unwrap().is_property {
            value += "def ";
        }
        value += &loc_sym.symbol().borrow().name;
        if loc_sym.loc_sym_type == SymType::FUNCTION && !loc_sym._function.as_ref().unwrap().is_property {// && args?
            //TODO add args to function
        }
        if !infered_type.is_empty() && *type_sym != S!("module") {
            if loc_sym.loc_sym_type == SymType::FUNCTION && !loc_sym._function.as_ref().unwrap().is_property {
                value += " -> ";
                value += &infered_type;
            } else if loc_sym.symbol().borrow().name != *infered_type && loc_sym.loc_sym_type != SymType::CLASS {
                if *type_sym == S!("type alias") {
                    value += &format!(": type[{}]", infered_type);
                } else {
                    value += ": ";
                    value += &infered_type;
                }
            }
        }
        value += "  \n```";
        value
    }
}