use lsp_types::{GotoDefinitionResponse, Location, LocationLink, Position, Range};
use std::{cell::RefCell, rc::Rc};

use crate::core::file_mgr::{Ast, FileInfo, FileMgr};
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::features::goto_utils::{GotoRequest, GotoSource, GotoSourceType, GotoUtils};
use crate::threads::SessionInfo;

pub struct DefinitionFeature {}

impl DefinitionFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: SourceFileKey,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let ast_type = file_info.borrow().file_info_ast.borrow().ast.clone();
        let definitions_sources = match ast_type {
            Ast::PythonAst(_) => GotoUtils::get_symbols(session, GotoRequest::Definition, file_symbol, file_info, line, character),
            Ast::XmlAst => GotoUtils::get_symbols_xml(session, file_symbol, file_info, line, character),
            Ast::CsvAst => GotoUtils::get_symbols_csv(session, file_symbol, file_info, line, character),
            Ast::JsAst(_) => {return DefinitionFeature::get_js_definition(session, file_info, line, character);},
        };
        let links: Vec<LocationLink> = definitions_sources
            .iter()
            .flat_map(|def| GotoUtils::goto_source_to_location(session, def))
            .collect();
        Some(GotoDefinitionResponse::Link(links))
    }

    fn get_js_definition(session: &mut SessionInfo, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<GotoDefinitionResponse> {
        // Check if cursor is over a template reference (e.g. `static template = "module.xml_id"`)
        let template_refs = file_info.borrow().file_info_ast.borrow().ast.as_js_ast().js_template_refs.clone();
        let template_hit = template_refs.iter().find_map(|template_ref| {
            let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &file_info.borrow().uri, &template_ref.range);
            Self::position_in_range(line, character, &range).then_some((template_ref, range))
        });
        if let Some((template_ref, range)) = template_hit
            && let Some(templates) = session.sync_odoo.js_templates.get(&template_ref.t_name)
        {
            let mut locations = vec![];
            for template in templates.iter_valid(&session.sync_odoo.symbol_table) {
                locations.extend(GotoUtils::goto_source_to_location(session, &GotoSource {
                    source: GotoSourceType::SymbolKey(template.into()),
                    origin_selection_range: Some(range),
                }));
            }
            return Some(GotoDefinitionResponse::Link(locations));
        }

        let file_path = &file_info.borrow().uri;
        let locs: Vec<Location> = if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
            bridge.get_definition(file_path, line, character)
                .into_iter()
                .map(|(target_file, sl, sc, el, ec)| {
                    let uri = FileMgr::pathname2uri(&target_file);
                    Location {
                        uri,
                        range: Range {
                            start: Position { line: sl, character: sc },
                            end:   Position { line: el, character: ec },
                        },
                    }
                })
                .collect()
        } else {
            vec![]
        };
        if locs.is_empty() {
            return None;
        }
        Some(GotoDefinitionResponse::Array(locs))
    }

    fn position_in_range(line: u32, character: u32, range: &Range) -> bool {
        let after_start = line > range.start.line
            || (line == range.start.line && character >= range.start.character);
        let before_end = line < range.end.line
            || (line == range.end.line && character <= range.end.character);
        after_start && before_end
    }
}
