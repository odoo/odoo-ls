use std::{cell::RefCell, rc::{Rc, Weak}};

use lsp_types::Diagnostic;
use ruff_text_size::TextRange;

use crate::{constants::{BuildStatus}, threads::SessionInfo};

use super::{evaluation::{Evaluation, SymbolRef}, model::ModelData, symbols::symbol::Symbol, symbol_location::SymbolLocation, symbols::{class_symbol::ClassSymbol, function_symbol::FunctionSymbol}};


#[derive(Debug)]
pub struct LocalizedSymbol {
    pub loc_sym_type: String,
    pub symbol: Weak<RefCell<Symbol>>, //owner
    pub doc_string: Option<String>,
    pub validation_status: BuildStatus,

    pub _function: Option<FunctionSymbol>,
    pub _class: Option<ClassSymbol>,
    pub _model: Option<ModelData>,
}

impl LocalizedSymbol {
    pub fn new(owner: Weak<RefCell<Symbol>>, loc_sym_type: String, range: TextRange) -> Self {
        Self {
            symbol: owner,
            symbols: None,
            loc_sym_type,
            range: range,
            is_import_variable: false,
            doc_string: None,
            ast_indexes: None,
            evaluations: vec![],
            validation_status: BuildStatus::PENDING,

            _function: None,
            _class: None,
            _model: None,
        }
    }

    pub fn get_module_sym(&self) -> Option<Rc<RefCell<Symbol>>> {
        self.symbol.upgrade().unwrap().borrow().get_module_sym().clone()
    }

    pub fn is_type_alias(&self) -> bool {
        return self.evaluations.len() >= 1 && self.evaluations.iter().all(|x| !x.symbol.instance) && !self.is_import_variable;
    }

    pub fn to_symbol_ref(&self) -> SymbolRef {
        SymbolRef::from(self)
    }

    ///Return the symbol owning this LocalizedSymbol. Panic if not available.
    pub fn symbol(&self) -> Rc<RefCell<Symbol>> {
        self.symbol.upgrade().unwrap()
    }

    ///Return last declarations of LocalizedSymbols that are in the range of this LocalizedSymbol
    pub fn get_loc_symbol(&self, names: Vec<String>) -> Vec<Rc<RefCell<LocalizedSymbol>>> {
        let symbol = self.symbol();
        let symbol = symbol.borrow();
        let child = symbol.get_symbol(&(vec![], names));
        if let Some(child) = child {
            return child.borrow().get_loc_sym(self.range.end().to_u32());
        }
        vec![]
    }
}
