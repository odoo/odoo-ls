use csv::{Reader, StringRecord};
use lsp_types::Range;
use ruff_text_size::{TextRange, TextSize};

use crate::core::symbols::symbol_keys::CsvFileKey;
use crate::features::goto_utils::GotoSource;
use crate::{
    constants::OYarn,
    core::{
        odoo::SyncOdoo,
        symbols::{
            symbol_keys::{ModuleKey, SymbolKey},
            storage::SymbolTable,
            VariableSymbol,
        },
    },
    oyarn,
    threads::SessionInfo,
};

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

pub struct CsvRecordIter<'a> {
    content: &'a str,
    records: csv::StringRecordsIter<'a, &'a [u8]>,
    first_record: Option<Result<StringRecord, csv::Error>>,
    // We keep a window of two records, because errors only have position
    // not range. So to give a range for an error we need to know where the next record starts.
    // If the error is on the last record, we give a range until the end of the file.
    second_record: Option<Result<StringRecord, csv::Error>>,
}

impl<'a> CsvRecordIter<'a> {
    pub fn new(renderer: &'a mut csv::Reader<&'a [u8]>, content: &'a str) -> Self {
        let mut records = renderer.records();
        let first_record = records.next();
        let second_record = records.next();
        Self { records, content, first_record, second_record }
    }
}

impl<'a> Iterator for CsvRecordIter<'a> {
    // (start, end, record or error)
    // start and end are byte offsets in the file, used for diagnostics range
    type Item = (u32, u32, Result<StringRecord, csv::Error>);

    fn next(&mut self) -> Option<Self::Item> {
        let record = self.first_record.take()?;
        let mut start = match record {
            Ok(ref rec) => rec.position()?.byte() as u32,
            Err(ref err) => err.position()?.byte() as u32,
        };
        if self.content.as_bytes().get(start as usize) == Some(&b'\n') {
            start += 1;
        }
        let end = match self.second_record {
            Some(Ok(ref rec)) => rec.position()?.byte() as u32,
            Some(Err(ref err)) => err.position()?.byte() as u32,
            None => self.content.as_bytes().len() as u32,
        };
        self.first_record = self.second_record.take();
        self.second_record = self.records.next();
        Some((start, end, record))
    }
}

pub struct CsvAstUtils {}

impl CsvAstUtils {

