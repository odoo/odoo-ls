use crate::core::evaluation::Evaluation;
use crate::core::file_mgr::FileInfo;
use crate::core::symbols::symbol::Symbol;
use crate::features::ast_utils::AstUtils;
use crate::features::features_utils::FeaturesUtils;
use crate::features::xml_ast_utils::{XmlAstResult, XmlAstUtils};
use crate::threads::SessionInfo;
use lsp_types::{Hover, HoverContents, MarkupContent};
use std::cell::RefCell;
use std::rc::Rc;
use tracing::warn;


pub struct HoverFeature {}

impl HoverFeature {

    pub fn hover_python(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
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
            session, Some(file_symbol.clone()), Some(&file_info.borrow().uri), &evals, &call_expr, Some(offset)
        )?;
        let range = Some(file_info.borrow().text_range_to_range(&range.unwrap(), session.sync_odoo.encoding));
        Some(Hover { contents:
            HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value
            }),
            range: range
        })
    }

    pub fn hover_xml(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
        let document_result = roxmltree::Document::parse(&data);
        let document = match document_result {
             Ok(doc) => doc,
             Err(_) => {
                warn!("Failed to parse XML document for hover at line {}, character {} in file {}", line, character, file_info.borrow().uri);
                return None
            },
         };
        let root = document.root_element();
        let (xml_results, range) = XmlAstUtils::get_symbols(session, file_symbol, root, offset, true);
        if xml_results.is_empty() {
            return None;
        }
        // Separate Symbol and XML_DATA results
        let (symbols, xml_data): (Vec<_>, Vec<_>) = xml_results.into_iter().fold(
            (Vec::new(), Vec::new()),
            |(mut symbols, mut xml_data), e| {
                match e {
                    XmlAstResult::SYMBOL(s) => symbols.push(s),
                    XmlAstResult::XML_DATA(x_sym, x_range) => xml_data.push((x_sym, x_range)),
                }
                (symbols, xml_data)
            },
        );
        // Process Symbol results
        let evals = symbols.iter()
            .map(|s| Evaluation::eval_from_symbol(&Rc::downgrade(&s), Some(false))).collect::<Vec<Evaluation>>();
        let python_blocks = FeaturesUtils::build_markdown_description(session, Some(file_symbol.clone()), Some(&file_info.borrow().uri), &evals, &None, Some(offset));
        // Process XML_DATA results
        let xml_blocks = FeaturesUtils::build_xml_data_markdown_description(&xml_data);
        let value = python_blocks.into_iter().chain(xml_blocks.into_iter()).collect::<Vec<_>>().join("  \n***  \n");
        let range = range.map(|r| file_info.borrow().std_range_to_range(&r, session.sync_odoo.encoding));
        Some(Hover { contents:
            HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value,
            }),
            range
        })
    }

    pub fn hover_csv(_session: &mut SessionInfo, _file_symbol: &Rc<RefCell<Symbol>>, _file_info: &Rc<RefCell<FileInfo>>, _line: u32, _character: u32) -> Option<Hover> {
        None
    }
}