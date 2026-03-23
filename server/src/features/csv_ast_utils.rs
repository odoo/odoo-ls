use std::{cell::RefCell, rc::Rc};

use csv::{Reader, StringRecord};

use crate::{S, constants::{OYarn, SymType}, core::{odoo::SyncOdoo, symbols::symbol::Symbol, xml_data::{OdooData, OdooDataRecord}}, features::goto_utils::{GotoSource, GotoSourceType}, oyarn, threads::SessionInfo};



pub struct CsvAstUtils {}

impl CsvAstUtils {

    pub fn get_symbols(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, csv_reader: &mut Reader<&[u8]>, model_name: &OYarn, offset: usize) -> Vec<GotoSource> {
        let mut results = vec![];
        let module = file_symbol.borrow().find_module();
        let Some(model) = session.sync_odoo.models.get(model_name).cloned() else {return vec![];};
        let model_syms = model.borrow().get_main_symbols(session, module.clone());
        let Some(main_symbol) = model_syms.first().cloned() else {return results;};
        drop(model_syms);
        let mut headers = vec![];
        if !csv_reader.has_headers() {
            return results;
        }
        let Ok(header) = csv_reader.headers() else { return results;};
        let mut h_start = header.position().unwrap().byte() as usize;
        for h in header.iter() {
            let end = h_start + h.len() as usize;
            let has_quotes = h.starts_with('"') && h.ends_with('"') && h.len() >= 2;
            let header_txt = CsvAstUtils::remove_quotes(h);
            headers.push(oyarn!("{}", header_txt));
            if offset >= h_start && offset <= end {
                let header_elts = header_txt.splitn(2, [':', '/']).collect::<Vec<_>>();
                let symbols = main_symbol.borrow().get_member_symbol(session, &S!(header_elts[0]), module.clone(), false, true, false, true, false);
                if offset <= h_start + has_quotes as usize + header_elts[0].len() + 1 {
                    for sym in symbols.0.iter() {
                        results.push(GotoSource {
                            source: GotoSourceType::Symbol(sym.clone()),
                            origin_selection_range: None, //TODO
                        })
                    }
                } else {
                    for sym in symbols.0.iter() {
                        if sym.borrow().is_specific_field(session, &["Many2one", "One2many", "Many2many"]) && sym.borrow().typ() == SymType::VARIABLE{
                            let models = sym.borrow().as_variable().get_relational_model(session, module.clone());
                            if models.len() == 1 {
                                let model = models[0].clone();
                                let sub_symbols = model.borrow().get_member_symbol(session, &S!(header_elts[1]), module.clone(), false, true, false, true, false);
                                for sym in sub_symbols.0.iter() {
                                    results.push(GotoSource {
                                        source: GotoSourceType::Symbol(sym.clone()),
                                        origin_selection_range: None, //TODO
                                    })
                                }
                            }
                        }
                    }
                }
            }
            h_start = end + 1;
        }
        if headers.contains(&oyarn!("id")) {
            for record in csv_reader.records().filter_map(Result::ok) {
                    CsvAstUtils::get_symbols_in_record(session, offset, &headers, &record, &mut results, &file_symbol, model_name.clone(), &main_symbol, module.clone());
            }
        }
        results
    }

    fn get_symbols_in_record(session: &mut SessionInfo, offset: usize, headers: &Vec<OYarn>, record: &StringRecord, results: &mut Vec<GotoSource>, csv_symbol: &Rc<RefCell<Symbol>>, model_name: OYarn, main_symbol: &Rc<RefCell<Symbol>>, module: Option<Rc<RefCell<Symbol>>>) {
        if record.position().is_none() {
            return;
        }
        let mut start = record.position().unwrap().byte();
        let mut idx = 0;
        for field in record.iter(){
            let end = start + field.len() as u64;
            if start > offset as u64 || end < offset as u64 {
                start = end + 1;
                idx +=1 ;
                continue;
            }
            let field_data = oyarn!("{}", CsvAstUtils::remove_quotes(field));
            if headers.len() <= idx {
                break;
            }
            let field_elts = headers.get(idx).unwrap().splitn(2, [':', '/']).collect::<Vec<_>>();
            let field_name = field_elts[0];
            let relational_field = field_elts.get(1);
            if field_name == "id" {
                results.push(GotoSource {
                    source: GotoSourceType::OdooData(OdooData::RECORD(OdooDataRecord {
                        symbol: Rc::downgrade(csv_symbol),
                        model: (model_name.clone(), std::ops::Range::<usize> {
                            start: 0,
                            end: 1,
                        }),
                        xml_id: Some(field_data),
                        fields: vec![],
                        range: core::ops::Range{
                            start: record.position().unwrap().byte() as usize,
                            end: end as usize
                        },
                    })),
                    origin_selection_range: None,
                })
            } else if let Some(relational_field) = relational_field {
                // 1. find relational field in current model
                let field_syms = main_symbol.borrow().get_member_symbol(session, &S!(field_name), module.clone(), false, true, false, true, false).0;
                for field_sym in field_syms.iter() {
                    if field_sym.borrow().is_specific_field(session, &["Many2one", "One2many", "Many2many"])
                        && field_sym.borrow().typ() == SymType::VARIABLE
                    {
                        // 2. find related model
                        let related_models = field_sym.borrow().as_variable().get_relational_model(session, module.clone());
                        for related_model in related_models.iter() {
                            // 3. find related field in related model
                            let related_syms = related_model.borrow().get_member_symbol(session, &S!(*relational_field), module.clone(), false, true, false, true, false).0;
                            // 4. push result in results
                            if !related_syms.is_empty() {
                                results.extend(SyncOdoo::get_xml_ids(session,
                                    &csv_symbol,
                                    field_data.as_str(),
                                    &std::ops::Range::default(), //we don't care about range as it's used only for diagnostic
                                    &mut vec![]
                                ).iter().map(|odoo_data| GotoSource {
                                    source: GotoSourceType::OdooData(odoo_data.clone()),
                                    origin_selection_range: None,
                                }));
                            }
                        }
                    }
                }
            }
            start = end + 1;
            idx +=1 ;
        }
    }

    pub fn remove_quotes(s: &str) -> String {
        if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
            s[1..s.len()-1].to_string()
        } else {
            s.to_string()
        }
    }
}