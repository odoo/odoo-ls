use std::path::Path;

use csv::StringRecord;
use lsp_types::{Diagnostic, Position, Range};
use tracing::info;

use crate::{
    Sy, constants::{BuildStatus, BuildSteps, DEBUG_STEPS, DiagnosticSource, MissingDataSource, OYarn}, core::{
        diagnostics::{DiagnosticCode, create_diagnostic}, evaluation_utils::DeepFieldEvalWalker, file_mgr::{FileInfo, FileInfoKey, FileMgr}, symbols::{Buildable as _, symbol_keys::{CsvFileKey, ModuleKey}}
    }, features::csv_ast_utils::CsvFieldIter, oyarn, threads::SessionInfo
};
pub struct CsvValidator {
}

impl Default for CsvValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl CsvValidator {

    pub fn new() -> Self {
        Self {
        }
    }

    pub fn validate(&mut self, session: &mut SessionInfo, csv_symbol: CsvFileKey) {
        if !session.st().ready_for_step(csv_symbol.into(), BuildSteps::VALIDATION){
            return;
        }
        let mut diagnostics = vec![];
        session.st_mut()[csv_symbol].set_build_status(BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
        let path = session.st()[csv_symbol].path.clone();
        if DEBUG_STEPS {
            info!("VALIDATION - CSV: {}", path);
        }
        let (file_info, loaded) = FileMgr::get_or_recreate_file_info(session, csv_symbol.into());
        if !loaded {
            session.st_mut()[csv_symbol].set_build_status(BuildSteps::VALIDATION, BuildStatus::INVALID);
            return;
        }
        let Some(data) = session.file_mgr()[file_info].file_info_ast.borrow().text_document.as_ref().map(|td| td.contents().to_string()) else {
            // File can be invalid (not valid UTF-8 and so text_document is empty)
            return;
        };
        let model_name_pb = Path::new(&path);
        let model_name = model_name_pb.file_stem().unwrap().to_str().unwrap();
        let csv_module = session.st().find_module(csv_symbol);
        let mut rdr = csv::ReaderBuilder::new().from_reader(data.as_bytes());
        let Some(model_key) = session.model_mgr().get_model_key(model_name) else {
            let mut max_range = 1;
            if let Ok(headers) = rdr.headers() {
                max_range = headers.len();
            }
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05056, &[model_name]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(0, 0), end: Position::new(max_range as u32, 0) },
                    ..diagnostic.clone()
                });
            }
            self.finalize_validation(session, csv_symbol, file_info, diagnostics);
            return;
        };
        session.model_mgr_mut()[model_key].add_dependent(csv_symbol.into());
        let Some(csv_module) = csv_module else {
            self.finalize_validation(session, csv_symbol, file_info, diagnostics);
            return;
        };
        let model_main_class_sym = {
            let mut model_main_sym = session.model_mgr()[model_key].get_main_symbols(session, Some(csv_module));
            let Some(model_main_sym) = model_main_sym.find_map(|s| s.as_class_key()) else {
                return;
            };
            model_main_sym
        };
        if rdr.has_headers() && let Ok(header) = rdr.headers() {
            let mut header_is_xml = vec![false; header.len()];
            for (idx, (start, end, h)) in CsvFieldIter::new(header, &data).unwrap().enumerate() {
                let header_elts = h.splitn(2, [':', '/']).collect::<Vec<_>>();
                let field_name = header_elts[0].split("@").next().unwrap(); //remove translation if exists
                let mut deep_field_walker =
                    DeepFieldEvalWalker::new(model_main_class_sym.into(), Some(csv_module));
                let member_symbols = deep_field_walker.get_model_fields(
                    session,
                    model_main_class_sym.into(),
                    field_name,
                );
                if member_symbols.is_empty() {
                    header_is_xml[idx] = false;
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05057, &[h, model_name]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(start as u32, 0), end: Position::new(end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                } else {
                    header_is_xml[idx] = header_elts.len() == 2
                        && header_elts[1] == "id"
                        && deep_field_walker.last_field_is_relational(session);
                }
            }
            if session.st()[csv_symbol].headers.contains(&Sy!("id")) {
                for result in rdr.records().filter_map(Result::ok) {
                    self.validate_record(session, csv_module, &header_is_xml, &result, &mut diagnostics, &data);
                }
            }
        }
        self.finalize_validation(session, csv_symbol, file_info, diagnostics);
    }

    fn finalize_validation(&self, session: &mut SessionInfo, csv_symbol: CsvFileKey, file_info: FileInfoKey, diagnostics: Vec<Diagnostic>) {
        session.sync_odoo.symbol_table.set_build_status(csv_symbol.into(), BuildSteps::VALIDATION, BuildStatus::DONE);
        session.file_mgr_mut()[file_info].replace_diagnostics(DiagnosticSource::CSV_VALIDATION, diagnostics);
        FileInfo::publish_diagnostics(session, file_info);
    }

    fn validate_record(&self, session: &mut SessionInfo, csv_module: ModuleKey, headers_is_xml: &[bool], record: &StringRecord, diagnostics: &mut Vec<Diagnostic>, data: &str) {
        let Some(field_iter) = CsvFieldIter::new(record, data) else { return; };
        for (idx, (start, end, field)) in field_iter.enumerate() {
            let Some(&should_be_xml_id) = headers_is_xml.get(idx) else { break;};
            if should_be_xml_id {
                //check that field_data is a valid xml id
                let id_split = field.split(".").collect::<Vec<&str>>();
                let mut module_name = session.st().name(csv_module).as_str();
                if id_split.len() == 2 {
                    module_name = id_split.first().unwrap();
                }
                if id_split.last().unwrap().is_empty() { //if user want to set None value
                    continue;
                }
                let Some(&module_symbol) = session.sync_odoo.modules.get(module_name) else {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05003, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(start as u32, 0), end: Position::new(end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                    continue;
                };
                let complete_id = oyarn!("{}.{}", module_name, id_split.last().unwrap());
                let Some(module) = module_symbol.upgrade(session.st()) else {continue};
                if !session.st()[module].xml_ids.contains_key(*id_split.last().unwrap()) {
                    session.st_mut()[csv_module].not_found_data_ids.insert(MissingDataSource::XML_ID(complete_id), BuildSteps::VALIDATION);
                    session.sync_odoo.get_main_entry().borrow_mut().not_found_data_ids.insert(csv_module.into());
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05001, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(start as u32, 0), end: Position::new(end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            }
        }
    }
}
