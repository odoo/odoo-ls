use lsp_types::{Hover, HoverContents, MarkupContent, Range};
use crate::core::file_mgr::FileInfo;
use crate::threads::SessionInfo;
use std::rc::Rc;
use crate::core::symbols::symbol::Symbol;
use crate::features::ast_utils::AstUtils;
use crate::features::features_utils::FeaturesUtils;
use std::cell::RefCell;


pub struct HoverFeature {}

impl HoverFeature {

    pub fn get_hover(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let (analyse_ast_result, range, call_expr) = AstUtils::get_symbols(session, file_symbol, file_info, offset as u32);
        let evals = analyse_ast_result.evaluations;
        if evals.is_empty() {
            return None;
        };
        let range = Some(Range {
            start: file_info.borrow().offset_to_position(range.unwrap().start().to_usize()),
            end: file_info.borrow().offset_to_position(range.unwrap().end().to_usize())
        });
        Some(Hover { contents:
            HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: FeaturesUtils::build_markdown_description(session, Some(file_symbol.clone()), &evals, &call_expr, Some(offset))
            }),
            range: range
        })
    }
}