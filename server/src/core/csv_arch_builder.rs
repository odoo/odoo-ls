use std::{cell::RefCell, fs::File, rc::Rc};

use lsp_types::{Diagnostic};

use crate::{constants::{BuildStatus, BuildSteps}, threads::SessionInfo};

use super::{symbols::{symbol::Symbol}};

pub struct CsvArchBuilder {
}

impl CsvArchBuilder {

    pub fn new() -> Self {
        Self {
        }
    }

    pub fn load_csv(&mut self, session: &mut SessionInfo, csv_symbol: Rc<RefCell<Symbol>>, content: &String) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        csv_symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        //TODO load csv file
        let mut rdr = csv::Reader::from_reader(content.as_bytes());
        for result in rdr.records() {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            if let Ok(record) = result {
                println!("{:?}", record);
            }
        }
        csv_symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::DONE);
        diagnostics
    }
}