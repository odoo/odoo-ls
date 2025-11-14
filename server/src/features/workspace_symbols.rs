use std::{cell::{Ref, RefCell}, rc::Rc};

use lsp_server::{ErrorCode, ResponseError};
use lsp_types::{Location, WorkspaceLocation, WorkspaceSymbol, WorkspaceSymbolResponse};
use ruff_text_size::{TextRange, TextSize};

use crate::{S, constants::{PackageType, SymType}, core::{entry_point::EntryPointType, file_mgr::FileMgr, symbols::symbol::Symbol}, threads::SessionInfo, utils::string_fuzzy_contains};

pub struct WorkspaceSymbolFeature;

impl WorkspaceSymbolFeature {

    pub fn get_workspace_symbols(session: &mut SessionInfo<'_>, query: String) -> Result<Option<WorkspaceSymbolResponse>, ResponseError> {
        let mut symbols = vec![];
        let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
        let mut can_resolve_location_range = false;
        if let Some(cap_workspace) = session.sync_odoo.capabilities.workspace.as_ref() {
            if let Some(workspace_symb) = cap_workspace.symbol.as_ref() {
                if let Some(resolve_support) = workspace_symb.resolve_support.as_ref() {
                    for resolvable_property in &resolve_support.properties {
                        if resolvable_property == "location.range" {
                            can_resolve_location_range = true;
                            break;
                        }
                    }
                }
            }
        }
        for entry in ep_mgr.borrow().iter_all() {
            if entry.borrow().typ == EntryPointType::BUILTIN || entry.borrow().typ == EntryPointType::PUBLIC { //We don't want to search in builtins
                continue;
            }
            if WorkspaceSymbolFeature::browse_symbol(session, &entry.borrow().root, &query, None, None, can_resolve_location_range, &mut symbols) {
                return Err(ResponseError {
                    code: ErrorCode::RequestCanceled as i32,
                    message: S!("Workspace Symbol request cancelled"),
                    data: None,
                });
            }
        }
        Ok(Some(WorkspaceSymbolResponse::Nested(symbols)))
    }

    /**
     * Return true if the request has been cancelled and the cancellation should be propagated
     */
    fn browse_symbol(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>, query: &String, parent: Option<String>, parent_path: Option<&String>, can_resolve_location_range: bool, results: &mut Vec<WorkspaceSymbol>) -> bool {
        let symbol_borrowed = symbol.borrow();
        if symbol_borrowed.typ() == SymType::VARIABLE {
            return false;
        }
        if symbol_borrowed.typ() == SymType::FILE { //to avoid too many locks
            if session.sync_odoo.is_request_cancelled() {
                return true;
            }
        }
        let container_name = match &parent {
            Some(p) => Some(p.clone()),
            None => None,
        };
        let path = symbol_borrowed.paths();
        let path = if path.len() == 1 {
            Some(&path[0])
        } else if path.len() == 0{
            parent_path
        } else {
            None
        };
        if path.is_some() && symbol_borrowed.has_range() {
            //Test if symbol should be returned
            if string_fuzzy_contains(&symbol_borrowed.name(), &query) {
                WorkspaceSymbolFeature::add_symbol_to_results(session, &symbol_borrowed, &symbol_borrowed.name().to_string(), path.unwrap(), container_name.clone(), Some(symbol_borrowed.range()), can_resolve_location_range, results);
            }
            //Test if symbol is a model
            if symbol_borrowed.typ() == SymType::CLASS && symbol_borrowed.as_class_sym()._model.is_some() {
                let model_data = symbol_borrowed.as_class_sym()._model.as_ref().unwrap();
                let model_name = S!("\"") + &model_data.name.to_string() + "\"";
                if string_fuzzy_contains(&model_name, &query) {
                    WorkspaceSymbolFeature::add_symbol_to_results(session, &symbol_borrowed, &model_name, path.unwrap(), container_name.clone(), Some(symbol_borrowed.range()), can_resolve_location_range, results);
                }
            }
        }
        if symbol_borrowed.typ() == SymType::PACKAGE(PackageType::MODULE) {
            let module = symbol_borrowed.as_module_package();
            for xml_id_name in module.xml_id_locations.keys() {
                let xml_name = S!("xmlid.") + xml_id_name;
                if string_fuzzy_contains(&xml_name, &query) {
                    let xml_data = module.get_xml_id(xml_id_name);
                    for data in xml_data {
                        let xml_file_symbol = data.get_xml_file_symbol();
                        if let Some(xml_file_symbol) = xml_file_symbol {
                            if let Some(path) = xml_file_symbol.borrow().paths().get(0) {
                                let range = data.get_range();
                                let text_range = TextRange::new(TextSize::new(range.start as u32), TextSize::new(range.end as u32));
                                WorkspaceSymbolFeature::add_symbol_to_results(session, &xml_file_symbol.borrow(), &xml_name, path, Some(symbol_borrowed.name().to_string()), Some(&text_range), can_resolve_location_range, results);
                            }
                        }
                    }
                }
            }
        }
        for sym in symbol_borrowed.all_symbols() {
            if WorkspaceSymbolFeature::browse_symbol(session, &sym, query, Some(symbol_borrowed.name().to_string()), path, can_resolve_location_range, results) {
                return true;
            }
        }
        false
    }

