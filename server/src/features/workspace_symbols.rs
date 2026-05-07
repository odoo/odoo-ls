use lsp_server::{ErrorCode, ResponseError};
use lsp_types::{Location, WorkspaceLocation, WorkspaceSymbol, WorkspaceSymbolResponse};
use ruff_text_size::{TextRange, TextSize};

use crate::{S, constants::SymType, core::{entry_point::EntryPointType, file_mgr::FileMgr, symbols::{storage::SymbolTable, symbol_keys::{SourceFileKey, SymbolKey}}}, threads::SessionInfo, utils::string_fuzzy_contains};

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
            if WorkspaceSymbolFeature::browse_symbol(session, entry.borrow().root.into(), &query, None, None, can_resolve_location_range, &mut symbols) {
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
    fn browse_symbol(session: &mut SessionInfo, symbol: SymbolKey, query: &String, parent: Option<String>, parent_path: Option<&String>, can_resolve_location_range: bool, results: &mut Vec<WorkspaceSymbol>) -> bool {
        if symbol.typ() == SymType::VARIABLE {
            return false;
        }
        if symbol.typ() == SymType::FILE { //to avoid too many locks
            if session.sync_odoo.is_request_cancelled() {
                return true;
            }
        }
        let container_name = match &parent {
            Some(p) => Some(p.clone()),
            None => None,
        };
        let path = session.st().paths(symbol);
        let path = if path.len() == 1 {
            Some(&path[0])
        } else if path.len() == 0{
            parent_path
        } else {
            None
        };
        if path.is_some() && session.st().has_range(symbol) {
            //Test if symbol should be returned
            if string_fuzzy_contains(&session.st().name(symbol), &query) {
                let name = session.st().name(symbol).to_string();
                let range = session.st().range(symbol).clone();
                WorkspaceSymbolFeature::add_symbol_to_results(session, symbol, &name, path.unwrap(), container_name.clone(), Some(&range), can_resolve_location_range, results);
            }
            //Test if symbol is a model
            if let SymbolKey::Class(class_key) = symbol && let Some(model_data) = session.st()[class_key]._model.as_ref() {
                let model_name = S!("\"") + &model_data.name + "\"";
                let range = session.st().range(symbol).clone();
                if string_fuzzy_contains(&model_name, &query) {
                    WorkspaceSymbolFeature::add_symbol_to_results(session, symbol, &model_name, path.unwrap(), container_name.clone(), Some(&range), can_resolve_location_range, results);
                }
            }
        }
        if let SymbolKey::Module(module_key) = symbol {
            let mut res_to_add = vec![];
            for (xml_id_name, data_set) in session.st()[module_key].xml_ids.iter() {
                let xml_name = S!("xmlid.") + xml_id_name;
                if string_fuzzy_contains(&xml_name, &query) {
                    for data in data_set.iter_valid(session.st()) {
                        let xml_file_symbol = session.st().get_file(data.into());
                        if let Some(SourceFileKey::XmlFile(xml_file_key)) = xml_file_symbol {
                            let xml_file = &session.st()[xml_file_key];
                            let name = xml_file.name.to_string();
                            let path = xml_file.path.clone();
                            let data_range = session.st().range(data.into()).clone();
                            res_to_add.push((xml_file_key, xml_name.clone(), path, name, data_range));
                        }
                    }
                }
            }
            for (xml_file_key, xml_name, path, name, text_range) in res_to_add {
                WorkspaceSymbolFeature::add_symbol_to_results(session, xml_file_key.into(), &xml_name, &path, Some(name), Some(&text_range), can_resolve_location_range, results);
            }
        }
        for sym in session.st().all_symbols(symbol) {
            if WorkspaceSymbolFeature::browse_symbol(session, sym, query, Some(session.st().name(symbol).to_string()), path, can_resolve_location_range, results) {
                return true;
            }
        }
        false
    }

    fn add_symbol_to_results(session: &mut SessionInfo, symbol: SymbolKey, name: &String, path: &String, container_name: Option<String>, range: Option<&TextRange>, can_resolve_location_range: bool, results: &mut Vec<WorkspaceSymbol>) {
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
            kind: SymbolTable::get_lsp_symbol_kind(symbol),
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
