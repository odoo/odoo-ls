use std::{cell::RefCell, rc::Rc};

use csv::{Reader, StringRecord};
use lsp_types::{Location, Uri};

use crate::{S, Sy, constants::{OYarn, SymType}, core::{file_mgr::FileMgr, symbols::symbol::Symbol}, features::{csv_ast_utils::CsvAstUtils, references::ReferenceTarget}, oyarn, threads::SessionInfo};

pub struct CsvAstReferenceVisitor {}

impl CsvAstReferenceVisitor {

    /* search for a specific symbol in headers. Not used for  */
    pub fn search_target(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, csv_reader: &mut Reader<&[u8]>, model_name: Option<&OYarn>, target: &ReferenceTarget) -> Vec<Location> {
        let mut results = vec![];
        let path = file_symbol.borrow().paths()[0].clone();
        let uri = FileMgr::pathname2uri(&path);
        let module = file_symbol.borrow().find_module();
        let mut headers = vec![];
        if csv_reader.has_headers() {
            if let Ok(header) = csv_reader.headers() {
                let mut h_start = header.position().unwrap().byte() as usize;
                for h in header.iter() {
                    let end = h_start + h.len() as usize;
                    let header_txt = CsvAstUtils::remove_quotes(h);
                    headers.push(oyarn!("{}", header_txt));
                    let header_elts = header_txt.splitn(2, [':', '/']).collect::<Vec<_>>();
                    if let ReferenceTarget::Symbol(target_sym) = &target {
                        let Some(model_name) = model_name else {continue;};
                        let Some(model) = session.sync_odoo.models.get(model_name).cloned() else {return vec![];};
                        let model_syms = model.borrow().get_main_symbols(session, module.clone());
                        let Some(main_symbol) = model_syms.first().cloned() else {return results;};
                        let symbols = main_symbol.borrow().get_member_symbol(session, &S!(header_elts[0]), module.clone(), false, true, false, true, false);
                        for sym in symbols.0.iter() {
                            if Rc::ptr_eq(target_sym, sym) {
                                results.push(Location {
                                    uri: uri.clone(),
                                    range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &path, &std::ops::Range {
                                        start: h_start as usize,
                                        end: end as usize,
                                    }),
                                });
                            } else {
                                if sym.borrow().is_specific_field(session, &["Many2one", "One2many", "Many2many"]) && sym.borrow().typ() == SymType::VARIABLE{
                                    let models = sym.borrow().as_variable().get_relational_model(session, module.clone());
                                    if models.len() == 1 {
                                        let model = models[0].clone();
                                        let sub_symbols = model.borrow().get_member_symbol(session, &S!(header_elts[1]), module.clone(), false, true, false, true, false);
                                        if !sub_symbols.0.is_empty() {
                                            results.push(Location {
                                                uri: uri.clone(),
                                                range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &path, &std::ops::Range {
                                                    start: h_start as usize,
                                                    end: end as usize,
                                                }),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    h_start = end + 1;
                }
            }
        }
        if !headers.is_empty() && headers[0] == "id" && matches!(target, ReferenceTarget::String(_)) {
            for result in csv_reader.records() {
                if let Ok(result) = result {
                    results.extend(CsvAstReferenceVisitor::search_in_record(session, module.clone(), &uri, &path, &headers, &result, target));
                }
            }
        }
        results
    }

    fn search_in_record(session: &mut SessionInfo, module: Option<Rc<RefCell<Symbol>>>, uri: &Uri, path: &String, headers: &Vec<OYarn>, record: &StringRecord, reference_target: &ReferenceTarget) -> Vec<Location> {
        if record.position().is_none() {
            return vec![];
        }
        let mut locations = vec![];
        let module_name = module.as_ref().map(|m| m.borrow().name().clone()).unwrap_or_else(|| Sy!(""));
        let mut start = record.position().unwrap().byte();
        let mut idx = 0;
        for field in record.iter() {
            let end = start + field.len() as u64;
            let field_name = headers.get(idx).unwrap().clone();
            if field_name == "id" {
                let xml_id = if field.contains(".") {
                    oyarn!("{}", CsvAstUtils::remove_quotes(field))
                } else {
                    oyarn!("{}.{}", module_name, CsvAstUtils::remove_quotes(field))
                };
                let ReferenceTarget::String(search_str) = reference_target else {continue;};
                if xml_id == *search_str {
                    locations.push(Location {
                        uri: uri.clone(),
                        range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, path, &std::ops::Range {
                            start: start as usize,
                            end: end as usize,
                        }),
                    })
                }
            } else if field_name.ends_with(":id") {
                let xml_id = if field.contains(".") {
                    oyarn!("{}", CsvAstUtils::remove_quotes(field))
                } else {
                    oyarn!("{}.{}", module_name, CsvAstUtils::remove_quotes(field))
                };
                let ReferenceTarget::String(search_str) = reference_target else {continue;};
                if xml_id == *search_str {
                    locations.push(Location {
                        uri: uri.clone(),
                        range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, path, &std::ops::Range {
                            start: start as usize,
                            end: end as usize,
                        }),
                    })
                }
            }
            start = end + 1;
            idx +=1 ;
        }
        locations
    }
}