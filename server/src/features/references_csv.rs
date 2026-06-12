use crate::{
    Sy, constants::OYarn, core::{
        evaluation_utils::DeepFieldEvalWalker, file_mgr::FileMgr, symbols::{
            symbol_keys::{CsvFileKey, ModuleKey}
        }
    }, features::{csv_ast_utils::CsvFieldIter, references::ReferenceTarget}, oyarn, threads::SessionInfo
};
use csv::{Reader, StringRecord};
use lsp_types::{Location, Uri};

pub struct CsvAstReferenceVisitor {}

impl CsvAstReferenceVisitor {

    /* search for a specific symbol in headers. Not used for  */
    pub fn search_target(session: &mut SessionInfo, file_symbol: CsvFileKey, csv_reader: &mut Reader<&[u8]>, model_name: Option<&OYarn>, target: &ReferenceTarget, content: &str) -> Vec<Location> {
        let mut results = vec![];
        let path = session.st()[file_symbol].path.clone();
        let uri = FileMgr::pathname2uri(&path);
        let module = session.st().find_module(file_symbol);
        let mut headers = vec![];
        if csv_reader.has_headers() {
            if let Ok(header) = csv_reader.headers() {
                for (start, end, h) in CsvFieldIter::new(header, content).unwrap() {
                    headers.push(oyarn!("{}", h));
                    let header_elts = h.splitn(2, [':', '/']).collect::<Vec<_>>();
                    if let &ReferenceTarget::Symbol(target_sym) = target {
                        let Some(model_name) = model_name else {continue;};
                        let Some(model) = session.sync_odoo.models.get(model_name).cloned() else {return vec![];};
                        let Some(main_symbol) = model.borrow().get_main_symbols(session, module).next() else {return results;};
                        let mut deep_field_walker = DeepFieldEvalWalker::new(main_symbol.into(), module);
                        let symbols =
                            deep_field_walker.get_model_fields(session, main_symbol.into(), header_elts[0]);
                        if symbols.iter().any(|&sym| target_sym == sym) {
                            results.push(Location {
                                uri: uri.clone(),
                                range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &path, &std::ops::Range {
                                    start,
                                    end,
                                }),
                            });
                        }
                        let Some(next_base) = deep_field_walker.get_model_symbol(session) else {
                            continue;
                        };
                        let sub_symbols = deep_field_walker.get_model_fields(session, next_base.into(), header_elts[1]);
                        if sub_symbols.iter().any(|&sym| target_sym == sym) {
                            results.push(Location {
                                uri: uri.clone(),
                                range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &path, &std::ops::Range {
                                    start,
                                    end,
                                }),
                            });
                        }
                    }
                }
            }
        }
        if !headers.is_empty() && headers[0] == "id" && matches!(target, ReferenceTarget::String(_)) {
            for result in csv_reader.records() {
                if let Ok(result) = result {
                    results.extend(CsvAstReferenceVisitor::search_in_record(session, module, &uri, &path, &headers, &result, target, content));
                }
            }
        }
        results
    }

    fn search_in_record(session: &mut SessionInfo, module: Option<ModuleKey>, uri: &Uri, path: &String, headers: &Vec<OYarn>, record: &StringRecord, reference_target: &ReferenceTarget, content: &str) -> Vec<Location> {
        let Some(field_iter) = CsvFieldIter::new(record, content) else { return vec![]; };
        let mut locations = vec![];
        let module_name = module.map(|m| session.st()[m].name.clone()).unwrap_or_else(|| Sy!(""));
        for (idx, (start, end, field)) in field_iter.enumerate() {
            let field_name = headers.get(idx).unwrap().clone();
            if field_name == "id" {
                let xml_id = if field.contains(".") {
                    oyarn!("{}", field)
                } else {
                    oyarn!("{}.{}", module_name, field)
                };
                let ReferenceTarget::String(search_str) = reference_target else {continue;};
                if xml_id == *search_str {
                    locations.push(Location {
                        uri: uri.clone(),
                        range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, path, &std::ops::Range {
                            start,
                            end,
                        }),
                    })
                }
            } else if field_name.ends_with(":id") {
                let xml_id = if field.contains(".") {
                    oyarn!("{}", field)
                } else {
                    oyarn!("{}.{}", module_name, field)
                };
                let ReferenceTarget::String(search_str) = reference_target else {continue;};
                if xml_id == *search_str {
                    locations.push(Location {
                        uri: uri.clone(),
                        range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, path, &std::ops::Range {
                            start,
                            end,
                        }),
                    })
                }
            }
        }
        locations
    }
}
