use std::cell::RefCell;
use std::rc::Rc;
use std::rc::Weak;
use weak_table::PtrWeakHashSet;
use std::collections::HashSet;

use crate::threads::SessionInfo;

use super::odoo::SyncOdoo;
use super::symbols::module_symbol::ModuleSymbol;
use super::symbols::symbol::Symbol;

#[derive(Debug)]
pub struct ModelData {
    pub name: String,
    pub inherit: Vec<String>,
    pub inherits: Vec<(String, String)>,

    pub description: String,
    pub auto: bool,
    pub log_access: bool,
    pub table: String,
    pub sequence: String,
    pub sql_constraints: Vec<String>,
    pub is_abstract: bool,
    pub transient: bool,
    pub rec_name: Option<String>,
    pub order: String,
    pub check_company_auto: bool,
    pub parent_name: String,
    pub active_name: Option<String>,
    pub parent_store: bool,
    pub data_name: String,
    pub fold_name: String,
}

impl ModelData {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            inherit: Vec::new(),
            inherits: Vec::new(),
            description: String::new(),
            auto: false,
            log_access: false,
            table: String::new(),
            sequence: String::new(),
            sql_constraints: Vec::new(),
            is_abstract: false,
            transient: false,
            rec_name: None,
            order: String::from("id"),
            check_company_auto: false,
            parent_name: String::from("parent_id"),
            active_name: None,
            parent_store: false,
            data_name: String::from("date"),
            fold_name: String::from("fold"),
        }
    }
}

#[derive(Debug)]
pub struct Model {
    name: String,
    symbols: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub dependents: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
}

impl Model {
    pub fn new(name: String, symbol: Rc<RefCell<Symbol>>) -> Self {
        let mut res = Self {
            name,
            symbols: PtrWeakHashSet::new(),
            dependents: PtrWeakHashSet::new(),
        };
        res.symbols.insert(symbol);
        res
    }

    pub fn add_symbol(&mut self, session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>) {
        self.symbols.insert(symbol);
        self.add_dependents_to_validation(session);
    }

    pub fn remove_symbol(&mut self, session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>) {
        self.symbols.remove(symbol);
        self.add_dependents_to_validation(session);
    }

    pub fn get_symbols(&self, session: &mut SessionInfo, from_module: Rc<RefCell<Symbol>>) -> impl Iterator<Item= Rc<RefCell<Symbol>>> {
        let mut symbol = Vec::new();
        for s in self.symbols.iter() {
            let module = s.borrow().find_module().expect("Model should be declared in a module");
            if ModuleSymbol::is_in_deps(session, &from_module, &module.borrow().as_module_package().dir_name, &mut None) {
                symbol.push(s);
            }
        }
        symbol.into_iter()
    }

    pub fn get_main_symbols(&self, session: &mut SessionInfo, from_module: Option<Rc<RefCell<Symbol>>>, acc: &mut Option<HashSet<String>>) -> Vec<Rc<RefCell<Symbol>>> {
        if acc.is_none() {
            *acc = Some(HashSet::new());
        }
        let mut res: Vec<Rc<RefCell<Symbol>>> = vec![];
        for sym in self.symbols.iter() {
            if !sym.borrow().as_class_sym()._model.as_ref().unwrap().inherit.contains(&sym.borrow().as_class_sym()._model.as_ref().unwrap().name) {
                if from_module.is_none() || sym.as_ref().borrow().find_module().is_none() {
                    res.push(sym);
                } else {
                    let dir_name = sym.borrow().find_module().unwrap().borrow().as_module_package().dir_name.clone();
                    if (acc.is_some() && acc.as_ref().unwrap().contains(&dir_name)) ||
                    ModuleSymbol::is_in_deps(session, from_module.as_ref().unwrap(), &dir_name, acc) {
                        res.push(sym);
                        acc.as_mut().unwrap().insert(dir_name);
                    }
                }
            }
        }
        res
    }

    pub fn add_dependent(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.dependents.insert(symbol.clone());
    }

    pub fn add_dependents_to_validation(&self, session: &mut SessionInfo) {
        for dep in self.dependents.iter() {
            dep.borrow_mut().invalidate_sub_functions(session);
            session.sync_odoo.add_to_validations(dep.clone());
        }
    }
}
