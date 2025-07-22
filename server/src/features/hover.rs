use lsp_types::{Hover, HoverContents, MarkupContent, Position, Range};
use crate::core::evaluation::Evaluation;
use crate::core::file_mgr::FileInfo;
use crate::features::xml_ast_utils::{XmlAstResult, XmlAstUtils};
use crate::threads::SessionInfo;
use std::rc::Rc;
use crate::core::symbols::symbol::Symbol;
use crate::features::ast_utils::AstUtils;
use crate::features::features_utils::FeaturesUtils;
use std::cell::RefCell;


pub struct HoverFeature {}

impl HoverFeature {

    pub fn hover_python(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let (analyse_ast_result, range, call_expr) = AstUtils::get_symbols(session, file_symbol, file_info, offset as u32);
        let evals = analyse_ast_result.evaluations;
        if evals.is_empty() {
            return None;
        };
        let range = Some(file_info.borrow().text_range_to_range(&range.unwrap()));
        Some(Hover { contents:
            HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: FeaturesUtils::build_markdown_description(session, Some(file_symbol.clone()), &evals, &call_expr, Some(offset))
            }),
            range: range
        })
    }

    pub fn hover_xml(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let data = file_info.borrow().file_info_ast.borrow().text_rope.as_ref().unwrap().to_string();
        let document = roxmltree::Document::parse(&data);
        if let Ok(document) = document {
            let root = document.root_element();
            let (symbols, range) = XmlAstUtils::get_symbols(session, file_symbol, root, offset);
            let range = range.map(|r| (file_info.borrow().std_range_to_range(&r)));
            let evals = symbols.iter().filter(|s| matches!(s, XmlAstResult::SYMBOL(_)))
                .map(|s| Evaluation::eval_from_symbol(&Rc::downgrade(&s.as_symbol()), Some(false))).collect::<Vec<Evaluation>>();
            return Some(Hover { contents:
                HoverContents::Markup(MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: FeaturesUtils::build_markdown_description(session, Some(file_symbol.clone()), &evals, &None, Some(offset))
                }),
                range: range
            })
        }
        None
    }

    pub fn hover_csv(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
        None
    }
}