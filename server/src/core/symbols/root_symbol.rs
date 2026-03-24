use crate::{constants::OYarn, core::{entry_point::EntryPoint, symbols::symbol_table::SymbolKey}, oyarn};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Debug)]
pub struct RootSymbol {
    pub name: OYarn,
    pub entry_point: Option<Rc<RefCell<EntryPoint>>>,
    pub paths: Vec<String>,
    pub module_symbols: HashMap<OYarn, SymbolKey>,
}

impl RootSymbol {

    pub fn new() -> Self {
        Self {
            name: oyarn!("Root"),
            paths: vec![],
            entry_point: None,
            module_symbols: HashMap::new(),
        }
    }

    // pub fn add_file(&mut self, file: &Rc<RefCell<Symbol>>) {
    //     file.borrow_mut().set_is_external(true);
    //     self.module_symbols.insert(file.borrow().name().clone(), file.clone());
    // }

    pub fn add_file(&mut self, file: SymbolKey, name: &str) {
        self.module_symbols.insert(oyarn!("{}", name), file);
    }

}