    fn add_symbol_to_results(session: &mut SessionInfo, symbol: &Ref<Symbol>, name: &String, path: &String, container_name: Option<String>, range: Option<&TextRange>, can_resolve_location_range: bool, results: &mut Vec<WorkspaceSymbol>) {
        let location = if can_resolve_location_range {
            lsp_types::OneOf::Right(WorkspaceLocation {
                uri: FileMgr::pathname2uri(path)
            })
        } else {
            let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(path);
            let Some(range) = range else {
                return;
            };
            if let Some(file_info) = file_info {
                lsp_types::OneOf::Left(Location::new(
                    FileMgr::pathname2uri(path),
                    file_info.borrow().text_range_to_range(range, session.sync_odoo.encoding)
                ))
            } else {
                return;
            }
        };
        let data = if can_resolve_location_range && range.is_some() {
            Some(lsp_types::LSPAny::Array(vec![
                lsp_types::LSPAny::Number(serde_json::Number::from(range.as_ref().unwrap().start().to_u32())),
                lsp_types::LSPAny::Number(serde_json::Number::from(range.as_ref().unwrap().end().to_u32())),
            ]))
        } else {
            None
        };
        results.push(WorkspaceSymbol {
            name: name.clone(),
            kind: symbol.get_lsp_symbol_kind(),
            tags: None,
            container_name,
            location: location,
            data: data,
        });
    }

    pub fn resolve_workspace_symbol(session: &mut SessionInfo<'_>, symbol: &WorkspaceSymbol) -> Result<WorkspaceSymbol, ResponseError> {
        let mut resolved_symbol = symbol.clone();
        let location = match &symbol.location {
            lsp_types::OneOf::Left(_) => None,
            lsp_types::OneOf::Right(wl) => Some(wl.clone()),
        };
        if let Some(location) = location {
            let uri = FileMgr::uri2pathname(location.uri.as_str());
            let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&uri);
            if let Some(file_info) = file_info {
                if let Some(data) = symbol.data.as_ref() {
                    if data.is_array() {
                        let arr = data.as_array().unwrap();
                        if arr.len() == 2 {
                            let start_u32 = arr[0].as_u64().unwrap() as u32;
                            let end_u32 = arr[1].as_u64().unwrap() as u32;
                            let range = file_info.borrow().try_text_range_to_range(
                                &TextRange::new(TextSize::new(start_u32), TextSize::new(end_u32)),
                                session.sync_odoo.encoding);
                            if let Some(range) = range {
                                resolved_symbol.location = lsp_types::OneOf::Left(Location::new(
                                    location.uri.clone(),
                                    range,
                                ));
                            } else {
                                return Err(ResponseError {
                                    code: ErrorCode::ContentModified as i32, message: S!("Unable to resolve Workspace Symbol - File content modified"), data: None
                                })
                            }
                            return Ok(resolved_symbol)
                        } else {
                            return Err(ResponseError { code: ErrorCode::InternalError as i32, message: S!("Unable to resolve Workspace Symbol - Invalid data to resolve range"), data: None })
                        }
                    } else {
                        return Err(ResponseError { code: ErrorCode::InternalError as i32, message: S!("Unable to resolve Workspace Symbol - Invalid data to resolve range"), data: None })
                    }
                } else {
                    return Err(ResponseError { code: ErrorCode::InternalError as i32, message: S!("Unable to resolve Workspace Symbol - No data to resolve range"), data: None })
                }
            } else {
                return Err(ResponseError { code: ErrorCode::InternalError as i32, message: S!("Unable to resolve Workspace Symbol - No file info"), data: None })
            }
        } else {
            return Err(ResponseError { code: ErrorCode::InternalError as i32, message: S!("Unable to resolve Workspace Symbol - no provided location to resolve"), data: None })
        }
    }

}