use crate::{
    constants::{BuildStatus, BuildSteps, OYarn},
    core::{
        data_hooks,
        diagnostics::{create_diagnostic, DiagnosticCode},
        symbols::{
            symbol_keys::{CsvFileKey, SymbolKey, Wk},
            Buildable,
        },
        xml_data::{OdooData, OdooDataField, OdooDataRecord},
    },
    features::csv_ast_utils::{CsvFieldIter, CsvRecordIter},
    oyarn,
    threads::SessionInfo,
    Sy,
};
use csv::StringRecord;
use lsp_types::{Diagnostic, Position, Range};
use std::path::PathBuf;
use tracing::error;

pub struct CsvArchBuilder {
}

impl CsvArchBuilder {

    pub fn new() -> Self {
        Self {
        }
    }

    pub fn load_csv(&mut self, session: &mut SessionInfo, csv_symbol: CsvFileKey, content: &String) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        session.st_mut()[csv_symbol].set_build_status(BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        let model_name_pb = PathBuf::from(&session.st()[csv_symbol].path);
        let model_name = Sy!(model_name_pb.file_stem().unwrap().to_str().unwrap().to_string());
        let csv_module = session.st().find_module(csv_symbol);
        let Some(csv_module) = csv_module else {
            return diagnostics;
        };
        let csv = &mut session.st_mut()[csv_symbol];
        let mut rdr = csv::ReaderBuilder::new().from_reader(content.as_bytes());
        if rdr.has_headers() {
            if let Ok(header) = rdr.headers() {
                for h in header.iter() {
                    csv.headers.push(oyarn!("{}", h));
                }
            }
        }
        if csv.headers.contains(&Sy!("id")) {
            for (start, end, result) in CsvRecordIter::new(&mut rdr, content) {
                match result {
                    Ok(result) => {
                        let headers = &session.st()[csv_symbol].headers;
                        let record = self.extract_record(csv_symbol.into(), model_name.clone(), headers, &result, content);
                        let Some(record) = record else { continue };
                        let Some(xml_id) = record.xml_id.as_ref() else { continue };
                        let id_split = xml_id.split(".").collect::<Vec<&str>>();
                        if id_split.len() > 2 {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05051, &[xml_id.as_str()]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(start, 0), end: Position::new(end, 1) },
                                    ..diagnostic.clone()
                                });
                            }
                            continue;
                        }
                        let mut csv_module = csv_module;
                        if id_split.len() == 2 {
                            let module_name = Sy!(id_split.first().unwrap().to_string());
                            if let Some(module) = session.sync_odoo.modules.get(&module_name).and_then(|m| m.upgrade(session.st())) {
                                csv_module = module;
                            }
                        }
                        session.st_mut()[csv_module].xml_id_locations.entry(Sy!(id_split.last().unwrap().to_string())).or_default().insert(csv_symbol.into());
                        data_hooks::on_record_creation(session, csv_symbol.into(), &record);
                        session.st_mut()[csv_symbol].xml_ids.entry(Sy!(id_split.last().unwrap().to_string())).or_insert(vec![]).push(OdooData::RECORD(record));
                    },
                    Err(err) => {
                         match err.kind() {
                            csv::ErrorKind::UnequalLengths { pos: _, expected_len, len } => {
                                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05069, &[&len.to_string(), &expected_len.to_string()]) {
                                    diagnostics.push(Diagnostic {
                                        range: Range { start: Position::new(start, 0), end: Position::new(end, 1) },
                                        ..diagnostic.clone()
                                    });
                                }
                            }
                            _ => {
                                // Use OLS05070 for CSV parsing errors
                                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05070, &[&err.to_string()]) {
                                    diagnostics.push(Diagnostic {
                                        range: Range { start: Position::new(start, 0), end: Position::new(end, 0) },
                                        ..diagnostic.clone()
                                    });
                                }
                                error!("Could not read record in CSV file {:?}. Error: {:?}", model_name_pb, err);
                            }
                        }
                    }
                }
            }
        }
        session.st_mut()[csv_symbol].set_build_status(BuildSteps::ARCH, BuildStatus::DONE);
        session.sync_odoo.add_to_validations(csv_symbol);
        diagnostics
    }

    fn extract_record(&self, file_symbol: Wk<SymbolKey>, model_name: OYarn, headers: &Vec<OYarn>, record: &StringRecord, content: &String) -> Option<OdooDataRecord> {
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
