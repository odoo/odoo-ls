use std::path::PathBuf;

use csv::StringRecord;
use lsp_types::{Diagnostic, Position, Range};
use tracing::info;

use crate::{
    Sy, constants::{BuildStatus, BuildSteps, DEBUG_STEPS, DataType, DiagnosticLevel, OYarn}, core::{
        diagnostics::{DiagnosticCode, create_diagnostic},
        file_mgr::FileInfo,
        symbols::{storage::SymbolTable, symbol_keys::{CsvFileKey, ModuleKey}},
    }, features::csv_ast_utils::CsvFieldIter, threads::SessionInfo
};
use std::{cell::RefCell, rc::Rc};

pub struct CsvValidator {
}

impl CsvValidator {

    pub fn new() -> Self {
        Self {
        }
    }

    pub fn validate(&mut self, session: &mut SessionInfo, csv_symbol: CsvFileKey) {
        let mut diagnostics = vec![];
        session.st_mut().set_build_status(csv_symbol.into(), BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
        let path = session.st()[csv_symbol].path.clone();
        if DEBUG_STEPS {
            info!("VALIDATION - CSV: {}", path);
        }
        let Some(file_info) = SymbolTable::get_file_info_for_validation(session, csv_symbol.into()) else {
            session.st_mut().set_build_status(csv_symbol.into(), BuildSteps::VALIDATION, BuildStatus::INVALID);
            return;
        };
        let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
        let model_name_pb = PathBuf::from(path);
        let model_name = model_name_pb.file_stem().unwrap().to_str().unwrap();
        let csv_module = session.st().find_module(csv_symbol);
        let mut rdr = csv::ReaderBuilder::new().from_reader(data.as_bytes());
        let Some(model) = session.sync_odoo.models.get(model_name).cloned() else {
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
            self.finalize_validation(session, csv_symbol, &file_info, diagnostics);
            return;
        };
        session.st_mut().add_model_dependencies(csv_symbol.into(), &model);
        let Some(csv_module) = csv_module else {
            self.finalize_validation(session, csv_symbol, &file_info, diagnostics);
            return;
        };
        let model_main_sym = model.borrow().get_main_symbols(session, Some(csv_module));
        let Some(&model_main_sym) = model_main_sym.get(0) else { return;};
        if rdr.has_headers() && let Ok(header) = rdr.headers() {
            let mut header_is_xml = vec![false; header.len()];
            for (idx, (start, end, h)) in CsvFieldIter::new(header, &data).unwrap().enumerate() {
                let mut header_elts = h.splitn(2, [':', '/']).collect::<Vec<_>>();
                header_elts[0] = header_elts[0].split("@").next().unwrap(); //remove translation if exists
                let member_sym = SymbolTable::get_member_symbol(session, model_main_sym.into(), header_elts[0], Some(csv_module), false, true, false, true, false);
                if member_sym.0.is_empty() {
                    header_is_xml[idx] = false;
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05057, &[h, model_name]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(start as u32, 0), end: Position::new(end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                } else {
                    let mut is_relational = false;
                    for sym in member_sym.0.iter() {
                        if SymbolTable::is_specific_field(session, *sym, &["Many2one", "One2many", "Many2many"]) &&
                            header_elts.len() == 2 && header_elts[1] == "id"
                        {
                            //on relational fields, the header must contain :id to search for xml_id.
                            //Else, it will use _rec_name and _rec_name_search to find the right record, which we are not validating
                            is_relational = true;
                            break;
                        }
                    }
                    header_is_xml[idx] = is_relational;
                }
            }
            if session.st()[csv_symbol].headers.contains(&Sy!("id")) {
                for result in rdr.records().filter_map(Result::ok) {
                    self.validate_record(session, csv_module, &header_is_xml, &result, &mut diagnostics, &data);
                }
            }
        }
        self.finalize_validation(session, csv_symbol, &file_info, diagnostics);
    }

    fn finalize_validation(&self, session: &mut SessionInfo, csv_symbol: CsvFileKey, file_info: &Rc<RefCell<FileInfo>>, diagnostics: Vec<Diagnostic>) {
        session.sync_odoo.symbol_table.set_build_status(csv_symbol.into(), BuildSteps::VALIDATION, BuildStatus::DONE);
        file_info.borrow_mut().replace_diagnostics(DiagnosticLevel::CSV_VALIDATION, diagnostics);
        file_info.borrow_mut().publish_diagnostics(session);
    }

    fn validate_record(&self, session: &mut SessionInfo, csv_module: ModuleKey, headers_is_xml: &Vec<bool>, record: &StringRecord, diagnostics: &mut Vec<Diagnostic>, data: &str) {
        let Some(field_iter) = CsvFieldIter::new(record, data) else { return; };
        for (idx, (start, end, field)) in field_iter.enumerate() {
            let Some(should_be_xml_id) = headers_is_xml.get(idx) else { break;};
            if *should_be_xml_id {
                //check that field_data is a valid xml id
                let id_split = field.split(".").collect::<Vec<&str>>();
                let mut module_name = session.st().name(csv_module).as_str();
                if id_split.len() == 2 {
                    module_name = id_split.get(0).unwrap();
                }
                if id_split.last().unwrap().len() == 0 { //if user want to set None value
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
                let complete_id = format!("{}.{}", module_name, id_split.last().unwrap());
                let Some(module) = module_symbol.upgrade(session.st()) else {continue};
                if session.st()[module].xml_ids.get(*id_split.last().unwrap()).is_none() {
                    session.st_mut()[csv_module].not_found_data_ids.insert(DataType::XML_ID(Sy!(complete_id)), BuildSteps::VALIDATION);
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
