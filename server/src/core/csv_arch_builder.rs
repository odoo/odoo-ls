use crate::{
    Sy, constants::{BuildStatus, BuildSteps, DEBUG_STEPS, OYarn}, core::{
        build_scheduler::BuildScheduler, data_hooks, diagnostics::{DiagnosticCode, create_diagnostic}, symbols::{
            Buildable, ModuleSymbol, symbol_keys::{CsvFileKey, XmlId, XmlRecordKey}
        },
    }, features::csv_ast_utils::{CsvFieldIter, CsvRecordIter}, oyarn, threads::SessionInfo, utils::HashSet
};
use csv::StringRecord;
use lsp_types::{Diagnostic, Position, Range};
use ruff_text_size::{TextRange, TextSize};
use std::path::PathBuf;
use tracing::{error, info};

pub struct CsvArchBuilder {
}

impl Default for CsvArchBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CsvArchBuilder {

    pub fn new() -> Self {
        Self {
        }
    }

    pub fn load_csv(&mut self, session: &mut SessionInfo, csv_symbol: CsvFileKey, content: &str) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        if !session.st().ready_for_step(csv_symbol.into(), BuildSteps::ARCH) {
            return diagnostics;
        }
        session.st_mut()[csv_symbol].set_build_status(BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        let model_name_pb = PathBuf::from(&session.st()[csv_symbol].path);
        let model_name = Sy!(model_name_pb.file_stem().unwrap().to_str().unwrap().to_string());
        let csv_module = session.st().find_module(csv_symbol);
        let Some(csv_module) = csv_module else {
            return diagnostics;
        };
        if DEBUG_STEPS {
            info!("ARCH       - CSV: {}", session.st()[csv_symbol].path);
        }
        let mut rdr = csv::ReaderBuilder::new().from_reader(content.as_bytes());
        if rdr.has_headers()
            && let Ok(header) = rdr.headers() {
                let csv = &mut session.st_mut()[csv_symbol];
                for h in header.iter() {
                    csv.headers.push(oyarn!("{}", h));
                }
                // Look for duplicate column (field) names. Odoo keeps the last one, so we issue a warning for the one getting overridden.
                let columns: Vec<_> = CsvFieldIter::new(header, content).into_iter().flatten().collect();
                let mut seen = HashSet::default();
                for (start, end, name) in columns.into_iter().rev() {
                    if !seen.insert(name)
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05076, &[name]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(start as u32, 0), end: Position::new(end as u32, 0) },
                                ..diagnostic
                            });
                        }
                }
            }
        if session.st()[csv_symbol].headers.contains(&Sy!("id")) {
            for (start, end, result) in CsvRecordIter::new(&mut rdr, content) {
                match result {
                    Ok(result) => {
                        let record = self.extract_record(session, csv_symbol, model_name.clone(), &result, content);
                        let Some(record_key) = record else { continue };
                        let Some(xml_id) = session.st()[record_key].xml_id.clone() else { continue };
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
                            let module_name = *id_split.first().unwrap();
                            if let Some(module) = session.sync_odoo.modules.get(module_name).and_then(|m| m.upgrade(session.st())) {
                                csv_module = module;
                            }
                        }
                        data_hooks::on_record_creation(session, csv_symbol.into(), record_key);
                        ModuleSymbol::insert_xml_id(session.st_mut(), csv_module, Sy!(id_split.last().unwrap().to_string()), XmlId::XmlRecord(record_key));
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
        session.st_mut().set_build_status(csv_symbol.into(), BuildSteps::ARCH, BuildStatus::DONE);
        BuildScheduler::queue(session, csv_symbol);
        diagnostics
    }

    fn extract_record(&self, session: &mut SessionInfo, file_symbol: CsvFileKey, model_name: OYarn, record: &StringRecord, content: &str) -> Option<XmlRecordKey> {
        let fields: Vec<_> = CsvFieldIter::new(record, content)?.collect();
        let last_end = fields.last().map_or(0, |(_, end, _)| *end);
        let mut xml_id = None;
        let record_key = session.st_mut().add_new_xml_record(
            file_symbol.into(),
            (model_name, core::ops::Range {
                start: 0_usize,
                end: 1_usize
            }),
            None, //dummy
            TextRange::new(TextSize::new(0), TextSize::new(0_u32)) //dummy
        );
        let headers = &session.st()[file_symbol].headers.clone();
        // Iterate in reverse order so that the last occurrence of a field is the one that is stored
        for (idx, (start, end, field)) in fields.into_iter().enumerate().rev() {
            let field_name = headers.get(idx).unwrap();
            match session.st_mut().add_new_xml_field(record_key.into(),
                field_name,
                TextRange::new(TextSize::new(start as u32), TextSize::new(end as u32)),
                Some(field.to_string()),
                Some(TextRange::new(TextSize::new(start as u32), TextSize::new(end as u32))),
                None
            ) {
                Ok(_) => {
                    if field_name == "id" {
                        xml_id = Some(oyarn!("{}", field));
                    }
                },
                Err(_) => {
                    // field already stored, this one gets overriden.
                    // Diagnostic issued on the header in `load_csv`.
                }
            };
        }
        let rec = &mut session.st_mut()[record_key];
        rec.xml_id = xml_id;
        rec.range = TextRange::new(TextSize::new(record.position().unwrap().byte() as u32), TextSize::new(last_end as u32));
        Some(record_key)
    }
}
