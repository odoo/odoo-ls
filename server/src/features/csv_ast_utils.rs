use std::{cell::RefCell, rc::Rc};

use csv::{Reader, StringRecord};
use lsp_types::{Range, Position};

use crate::{S, constants::{OYarn, SymType}, core::{odoo::SyncOdoo, symbols::symbol::Symbol, xml_data::{OdooData, OdooDataRecord}}, features::goto_utils::{GotoSource, GotoSourceType}, oyarn, threads::SessionInfo};



pub struct CsvFieldIter<'a> {
    content: &'a str,
    record: &'a StringRecord,
    field_idx: usize,
    start: usize,
}

impl<'a> CsvFieldIter<'a> {
    pub fn new(record: &'a StringRecord, content: &'a str) -> Option<Self> {
        let position = record.position()?;
        let mut start = position.byte() as usize;
        // Account for CRLF (\r\n) as line break
        // Lines with CRLF will have an extra byte compared to LF (\n)
        if content.as_bytes().get(start) == Some(&b'\n') {
            start += 1;
        }
        Some(Self { content, record, field_idx: 0, start })
    }

    /// Accounts for outer double quotes in the field length, for positioning purposes.
    /// E.g.: `"abc",def,ghi` in a csv line, the first field has len 5 while the
    /// other two have len 3 each.
    fn field_len(field: &str, data_src: &str, offset: usize) -> usize {
        let mut len = field.len();
        if data_src.as_bytes().get(offset) == Some(&b'"') {
            len += 1;
        }
        if data_src.as_bytes().get(offset + len) == Some(&b'"') {
            len += 1;
        }
        len
    }
}

impl<'a> Iterator for CsvFieldIter<'a> {
    type Item = (usize, usize, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        let field = self.record.get(self.field_idx)?;
        let end = self.start + Self::field_len(field, self.content, self.start);
        let start = self.start;
        self.start = end + 1;
        self.field_idx += 1;
        Some((start, end, field))
    }
}

pub struct CsvAstUtils {}

impl CsvAstUtils {

    pub fn get_symbols(
        session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        csv_reader: &mut Reader<&[u8]>,
        model_name: &OYarn,
        offset: usize,
        content: &str,
    ) -> Vec<GotoSource> {
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
        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&file_symbol.borrow().get_symbol_first_path()).unwrap();
        let Ok(header) = csv_reader.headers() else { return results;};
        for (h_start, end, h) in CsvFieldIter::new(header, content).unwrap() {
            let has_quotes = end - h_start - h.len() == 2; // Range is bigger than field length by 2, meaning field is quoted
            headers.push(oyarn!("{}", h));
            if offset >= h_start && offset <= end {
                let header_elts = h.splitn(2, [':', '/']).collect::<Vec<_>>();
                let symbols = main_symbol.borrow().get_member_symbol(session, &S!(header_elts[0]), module.clone(), false, true, false, true, false);
                if offset <= h_start + has_quotes as usize + header_elts[0].len() { // Offset is on the field name, not on the relational part
                    for sym in symbols.0.iter() {
                        let substring_end = if header_elts.len() > 1 {
                            // start + field_name + initial quote + 1 to make end inclusive
                            (h_start + header_elts[0].len() + has_quotes as usize + 1) as u32
                        } else {
                            end as u32
                        };
                        results.push(GotoSource {
                            source: GotoSourceType::Symbol(sym.clone()),
                            origin_selection_range: Some(Range {
                                start: file_info.borrow().offset_to_position(h_start as u32, session.sync_odoo.encoding),
                                end: file_info.borrow().offset_to_position(substring_end, session.sync_odoo.encoding),
                            }),
                        })
                    }
                } else { // Offset is on the relational part
                    for sym in symbols.0.iter() {
                        if sym.borrow().is_specific_field(session, &["Many2one", "One2many", "Many2many"]) && sym.borrow().typ() == SymType::VARIABLE{
                            let models = sym.borrow().as_variable().get_relational_model(session, module.clone());
                            if models.len() == 1 {
                                let model = models[0].clone();
                                let sub_symbols = model.borrow().get_member_symbol(session, &S!(header_elts[1]), module.clone(), false, true, false, true, false);
                                for sym in sub_symbols.0.iter() {
                                    // start + quotes + field name + separator
                                    let substring_start = (h_start + header_elts[0].len() + has_quotes as usize + 1) as u32;
                                    results.push(GotoSource {
                                        source: GotoSourceType::Symbol(sym.clone()),
                                        origin_selection_range: Some(Range {
                                            start: file_info.borrow().offset_to_position(substring_start, session.sync_odoo.encoding),
                                            end: file_info.borrow().offset_to_position(end as u32, session.sync_odoo.encoding),
                                        }),
                                    })
                                }
                            }
                        }
                    }
                }
            }
        }
        if headers.contains(&oyarn!("id")) {
            for record in csv_reader.records().filter_map(Result::ok) {
                    CsvAstUtils::get_symbols_in_record(session, offset, &headers, &record, &mut results, &file_symbol, model_name.clone(), &main_symbol, module.clone(), content);
            }
        }
        results
    }

    fn get_symbols_in_record(
        session: &mut SessionInfo,
        offset: usize,
        headers: &Vec<OYarn>,
        record: &StringRecord,
        results: &mut Vec<GotoSource>,
        csv_symbol: &Rc<RefCell<Symbol>>,
        model_name: OYarn,
        main_symbol: &Rc<RefCell<Symbol>>,
        module: Option<Rc<RefCell<Symbol>>>,
        content: &str,
    ) {
        let Some(field_iter) = CsvFieldIter::new(record, content) else { return; };
        for (idx, (start, end, field)) in field_iter.enumerate() {
            if start > offset || end < offset {
                continue;
            }
            let field_data = oyarn!("{}", field);
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
                            start,
                            end,
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
        }
    }
}