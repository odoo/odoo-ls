use ruff_text_size::TextRange;
use lsp_types::{Hover, HoverContents, MarkupContent, Range};
use weak_table::traits::WeakElement;
use crate::core::evaluation::AnalyzeAstResult;
use crate::core::file_mgr::FileInfo;
use crate::threads::SessionInfo;
use std::path::PathBuf;
use std::rc::Rc;
use crate::core::symbol::Symbol;
use crate::constants::*;
use crate::features::ast_utils::AstUtils;
use crate::S;
use std::cell::RefCell;

pub struct HoverFeature {}

impl HoverFeature {

    pub fn get_hover(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let (analyse_ast_result, range): (AnalyzeAstResult, Option<TextRange>) = AstUtils::get_symbols(session, file_symbol, file_info, offset as u32);
        let Some(evaluation) = analyse_ast_result.symbol.as_ref() else {
            return None;
        };
        let symbol = evaluation.symbol.get_symbol(session, &mut None, &mut vec![]).0;
        if symbol.is_expired() {
            println!("symbol expired");
            return None;
        }
        let symbol = symbol.upgrade().unwrap();
        let (type_ref, _) = Symbol::follow_ref(symbol.clone(), session, &mut None, true, false, &mut vec![]);
        let type_ref = type_ref.upgrade().unwrap();
        let mut type_str = S!("Any");
        if !Rc::ptr_eq(&type_ref, &symbol) && (type_ref.borrow().sym_type != SymType::VARIABLE || type_ref.borrow().is_type_alias()) {
            type_str = type_ref.borrow().name.clone();
        }
        if analyse_ast_result.factory.is_some() && analyse_ast_result.effective_sym.is_some() {
            type_str = Symbol::follow_ref(analyse_ast_result.effective_sym.unwrap().upgrade().unwrap(), session, &mut None, true, false, &mut vec![]).0.upgrade().unwrap().borrow().name.clone();
        }
        let mut type_sym = symbol.borrow().sym_type.to_string().to_lowercase();
        if symbol.borrow().is_type_alias() {
            type_sym = S!("type alias");
            let mut type_alias_ref = Symbol::next_ref(&type_ref.borrow(), session, &mut None, &mut vec![]);
            if let Some(mut type_alias_ref) = type_alias_ref {
                if !Rc::ptr_eq(&type_alias_ref.upgrade().unwrap(), &type_ref) {
                    type_alias_ref = Symbol::follow_ref(type_alias_ref.upgrade().unwrap(), session, &mut None, true, false, &mut vec![]).0;
                    type_str = type_alias_ref.upgrade().unwrap().borrow().name.clone();
                }
            }
        }
        if symbol.borrow().sym_type == SymType::FUNCTION {
            if symbol.borrow()._function.as_ref().unwrap().is_property {
                type_sym = S!("property");
            } else {
                type_sym = S!("method");
            }
        }
        // BLOCK 1: (type) **name** -> infered_type
        let mut value: String = HoverFeature::build_block_1(&symbol, &type_sym, &type_str);
        // BLOCK 2: useful links
        if type_str != S!("Any") && type_str != S!("constant") {
            let paths = &type_ref.borrow().paths;
            if paths.len() > 0 {
                let mut path = PathBuf::new();//TODO FileMgr::pathname2uri(paths.first().unwrap());
                if type_ref.borrow().sym_type == SymType::PACKAGE {
                    path = PathBuf::from(path).join("__init__.py");
                }
                if type_ref.borrow().range.is_none() {
                    let t = type_ref.borrow();
                    let t2 = t.sym_type;
                    let t3 = t.name.clone();
                    println!("no range defined");
                } else {
                    value += "  \n***  \n";
                    value += format!("See also: [{}]({}#{})  \n", type_ref.borrow().name.as_str(), path.to_str().unwrap(), type_ref.borrow().range.unwrap().start().to_usize()).as_str();
                }
            }
        }
        // BLOCK 3: documentation
        if symbol.borrow().doc_string.is_some() {
            value = value + "  \n***  \n" + symbol.borrow().doc_string.as_ref().unwrap();
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

    fn build_block_1(symbol: &Rc<RefCell<Symbol>>, type_sym: &String, infered_type: &String) -> String {
        let symbol = symbol.borrow();
        let mut value = S!("```python  \n");
        value += &format!("({}) ", type_sym);
        if symbol.sym_type == SymType::FUNCTION && !symbol._function.as_ref().unwrap().is_property {
            value += "def ";
        }
        value += &symbol.name;
        if symbol.sym_type == SymType::FUNCTION && !symbol._function.as_ref().unwrap().is_property {// && args?
            //TODO add args to function
        }
        if !infered_type.is_empty() && *type_sym != S!("module") {
            if symbol.sym_type == SymType::FUNCTION && !symbol._function.as_ref().unwrap().is_property {
                value += " -> ";
                value += &infered_type;
            } else if symbol.name != *infered_type && symbol.sym_type != SymType::CLASS {
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