    pub fn get_symbols(
        session: &mut SessionInfo,
        file_symbol: CsvFileKey,
        csv_reader: &mut Reader<&[u8]>,
        model_name: &OYarn,
        offset: usize,
        content: &str,
    ) -> Vec<GotoSource> {
        let mut results = vec![];
        let module = session.st().find_module(file_symbol);
        let Some(model) = session.sync_odoo.models.get(model_name).cloned() else {return vec![];};
        let model_syms = model.borrow().get_main_symbols(session, module);
        let Some(&main_symbol) = model_syms.first() else {return results;};
        drop(model_syms);
        let mut headers = vec![];
        if !csv_reader.has_headers() {
            return results;
        }
        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(session.st().file_path(file_symbol.into())).unwrap();
        let Ok(header) = csv_reader.headers() else { return results;};
        for (h_start, end, h) in CsvFieldIter::new(header, content).unwrap() {
            let has_quotes = end - h_start - h.len() == 2; // Range is bigger than field length by 2, meaning field is quoted
            headers.push(oyarn!("{}", h));
            if offset >= h_start && offset <= end {
                let header_elts = h.splitn(2, [':', '/']).collect::<Vec<_>>();
                let symbols = SymbolTable::get_member_symbol(session, main_symbol.into(), header_elts[0], module, false, true, false, true, false);
                if offset <= h_start + has_quotes as usize + header_elts[0].len() { // Offset is on the field name, not on the relational part
                    for sym in symbols.0 {
                        let substring_end = if header_elts.len() > 1 {
                            // start + field_name + initial quote + 1 to make end inclusive
                            (h_start + header_elts[0].len() + has_quotes as usize + 1) as u32
                        } else {
                            end as u32
                        };
                        results.push(GotoSource {
                            source: sym,
                            origin_selection_range: Some(Range {
                                start: file_info.borrow().offset_to_position(h_start as u32, session.sync_odoo.encoding),
                                end: file_info.borrow().offset_to_position(substring_end, session.sync_odoo.encoding),
                            }),
                        })
                    }
                } else { // Offset is on the relational part
                    for sym in symbols.0 {
                        let SymbolKey::Variable(variable_key) = sym else {
                            continue;
                        };
                        if !SymbolTable::is_specific_field(session, sym, &["Many2one", "One2many", "Many2many"]) {
                            continue;
                        }
                        let models = VariableSymbol::get_relational_model(variable_key, session, module);
                        if models.len() == 1 {
                            let model = models[0];
                            let sub_symbols = SymbolTable::get_member_symbol(session, model.into(), header_elts[1], module, false, true, false, true, false);
                            for sym in sub_symbols.0 {
                                // start + quotes + field name + separator
                                let substring_start = (h_start + header_elts[0].len() + has_quotes as usize + 1) as u32;
                                results.push(GotoSource {
                                    source: sym,
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
        if headers.contains(&oyarn!("id")) {
            for record in csv_reader.records().filter_map(Result::ok) {
                    CsvAstUtils::get_symbols_in_record(session, offset, &headers, &record, &mut results, file_symbol, main_symbol.into(), module, content);
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
        csv_symbol: CsvFileKey,
        main_symbol: SymbolKey,
        module: Option<ModuleKey>,
        content: &str,
    ) {
        let Some(field_iter) = CsvFieldIter::new(record, content) else { return; };
        for (idx, (start, end, field)) in field_iter.enumerate() {
            //Search for selected field
            if start > offset || end < offset {
                continue;
            }
            let field_data = oyarn!("{}", field);
            if headers.len() <= idx {
                break;
            }
            let field_elts = headers.get(idx).unwrap().splitn(2, [':', '/']).collect::<Vec<_>>();
            let field_name = field_elts[0];
            let field_range = TextRange::new(TextSize::new(start as u32), TextSize::new(end as u32));
            let relational_field = field_elts.get(1);
            //if the selected field is "id", return the xml_id record with the same id from the same csv file
            if field_name == "id" {
                for data in session.st()[csv_symbol].symbols().iter() {
                    if let Some(record_key) = data.as_xml_record_key() {
                        let record = &session.st()[record_key];
                        let Some(record_xml_id) = &record.xml_id else {continue;};
                        if record.range.contains_range(field_range) && record_xml_id.as_str() == field_data.as_str() {
                            if let Some(xml_id) = record.fields().get("id") {
                                results.push(GotoSource {
                                    source: (*xml_id).into(),
                                    origin_selection_range: None,
                                });
                                break;
                            }
                        }
                    }
                }
            //in case of relational field, return the field in the related model
            } else if let Some(&relational_field) = relational_field {
                // 1. find relational field in current model
                let field_syms = SymbolTable::get_member_symbol(session, main_symbol, field_name, module, false, true, false, true, false).0;
                for field_sym in field_syms {
                    let SymbolKey::Variable(variable_key) = field_sym else {
                        continue;
                    };
                    if !SymbolTable::is_specific_field(session, field_sym, &["Many2one", "One2many", "Many2many"]) {
                        continue;
                    }
                    // 2. find related model
                    let related_models = VariableSymbol::get_relational_model(variable_key, session, module);
                    for related_model in related_models {
                        // 3. find related field in related model
                        let related_syms = SymbolTable::get_member_symbol(session, related_model.into(), relational_field, module, false, true, false, true, false).0;
                        // 4. push result in results
                        if !related_syms.is_empty() {
                            results.extend(SyncOdoo::get_xml_ids(session,
                                csv_symbol.into(),
                                field_data.as_str(),
                                &std::ops::Range::default(), //we don't care about range as it's used only for diagnostic
                                &mut vec![]
                            ).iter_valid(session.st()).map(|xml_id| GotoSource {
                                source: xml_id.into(),
                                origin_selection_range: None,
                            }));
                        }
                    }
                }
            }
        }
    }
}
