use lsp_types::MessageType;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::constants::OYarn;
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::core::symbols::symbol_keys::{ClassKey, ModuleKey};
use crate::core::symbols::storage::SymbolTable;
use crate::threads::SessionInfo;
use crate::weak_collections::WeakSet;

use super::symbols::ModuleSymbol;

#[derive(Debug)]
pub struct ModelData {
    pub name: OYarn,
    pub inherit: Vec<OYarn>,
    pub inherits: Vec<(OYarn, OYarn)>,

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
    /// Key: compute function name, Value: field names that are computed by this function
    pub computes: HashMap<OYarn, HashSet<OYarn>>,
}

impl ModelData {
    pub fn new() -> Self {
        Self {
            name: OYarn::from(""),
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
            computes: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct Model {
    name: OYarn,
    symbols: WeakSet<ClassKey>,
    pub dependents: WeakSet<SourceFileKey>,
}

impl Model {
    pub fn new(name: OYarn, symbol: ClassKey) -> Self {
        let mut res = Self {
            name,
            symbols: WeakSet::new(),
            dependents: WeakSet::new(),
        };
        res.symbols.insert(symbol);
        res
    }

    pub fn add_symbol(&mut self, session: &mut SessionInfo, symbol: ClassKey) {
        if self.symbols.contains(&symbol) {
            return;
        }
        self.symbols.insert(symbol);
        let from_module = session.sync_odoo.symbol_table.find_module(symbol);
        self.add_dependents_to_validation(session, from_module);
    }

    pub fn remove_symbol(&mut self, session: &mut SessionInfo, symbol: ClassKey, from_module: Option<ModuleKey>) {
        self.symbols.remove(&symbol);
        self.add_dependents_to_validation(session, from_module);
    }

    pub fn get_symbols(&self, symbol_table: &SymbolTable, from_module: Option<ModuleKey>) -> Vec<ClassKey> {
        let mut symbol = Vec::new();
        for s in self.symbols.iter_valid(symbol_table) {
            let module = symbol_table.find_module(s).expect("Unreachable: Model should be declared in a module");
            let module_sym = &symbol_table[module];
            if from_module.is_none() || ModuleSymbol::is_in_deps(symbol_table, from_module.unwrap(), &module_sym.dir_name) {
                symbol.push(s);
            }
        }
        symbol
    }

    pub fn get_main_symbols(&self, session: &SessionInfo, from_module: Option<ModuleKey>) -> Vec<ClassKey> {
        let st = &session.sync_odoo.symbol_table;
        let mut res = vec![];
        let symbols = self.symbols.iter_valid(st).filter(|&key| {
            let model = st[key]._model.as_ref().unwrap();
            !model.inherit.contains(&model.name)
        });
        let Some(dependent) = from_module else {
            return symbols.collect();
        };
        for key in symbols {
            let Some(dependency) = st.find_module(key) else {
                res.push(key);
                continue;
            };
            let dependency_name = &st[dependency].dir_name;
            if ModuleSymbol::is_in_deps(st, dependent, dependency_name) {
                res.push(key);
            }
        }
        res
    }

    pub fn model_in_deps(&self, session: &mut SessionInfo, from_module: ModuleKey) -> bool {
        let st = &session.sync_odoo.symbol_table;
        for key in self.symbols.iter_valid(st) {
            let model = st[key]._model.as_ref().unwrap();
            if !model.inherit.contains(&model.name) {
                let module = st.find_module(key).unwrap();
                let dir_name = &st[module].dir_name;
                if ModuleSymbol::is_in_deps(st, from_module, dir_name) {
                    return true;
                }
            }
        }
        false
    }

    pub fn get_full_model_symbols(model_rc: Rc<RefCell<Model>>, session: &SessionInfo, from_module: ModuleKey) -> HashSet<ClassKey> {
        let st = &session.sync_odoo.symbol_table;
        let mut symbol_set  = HashSet::new();
        let mut already_in = HashSet::new();
        let mut queue = VecDeque::from([model_rc]);
        while let Some(current_model_rc) = queue.pop_front() {
            let current_model = current_model_rc.borrow();
            let symbols = current_model.get_symbols(st, Some(from_module));
            for &key in symbols.iter() {
                let Some(model_data) = &st[key]._model else {continue};
                for inherit in model_data.inherit.iter() {
                    if let Some(model) = session.sync_odoo.models.get(inherit).cloned() {
                        if !already_in.contains(&model.borrow().name) {
                            already_in.insert(model.borrow().name.clone());
                            queue.push_back(model.clone());
                        }
                    }
                }
            }
            symbol_set.extend(symbols.into_iter());
        }
        symbol_set
    }

    pub fn get_inherits_models(&self, session: &mut SessionInfo, from_module: ModuleKey) -> Vec<Rc<RefCell<Model>>> {
        let st = &session.sync_odoo.symbol_table;
        let mut res = vec![];
        let mut already_in = HashSet::new();
        let symbols = self.get_symbols(st, Some(from_module));
        for symbol_key in symbols {
            let Some(model_data) = &st[symbol_key]._model else {
                continue;
            };
            for (model_name, _field) in model_data.inherits.iter() {
                if let Some(model) = session.sync_odoo.models.get(model_name).cloned() {
                    if !already_in.contains(&model.borrow().name) {
                        res.push(model.clone());
                        already_in.insert(model.borrow().name.clone());
                    }
                }
            }
        }
        res
    }

    pub fn has_symbols(&mut self, symbol_table: &SymbolTable) -> bool {
        self.symbols.clear_invalid(symbol_table);
        !self.symbols.is_empty()
    }

    /* Return all symbols that build this model.
        It returns the symbol and an optional string that represents the module name that should be added to dependencies to be used.
        if with_inheritance is true, it will also return symbols from inherited models (NOT Base classes).
    */
    pub fn all_symbols(&self, session: &SessionInfo, from_module: Option<ModuleKey>, with_inheritance: bool) -> Vec<(ClassKey, Option<OYarn>)> {
        self.all_symbols_helper(session, from_module, with_inheritance, &mut HashSet::new())
    }

    fn all_symbols_helper(&self, session: &SessionInfo, from_module: Option<ModuleKey>, with_inheritance: bool, seen_inherited_models: &mut HashSet<OYarn>) -> Vec<(ClassKey, Option<OYarn>)> {
        // Inheriting classes carry the current model in their own `_inherit`,
        // so a naive recursion would re-walk this model's classes for every
        // such class. Mark self as visited up front (mirrors all_inherits_helper).
        if seen_inherited_models.contains(&self.name) {
            return Vec::new();
        }
        seen_inherited_models.insert(self.name.clone());
        let st = &session.sync_odoo.symbol_table;
        let mut symbols = Vec::new();
        for s in self.symbols.iter_valid(st) { // filter stale keys
            if let Some(from_module) = from_module {
                let module = st.find_module(s);
                if let Some(module) = module {
                    let dir_name = &st[module].dir_name;
                    if ModuleSymbol::is_in_deps(st, from_module, dir_name) {
                        symbols.push((s, None));
                    } else {
                        symbols.push((s, Some(dir_name.clone())));
                    }
                } else {
                    session.log_message(MessageType::WARNING, "A model should be declared in a module.".to_string());
                }
            } else {
                symbols.push((s, None));
            }
            if !with_inheritance {
                continue;
            }
            let inherited_models = &st[s]._model.as_ref().unwrap().inherit;
            for inherited_model in inherited_models.iter() {
                if !seen_inherited_models.contains(inherited_model) {
                    seen_inherited_models.insert(inherited_model.clone());
                    if let Some(model) = session.sync_odoo.models.get(inherited_model).cloned() {
                        symbols.extend(model.borrow().all_symbols_helper(session, from_module, true, seen_inherited_models));
                    }
                }
            }
        }
        symbols
    }

    pub fn all_symbols_inherits(&self, session: &SessionInfo, from_module: Option<ModuleKey>) -> (Vec<(ClassKey, Option<OYarn>)>, Vec<(ClassKey, Option<OYarn>)>) {
        let mut visited_models = HashSet::new();
        self.all_inherits_helper(session, from_module, &mut visited_models)
    }

    fn all_inherits_helper(&self, session: &SessionInfo, from_module: Option<ModuleKey>, visited_models: &mut HashSet<OYarn>) -> (Vec<(ClassKey, Option<OYarn>)>, Vec<(ClassKey, Option<OYarn>)>) {
        if visited_models.contains(&self.name) {
            return (Vec::new(), Vec::new());
        }
        visited_models.insert(self.name.clone());
        let st = &session.sync_odoo.symbol_table;
        let mut symbols = Vec::new();
        let mut inherits_symbols = Vec::new();
        for s in self.symbols.iter_valid(st) {
            if let Some(from_module) = from_module {
                let module = st.find_module(s);
                if let Some(module) = module {
                    let dir_name = &st[module].dir_name;
                    if ModuleSymbol::is_in_deps(st, from_module, dir_name) {
                        symbols.push((s, None));
                    } else {
                        symbols.push((s, Some(dir_name.clone())));
                    }
                } else {
                    session.log_message(MessageType::WARNING, "A model should be declared in a module.".to_string());
                }
            } else {
                symbols.push((s, None));
            }
            // First get results from normal inherit
            // To make sure we visit all of inherit before inherits, since it is DFS
            // Only inherits in the tree that are not already visited will be processed in the next iteration
            let model_data = st[s]._model.as_ref().unwrap();
            for inherited_model in &model_data.inherit {
                if let Some(model) = session.sync_odoo.models.get(inherited_model).cloned() {
                    let (main_result, inherits_result) = model.borrow().all_inherits_helper(session, from_module, visited_models);
                    symbols.extend(main_result);
                    inherits_symbols.extend(inherits_result);
                }
            }
            for (inherits_model, _) in &model_data.inherits {
                if let Some(model) = session.sync_odoo.models.get(inherits_model).cloned() {
                    let (main_result, inherits_result) = model.borrow().all_inherits_helper(session, from_module, visited_models);
                    // Everything that is in inherits should be added to inherits_symbols, regardless of whether
                    // it was in inherit or inherits. Since we need that distinction to later only get fields
                    inherits_symbols.extend(main_result);
                    inherits_symbols.extend(inherits_result);
                }
            }
        }
        (symbols, inherits_symbols)
    }

    pub fn add_dependent(&mut self, symbol: SourceFileKey) {
        self.dependents.insert(symbol);
    }

    pub fn add_dependents_to_validation(&self, session: &mut SessionInfo, module_change: Option<ModuleKey>) {
        for dep in self.dependents.iter_valid(session.st()) {
            let st = session.st_mut();
            st.invalidate_sub_functions(dep);
            let module = st.find_module(dep);
            if module_change.is_none() || module.is_none() || ModuleSymbol::is_in_deps(st, module.unwrap(), &st[module_change.unwrap()].dir_name) {
                session.sync_odoo.add_to_validations(dep);
            }
        }
    }

    pub fn inherits_from(&self, session: &SessionInfo, base: &Rc<RefCell<Model>>) -> bool {
        fn inner(this: &Model, session: &SessionInfo, base: &Rc<RefCell<Model>>, checked: &mut HashSet<OYarn>) -> bool {
            if checked.contains(&this.name) {
                return false;
            }
            checked.insert(this.name.clone());
            let symbol_table = &session.sync_odoo.symbol_table;
            for symbol in this.symbols.iter_valid(symbol_table) {
                let Some(model_data) = &symbol_table[symbol]._model else {continue};
                for inherit in model_data.inherit.iter() {
                    if inherit == &base.borrow().name {
                        return true;
                    }
                    if let Some(model) = session.sync_odoo.models.get(inherit).cloned() {
                        if inner(&model.borrow(), session, base, checked) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        inner(self, session, base, &mut HashSet::new())
    }
}
