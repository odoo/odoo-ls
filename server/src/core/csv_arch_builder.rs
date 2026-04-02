use std::{collections::HashSet, path::PathBuf};

use csv::StringRecord;
use lsp_types::Diagnostic;
use tracing::{error};

use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::{symbols::{dependency_mgr::Buildable, symbol_keys::{CsvFileKey, SymbolKey, Weak}}, xml_data::{OdooData, OdooDataField, OdooDataRecord}},features::csv_ast_utils::CsvFieldIter, oyarn, threads::SessionInfo, weak_hash_set::WeakSet, Sy};


pub struct CsvArchBuilder {
}

impl CsvArchBuilder {

    pub fn new() -> Self {
        Self {
        }
    }

    // @arena Vec<Diagnostic> is not used by caller
    pub fn load_csv(&mut self, session: &mut SessionInfo, csv_symbol: CsvFileKey, content: &String) -> Vec<Diagnostic> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let diagnostics = vec![];
        st!()[csv_symbol].set_build_status(BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        let model_name_pb = PathBuf::from(&st!()[csv_symbol].path);
        let model_name = Sy!(model_name_pb.file_stem().unwrap().to_str().unwrap().to_string());
        let csv_module = st!().find_module(csv_symbol);
        let Some(csv_module) = csv_module else {
            return diagnostics;
        };
        let csv = &mut st!()[csv_symbol];
        let mut rdr = csv::ReaderBuilder::new().from_reader(content.as_bytes());
        if rdr.has_headers() {
            if let Ok(header) = rdr.headers() {
                for h in header.iter() {
                    csv.headers.push(oyarn!("{}", h));
                }
            }
        }
        if csv.headers.contains(&Sy!("id")) {
            for result in rdr.records() {
                match result {
                    Ok(result) => {
                        let headers = &st!()[csv_symbol].headers;
                        let record = self.extract_record(csv_symbol.into(), model_name.clone(), headers, &result, content);
                        let Some(record) = record else { continue };
                        let Some(xml_id) = record.xml_id.as_ref() else { continue };
                        let id_split = xml_id.split(".").collect::<Vec<&str>>();
                        if id_split.len() > 2 {
                            //TODO diagnostic
                            continue;
                        }
                        let mut csv_module = csv_module;
                        if id_split.len() == 2 {
                            let module_name = Sy!(id_split.first().unwrap().to_string());
                            if let Some(&m) = session.sync_odoo.modules.get(&module_name) {
                                csv_module = m.upgrade(&st!()).unwrap();
                            }
                        }
                        st!()[csv_module].xml_id_locations.entry(Sy!(id_split.last().unwrap().to_string())).or_insert_with(WeakSet::new).insert(csv_symbol.into());
                        st!()[csv_symbol].xml_ids.entry(Sy!(id_split.last().unwrap().to_string())).or_insert(vec![]).push(OdooData::RECORD(record));
                    },
                    Err(err) => {
                        error!("Could not read record in CSV file {:?}. Error: {:?}", model_name_pb, err);
                    }
                }
            }
        }
        st!()[csv_symbol].set_build_status(BuildSteps::ARCH, BuildStatus::DONE);
        session.sync_odoo.add_to_validations(csv_symbol.into());
        diagnostics
    }

    fn extract_record(&self, file_symbol: Weak<SymbolKey>, model_name: OYarn, headers: &Vec<OYarn>, record: &StringRecord, content: &String) -> Option<OdooDataRecord> {
        let field_iter = CsvFieldIter::new(record, content)?;
        let mut fields = vec![];
        let mut last_end = 0;
        let mut xml_id = None;
        for (idx, (start, end, field)) in field_iter.enumerate() {
            let field_name = headers.get(idx).unwrap().clone();
            if field_name == "id" {
                xml_id = Some(oyarn!("{}", field));
            }
            fields.push(
                OdooDataField {
                    name: field_name,
                    range: core::ops::Range {
                        start: start as usize,
                        end: end as usize,
                    },
                    text: Some(field.to_string()),
                    text_range: Some(core::ops::Range {
                        start: start as usize,
                        end: end as usize,
                    }),
                    ref_key: None,
                }
            );
            last_end = end;
        }
        Some(OdooDataRecord {
            symbol: file_symbol,
            fields: fields,
            model: (model_name, core::ops::Range {
                start: 0 as usize,
                end: 1 as usize
            }),
            xml_id: xml_id,
            range: core::ops::Range{
                start: record.position().unwrap().byte() as usize,
                end: last_end as usize
            }
        })
    }
}
