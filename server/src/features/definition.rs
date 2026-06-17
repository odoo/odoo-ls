use lsp_types::{GotoDefinitionResponse};
use std::{cell::RefCell, rc::Rc};

use crate::core::file_mgr::{AstType, FileInfo};
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::features::goto_utils::{GotoRequest, GotoUtils};
use crate::threads::SessionInfo;

pub struct DefinitionFeature {}

impl DefinitionFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: SourceFileKey,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let ast_type = file_info.borrow().file_info_ast.borrow().ast_type.clone();
        let definitions_sources = match ast_type {
            AstType::Python => GotoUtils::get_symbols(session, GotoRequest::Definition, file_symbol, file_info, line, character),
            AstType::Xml => GotoUtils::get_symbols_xml(session, file_symbol, file_info, line, character),
            AstType::Csv => GotoUtils::get_symbols_csv(session, file_symbol, file_info, line, character),
            AstType::Js => {vec![]}, // Fill later directly with links from tsserver
        };
        let mut links = vec![];
        for def in definitions_sources.iter() {
            links.extend(GotoUtils::goto_source_to_location(session, def));
        }
        }
        if matches!(ast_type, AstType::Js) {
            // Check if cursor is over a template reference (e.g. `static template = "module.xml_id"`)
            let template_refs = file_info.borrow().file_info_ast.borrow().js_template_refs.clone();
            for (range, template_name, _) in &template_refs {
                if Self::position_in_range(line, character, range) {
                    let Some(templates) = session.sync_odoo.js_templates.get(template_name) else { continue; };
                    let mut locations = vec![];
                    for template in templates.iter_valid(&session.sync_odoo.symbol_table) {
                        locations.extend(GotoUtils::goto_source_to_location(session, &GotoSource {
                            source: template.into(),
                            origin_selection_range: Some(*range),
                        }));
                    }
                    return Some(GotoDefinitionResponse::Link(locations));
                }
            }

            let file_path = &file_info.borrow().uri;
            let locs: Vec<Location> = if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
                bridge.get_definition(&file_path, line, character)
                    .into_iter()
                    .filter_map(|(target_file, sl, sc, el, ec)| {
                        let uri = FileMgr::pathname2uri(&target_file);
                        Some(Location {
                            uri,
                            range: Range {
                                start: Position { line: sl, character: sc },
                                end:   Position { line: el, character: ec },
                            },
                        })
                    })
                    .collect()
            } else {
                vec![]
            };
            if locs.is_empty() {
                return None;
            }
            return Some(GotoDefinitionResponse::Array(locs));
        }
        Some(GotoDefinitionResponse::Link(links))
    }
}
