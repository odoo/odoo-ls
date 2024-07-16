use ruff_text_size::TextRange;

use crate::{constants::SymType, core::evaluation::Evaluation, threads::SessionInfo};
use std::{cell::{RefCell, RefMut}, rc::Weak};

use super::symbol::Symbol;

#[derive(Debug)]
pub struct VariableSymbol {
    pub name: String,
    pub is_external: bool,
    pub doc_string: Option<String>,
    pub ast_indexes: Vec<u16>, //list of index to reach the corresponding ast node from file ast
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub is_import_variable: bool,
    pub is_parameter: bool,
    pub evaluations: Vec<Evaluation>, //Vec, because sometimes a single allocation can be ambiguous, like ''' a = "5" if X else 5 '''
    pub range: TextRange,
}

impl VariableSymbol {

    pub fn new(name: String, range: TextRange, is_external: bool) -> Self {
        Self {
            name,
            is_external,
            doc_string: None,
            ast_indexes: vec![],
            weak_self: None,
            parent: None,
            range,
            is_import_variable: false,
            is_parameter: false,
            evaluations: vec![],
        }
    }

    pub fn is_type_alias(&self) -> bool {
        return self.evaluations.len() >= 1 && self.evaluations.iter().all(|x| !x.symbol.instance) && !self.is_import_variable;
    }

}