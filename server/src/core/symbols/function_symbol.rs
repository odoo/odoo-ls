use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use lsp_types::Diagnostic;

use crate::{constants::BuildStatus, core::symbol_location::SectionRange};

use super::symbol::MainSymbol;

#[derive(Debug)]
pub struct FunctionSymbol {
    pub is_static: bool,
    pub is_property: bool,
    pub diagnostics: Vec<Diagnostic>, //only temporary used for CLASS and FUNCTION to be collected like others are stored on FileInfo
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<String, Rc<RefCell<MainSymbol>>>,
    //TODO ??
}