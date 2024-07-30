use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use lsp_types::Diagnostic;
use ruff_text_size::TextRange;

use crate::{constants::BuildStatus};

use super::{symbol::MainSymbol, symbol_mgr::SectionRange};

#[derive(Debug)]
pub struct FunctionSymbol {
    pub name: String,
    pub is_external: bool,
    pub is_static: bool,
    pub is_property: bool,
    pub diagnostics: Vec<Diagnostic>, //only temporary used for CLASS and FUNCTION to be collected like others are stored on FileInfo
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub range: TextRange,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<String, HashMap<u32, Vec<Rc<RefCell<MainSymbol>>>>>,
}