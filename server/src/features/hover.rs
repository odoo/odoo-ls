use lsp_types::{Hover, HoverContents, MarkupContent};
use tracing::warn;
use crate::core::evaluation::Evaluation;
use crate::core::file_mgr::FileInfo;
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::features::xml_ast_utils::XmlAstUtils;
use crate::threads::SessionInfo;
use std::rc::Rc;
use crate::features::ast_utils::AstUtils;
use crate::features::features_utils::FeaturesUtils;
use std::cell::RefCell;


pub struct HoverFeature {}

impl HoverFeature {

    pub fn hover_python(session: &mut SessionInfo, file_symbol: SourceFileKey, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let file_info_ast_clone = file_info.borrow().file_info_ast.clone();
        let file_info_ast_ref = file_info_ast_clone.borrow();
        let (analyse_ast_result, range, expr, call_expr) = AstUtils::get_symbols(session, &file_info_ast_ref, file_symbol, offset as u32);
        let evals = analyse_ast_result.evaluations;
        if evals.is_empty() {
            return None;
        };
        drop(expr);
        drop(file_info_ast_ref);
        let value = FeaturesUtils::build_markdown_description(
            session, Some(file_symbol), Some(&file_info.borrow().uri), &evals, &call_expr, Some(offset)
        );
        if value.is_empty() {
            return None;
        }
        let range = Some(file_info.borrow().text_range_to_range(&range.unwrap(), session.sync_odoo.encoding));
        Some(Hover { contents:
            HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value
            }),
            range
        })
    }

    pub fn hover_xml(session: &mut SessionInfo, file_symbol: SourceFileKey, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
        let document = match roxmltree::Document::parse(&data) {
            Ok(doc) => doc,
            Err(_) => {
                warn!("Failed to parse XML document for hover at line {}, character {} in file {}", line, character, file_info.borrow().uri);
                return None;
            }
        };
        let root = document.root_element();
        let (symbols, range) = XmlAstUtils::get_symbols(session, file_symbol, root, offset, true);
        if symbols.is_empty() {
            return None;
        }
        let evals = symbols.iter()
            .map(|s| Evaluation::eval_from_symbol(session.st(), *s, Some(false))).collect::<Vec<Evaluation>>();
        let value = FeaturesUtils::build_markdown_description(session, Some(file_symbol), Some(&file_info.borrow().uri), &evals, &None, Some(offset));
        if value.is_empty() {
            return None;
        }
        let range = range.map(|r| file_info.borrow().std_range_to_range(&r, session.sync_odoo.encoding));
        Some(Hover { contents:
            HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value,
            }),
            range,
        })
    }

    pub fn hover_csv(_session: &mut SessionInfo, _file_symbol: SourceFileKey, _file_info: &Rc<RefCell<FileInfo>>, _line: u32, _character: u32) -> Option<Hover> {
        None
    }
}
