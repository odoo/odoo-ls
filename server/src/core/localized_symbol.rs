use std::{cell::RefCell, rc::{Rc, Weak}};

use ruff_text_size::TextRange;

use crate::constants::LocSymType;

use super::{evaluation::{Evaluation, SymbolRef}, model::ModelData, symbol::Symbol, symbol_location::SymbolLocation, symbols::{class_symbol::ClassSymbol, function_symbol::FunctionSymbol}};


#[derive(Debug)]
pub struct LocalizedSymbol {
    pub loc_sym_type: LocSymType,
    pub symbol: Weak<RefCell<Symbol>>, //owner
    pub evaluations: Vec<Evaluation>, //Vec, because sometimes a single allocation can be ambiguous, like ''' a = "5" if X else 5 '''
    pub range: TextRange,
    pub is_import_variable: bool,
    pub doc_string: Option<String>,
    pub ast_indexes: Option<Vec<u16>>, //list of index to reach the corresponding ast node from file ast

    pub _function: Option<FunctionSymbol>,
    pub _class: Option<ClassSymbol>,
    pub _model: Option<ModelData>,
}

impl LocalizedSymbol {
    pub fn new(owner: Weak<RefCell<Symbol>>, loc_sym_type: LocSymType, range: TextRange) -> Self {
        Self {
            symbol: owner,
            loc_sym_type,
            range: range,
            is_import_variable: false,
            doc_string: None,
            ast_indexes: None,
            evaluations: vec![],

            _function: None,
            _class: None,
            _model: None,
        }
    }

    pub fn get_module_sym(&self) -> Option<Rc<RefCell<Symbol>>> {
        self.symbol.upgrade().unwrap().borrow().get_module_sym().clone()
    }

    pub fn is_type_alias(&self) -> bool {
        return self.evaluations.len() >= 1 && self.evaluations.iter().all(|&x| !x.symbol.instance) && !self.is_import_variable;
    }

    pub fn to_symbol_ref(&self) -> SymbolRef {
        SymbolRef::from(self)
    }
}

//to iter through all last LocalizedSymbol of all symbols in a Symbol
pub struct LocalizedSymbolIter<'a> {
    outer_iter: std::collections::hash_map::Values<'a, String, Rc<RefCell<Symbol>>>,
    inner_iter: Option<std::slice::Iter<'a, Rc<RefCell<LocalizedSymbol>>>>,
    position: u32
}

impl<'a> LocalizedSymbolIter<'a> {
    pub fn new(symbol_location: &'a SymbolLocation, position: u32) -> Self {
        let mut outer_iter = symbol_location.symbols().values();
        let inner_iter = outer_iter.next().map(|symbol| symbol.borrow().get_loc_sym(position).iter());

        LocalizedSymbolIter { outer_iter, inner_iter, position}
    }
}

impl<'a> Iterator for LocalizedSymbolIter<'a> {
    type Item = &'a Rc<RefCell<LocalizedSymbol>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ref mut inner) = self.inner_iter {
                if let Some(loc_sym) = inner.next() {
                    return Some(loc_sym);
                }
            }
            self.inner_iter = self.outer_iter.next().map(|symbol| symbol.borrow().get_loc_sym(self.position).iter());
            if self.inner_iter.is_none() {
                return None;
            }
        }
    }
}