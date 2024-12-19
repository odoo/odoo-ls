use std::cell::RefCell;
use std::rc::Rc;
use std::rc::Weak;
use lsp_types::MessageType;
use weak_table::PtrWeakHashSet;
use std::collections::HashSet;

use crate::constants::BuildStatus;
use crate::constants::BuildSteps;
use crate::constants::SymType;
use crate::threads::SessionInfo;

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
        self.symbols.insert(symbol.clone());
        let from_module = symbol.borrow().find_module();
        self.add_dependents_to_validation(session, from_module);
    }

    pub fn remove_symbol(&mut self, session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>, from_module: Option<Rc<RefCell<Symbol>>>) {
        self.symbols.remove(symbol);
        self.add_dependents_to_validation(session, from_module);
    }

    pub fn get_symbols(&self, session: &mut SessionInfo, from_module: Option<Rc<RefCell<Symbol>>>) -> impl Iterator<Item= Rc<RefCell<Symbol>>> {
        let mut symbol = Vec::new();
        for s in self.symbols.iter() {
            let module = s.borrow().find_module().expect("Model should be declared in a module");
            if from_module.is_none() || ModuleSymbol::is_in_deps(session, from_module.as_ref().unwrap(), &module.borrow().as_module_package().dir_name, &mut None) {
                symbol.push(s);
            }
        }
        symbol.into_iter()
    }

    pub fn get_full_model_symbols(&self, session: &mut SessionInfo, from_module: Rc<RefCell<Symbol>>) -> impl Iterator<Item= Rc<RefCell<Symbol>>> {
        let mut symbol: PtrWeakHashSet<Weak<RefCell<Symbol>>> = PtrWeakHashSet::new();
        for s in self.symbols.iter() {
            let module = s.borrow().find_module().expect("Model should be declared in a module");
            if ModuleSymbol::is_in_deps(session, &from_module, &module.borrow().as_module_package().dir_name, &mut None) {
                symbol.insert(s);
            }
        }
        for inherit_model in self.get_inherited_models(session, Some(from_module.clone())).iter() {
            for s in inherit_model.borrow().symbols.iter() {
                let module = s.borrow().find_module().expect("Model should be declared in a module");
                if ModuleSymbol::is_in_deps(session, &from_module, &module.borrow().as_module_package().dir_name, &mut None) {
                    symbol.insert(s);
                }
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

    pub fn get_inherited_models(&self, session: &mut SessionInfo, from_module: Option<Rc<RefCell<Symbol>>>) -> Vec<Rc<RefCell<Model>>> {
        let mut res = vec![];
        let main_sym = self.get_main_symbols(session, from_module, &mut None);
        if main_sym.len() != 1 {
            return res;
        }
        if let Some(model_data) = &main_sym[0].borrow().as_class_sym()._model {
            for inherit in model_data.inherit.iter() {
                if let Some(model) = session.sync_odoo.models.get(inherit).cloned() {
                    res.push(model);
                }
            }
        }
        res
    }

    /* Return all symbols that build this model.
        It returns the symbol and an optional string that represents the module name that should be added to dependencies to be used.
    */
    pub fn all_symbols(&self, session: &mut SessionInfo, from_module: Option<Rc<RefCell<Symbol>>>) -> Vec<(Rc<RefCell<Symbol>>, Option<String>)> {
        let mut symbol = Vec::new();
        for s in self.symbols.iter() {
            if let Some(from_module) = from_module.as_ref() {
                let module = s.borrow().find_module();
                if let Some(module) = module {
                    if ModuleSymbol::is_in_deps(session, &from_module, &module.borrow().as_module_package().dir_name, &mut None) {
                        symbol.push((s, None));
                    } else {
                        symbol.push((s, Some(module.borrow().as_module_package().dir_name.clone())));
                    }
                } else {
                    session.log_message(MessageType::WARNING, "A model should be declared in a module.".to_string());
                }
            } else {
                symbol.push((s.clone(), None));
            }
        }
        symbol
    }

    pub fn add_dependent(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.dependents.insert(symbol.clone());
    }

    pub fn add_dependents_to_validation(&self, session: &mut SessionInfo, module_change: Option<Rc<RefCell<Symbol>>>) {
        for dep in self.dependents.iter() {
            dep.borrow_mut().invalidate_sub_functions(session);
            let module = dep.borrow().find_module();
            if module_change.is_none() || module.is_none() || ModuleSymbol::is_in_deps(session, &module.as_ref().unwrap(), &module_change.as_ref().unwrap().borrow().as_module_package().dir_name, &mut None) {
                let typ = dep.borrow().typ().clone();
                match typ {
                    SymType::FUNCTION => {
                        dep.borrow_mut().set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
                        dep.borrow_mut().set_build_status(BuildSteps::ODOO, BuildStatus::PENDING);
                        session.sync_odoo.add_to_validations(dep.clone());
                    },
                    _ => {
                        session.sync_odoo.add_to_validations(dep.clone());
                    }
                }
            }
        }
    }
}
