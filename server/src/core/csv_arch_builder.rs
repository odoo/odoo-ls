use std::{cell::RefCell, rc::Rc};

use lsp_types::{Diagnostic};

use crate::{constants::{BuildStatus, BuildSteps, OYarn, EXTENSION_NAME}, oyarn, threads::SessionInfo, S};

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
        csv_symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::DONE);
        diagnostics
    }
}