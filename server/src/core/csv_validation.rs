use std::{cell::RefCell, path::PathBuf, rc::Rc};

use csv::StringRecord;
use lsp_types::{Diagnostic, Position, Range};

use crate::{S, Sy, constants::{BuildStatus, BuildSteps, OYarn}, core::{diagnostics::{DiagnosticCode, create_diagnostic}, file_mgr::FileInfo}, features::csv_ast_utils::CsvAstUtils, oyarn, threads::SessionInfo};

use super::{symbols::{symbol::Symbol}};

pub struct CsvValidator {
}

impl CsvValidator {

    pub fn new() -> Self {
        Self {
        }
    }

    pub fn validate(&mut self, session: &mut SessionInfo, csv_symbol: Rc<RefCell<Symbol>>) {
        let mut diagnostics = vec![];
        csv_symbol.borrow_mut().set_build_status(BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&csv_symbol.borrow().get_symbol_first_path()).unwrap();
        let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
        let model_name_pb = PathBuf::from(&csv_symbol.borrow().paths()[0]);
        let model_name = Sy!(model_name_pb.file_stem().unwrap().to_str().unwrap().to_string());
        let csv_module = csv_symbol.borrow().find_module();
        let mut csv_sym = csv_symbol.borrow_mut();
        let mut rdr = csv::ReaderBuilder::new().from_reader(data.as_bytes());
        let Some(model) = session.sync_odoo.models.get(&model_name).cloned() else {
            let mut max_range = 1;
            if let Ok(headers) = rdr.headers() {
                max_range = headers.len();
            }
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05056, &[model_name.as_str()]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(0, 0), end: Position::new(max_range as u32, 0) },
                    ..diagnostic.clone()
                });
            }
            self.finalize_validation(session, &mut csv_sym, &file_info, diagnostics);
            return;
        };
        csv_sym.add_model_dependencies(&model);
        let csv = csv_sym.as_csv_file_sym_mut();
        let Some(csv_module) = &csv_module else {
            self.finalize_validation(session, &mut csv_sym, &file_info, diagnostics);
            return;
        };
        let model_main_sym = model.borrow().get_main_symbols(session, Some(csv_module.clone()));
        let Some(model_main_sym) = model_main_sym.get(0) else { return;};
        {
            if rdr.has_headers() && let Ok(header) = rdr.headers() {
                let mut start = 0;
                let mut header_is_xml = vec![false; header.len()];
                for (idx, h) in header.iter().enumerate() {
                    let end = start + field_len(h, &data, start);
                    let mut header_elts = h.splitn(2, [':', '/']).collect::<Vec<_>>();
                    header_elts[0] = header_elts[0].split("@").next().unwrap(); //remove translation if exists
                    let member_sym = model_main_sym.borrow().get_member_symbol(session, &S!(header_elts[0]), Some(csv_module.clone()), false, true, false, true, false);
                    if member_sym.0.is_empty() {
                        header_is_xml[idx] = false;
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05057, &[h, model_name.as_str()]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(start as u32, 0), end: Position::new(end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    } else {
                        let mut is_relational = false;
                        for sym in member_sym.0.iter() {
                            if sym.borrow().is_specific_field(session, &["Many2one", "One2many", "Many2many"]) &&
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
                    start = end + 1;
                }
                if csv.headers.contains(&Sy!("id")) {
                    for result in rdr.records().filter_map(Result::ok) {
                        self.validate_record(session, csv_module, &header_is_xml, &result, &mut diagnostics, &data);
                    }
                }
            }
        }
        self.finalize_validation(session, &mut csv_sym, &file_info, diagnostics);
    }

    fn finalize_validation(&self, session: &mut SessionInfo, csv_symbol: &mut Symbol, file_info: &Rc<RefCell<FileInfo>>, diagnostics: Vec<Diagnostic>) {
        csv_symbol.set_build_status(BuildSteps::VALIDATION, BuildStatus::DONE);
        file_info.borrow_mut().replace_diagnostics(BuildSteps::VALIDATION, diagnostics);
        file_info.borrow_mut().publish_diagnostics(session);
    }

    fn validate_record(&self, session: &mut SessionInfo, csv_module: &Rc<RefCell<Symbol>>, headers_is_xml: &Vec<bool>, record: &StringRecord, diagnostics: &mut Vec<Diagnostic>, data: &String) {
        if record.position().is_none() {
            return;
        }
        let mut start = record.position().unwrap().byte() as usize;
        // Account for CRLF (\r\n) as line break
        if data.as_bytes().get(start) == Some(&b'\n') {
            start += 1;
        }
        for (idx, field) in record.iter().enumerate() {
            let end = start + field_len(field, data, start);
            let Some(should_be_xml_id) = headers_is_xml.get(idx) else { break;};
            if *should_be_xml_id {
                //check that field_data is a valid xml id
                let id_split = field.split(".").collect::<Vec<&str>>();
                let mut module_name = csv_module.borrow().name().clone();
                if id_split.len() == 2 {
                    module_name = oyarn!("{}", id_split.get(0).unwrap());
                }
                if id_split.last().unwrap().len() == 0 { //if user want to set None value
                    continue;
                }
                let Some(module_symbol) = session.sync_odoo.modules.get(&module_name) else {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05003, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(start as u32, 0), end: Position::new(end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                    continue;
                };
                let Some(module) = module_symbol.upgrade() else {continue};
                let module_borrow = module.borrow();
                if module_borrow.as_module_package().xml_id_locations.get(&Sy!(id_split.last().unwrap().to_string())).is_none() {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05001, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(start as u32, 0), end: Position::new(end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            }
            start = end + 1;
        }
    }
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
