use std::{cell::RefCell, path::PathBuf, rc::{Rc, Weak}};

use csv::StringRecord;
use lsp_types::Diagnostic;
use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, OYarn}, core::xml_data::{OdooData, OdooDataField, OdooDataRecord}, oyarn, threads::SessionInfo, Sy};

use super::{symbols::{symbol::Symbol}};

pub struct CsvArchBuilder {
}

impl CsvArchBuilder {

    pub fn new() -> Self {
        Self {
        }
    }

    pub fn load_csv(&mut self, session: &mut SessionInfo, csv_symbol: Rc<RefCell<Symbol>>, content: &String) -> Vec<Diagnostic> {
        let diagnostics = vec![];
        csv_symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        let model_name_pb = PathBuf::from(&csv_symbol.borrow().paths()[0]);
        let model_name = Sy!(model_name_pb.file_stem().unwrap().to_str().unwrap().to_string());
        let csv_module = csv_symbol.borrow().find_module();
        let Some(csv_module) = &csv_module else {
            return diagnostics;
        };
        {
            let mut csv_sym = csv_symbol.borrow_mut();
            let csv = csv_sym.as_csv_file_sym_mut();
            let mut rdr = csv::Reader::from_reader(content.as_bytes());
            if rdr.has_headers() {
                if let Ok(header) = rdr.headers() {
                    for h in header.iter() {
                        csv.headers.push(oyarn!("{}", h));
                    }
                }
            }
            if !csv.headers.is_empty() && csv.headers[0] == "id" {
                for result in rdr.records() {
                    if let Ok(result) = result {
                        let record = self.extract_record(Rc::downgrade(&csv_symbol), model_name.clone(), &csv.headers, &result);
                        if let Some(record) = record {
                            if let Some(xml_id) = record.xml_id.as_ref() {
                                let id_split = xml_id.split(".").collect::<Vec<&str>>();
                                if id_split.len() > 2 {
                                    //TODO diagnostic
                                    continue;
                                }
                                let mut csv_module = csv_module.clone();
                                if id_split.len() == 2 {
                                    let module_name = Sy!(id_split.first().unwrap().to_string());
                                    if let Some(m) = session.sync_odoo.modules.get(&module_name) {
                                        csv_module = m.upgrade().unwrap();
                                    }
                                }
                                csv_module.borrow_mut().as_module_package_mut().xml_id_locations.entry(Sy!(id_split.last().unwrap().to_string())).or_insert(PtrWeakHashSet::new()).insert(csv_symbol.clone());
                                csv.xml_ids.entry(Sy!(id_split.last().unwrap().to_string())).or_insert(vec![]).push(OdooData::RECORD(record));
                            }
                        }
                    }
                }
            }
        }
        csv_symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::DONE);
        diagnostics
    }

    fn extract_record(&self, file_symbol: Weak<RefCell<Symbol>>, model_name: OYarn, headers: &Vec<OYarn>, record: &StringRecord) -> Option<OdooDataRecord> {
        if record.position().is_none() {
            return None;
        }
        let mut fields = vec![];
        let mut start = record.position().unwrap().byte();
        let mut idx = 0;
        let mut last_end = 0;
        let mut xml_id = None;
        for field in record.iter(){
            let end = start + field.len() as u64;
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
            start = end + 1;
            last_end = end;
            idx +=1 ;
        }
        Some(OdooDataRecord {
            file_symbol: file_symbol,
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