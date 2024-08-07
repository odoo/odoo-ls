use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use lsp_types::Diagnostic;
use ruff_text_size::TextRange;

use crate::{constants::BuildStatus, core::evaluation::Evaluation};

use super::{symbol::MainSymbol, symbol_mgr::{SectionRange, SymbolMgr}};

#[derive(Debug)]
pub struct FunctionSymbol {
    pub name: String,
    pub is_external: bool,
    pub is_static: bool,
    pub is_property: bool,
    pub doc_string: Option<String>,
    pub ast_indexes: Vec<u16>, //list of index to reach the corresponding ast node from file ast
    pub diagnostics: Vec<Diagnostic>, //only temporary used for CLASS and FUNCTION to be collected like others are stored on FileInfo
    pub evaluations: Vec<Evaluation>, //Vec, because sometimes a single allocation can be ambiguous, like ''' a = "5" if X else 5 '''
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

impl FunctionSymbol {

    pub fn new(name: String, range: TextRange, is_external: bool) -> Self {
        let mut res = Self {
            name,
            is_external,
            weak_self: None,
            parent: None,
            range,
            is_static: false,
            is_property: false,
            diagnostics: vec![],
            ast_indexes: vec![],
            doc_string: None,
            evaluations: vec![],
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            odoo_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            sections: vec![],
            symbols: HashMap::new(),
        };
        res._init_symbol_mgr();
        res
    }

    pub fn add_symbol(&mut self, content: &Rc<RefCell<MainSymbol>>, section: u32) {
        let sections = self.symbols.entry(content.borrow().name().clone()).or_insert_with(|| HashMap::new());
        let section_vec = sections.entry(section).or_insert_with(|| vec![]);
        section_vec.push(content.clone());
    }
}