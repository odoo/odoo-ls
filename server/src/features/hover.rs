use lsp_types::{Hover, HoverContents, MarkupContent};
use tracing::warn;
use crate::core::evaluation::Evaluation;
use crate::core::file_mgr::FileInfoKey;
use crate::core::symbols::symbol_keys::{SourceFileKey};
use crate::features::xml_ast_utils::XmlAstUtils;
use crate::threads::SessionInfo;
use crate::features::ast_utils::AstUtils;
use crate::features::features_utils::FeaturesUtils;


pub struct HoverFeature {}

impl HoverFeature {

    pub fn hover_python(session: &mut SessionInfo, file_symbol: SourceFileKey, file_info: FileInfoKey, line: u32, character: u32) -> Option<Hover> {
        let offset = session.file_mgr()[file_info].position_to_offset(line, character, session.sync_odoo.encoding);
        let file_info_ast_clone = session.file_mgr()[file_info].file_info_ast.clone();
        let file_info_ast_ref = file_info_ast_clone.borrow();
        let (analyse_ast_result, range, _expr, call_expr) = AstUtils::get_symbols(session, &file_info_ast_ref, file_symbol, offset as u32);
        let evals = analyse_ast_result.evaluations;
        if evals.is_empty() {
            return None;
        };
        let uri = session.file_mgr()[file_info].uri.clone();
        let value = FeaturesUtils::build_markdown_description(
            session, Some(file_symbol), Some(&uri), &evals, &call_expr, Some(offset)
        );
        if value.is_empty() {
            return None;
        }
        let range = Some(session.file_mgr()[file_info].text_range_to_range(range.unwrap(), session.sync_odoo.encoding));
        Some(Hover { contents:
            HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value
            }),
            range
        })
    }

    pub fn hover_xml(session: &mut SessionInfo, file_symbol: SourceFileKey, file_info: FileInfoKey, line: u32, character: u32) -> Option<Hover> {
        let offset = session.file_mgr()[file_info].position_to_offset(line, character, session.sync_odoo.encoding);
        let data = session.file_mgr()[file_info].file_info_ast.borrow().text_document.as_ref()?.contents().to_string();
        let document = match roxmltree::Document::parse(&data) {
            Ok(doc) => doc,
            Err(_) => {
                warn!("Failed to parse XML document for hover at line {}, character {} in file {}", line, character, session.file_mgr()[file_info].uri);
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
        let uri = session.file_mgr()[file_info].uri.clone();
        let value = FeaturesUtils::build_markdown_description(session, Some(file_symbol), Some(&uri), &evals, &None, Some(offset));
        if value.is_empty() {
            return None;
        }
        let range = range.map(|r| session.file_mgr()[file_info].std_range_to_range(&r, session.sync_odoo.encoding));
        Some(Hover { contents:
            HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value,
            }),
            range,
        })
    }

    pub fn hover_csv(_session: &mut SessionInfo, _file_symbol: SourceFileKey, _file_info: FileInfoKey, _line: u32, _character: u32) -> Option<Hover> {
        None
    }

    pub fn hover_js(session: &mut SessionInfo, file_path: &str, line: u32, character: u32) -> Option<Hover> {
        if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut()
            && let Some(hover) = bridge.get_hover(file_path, line, character) {
                return Some(Hover { contents:
                    HoverContents::Markup(MarkupContent {
                        kind: lsp_types::MarkupKind::Markdown,
                        value: hover
                    }),
                    range: None
                })
            }
        None
    }
}
