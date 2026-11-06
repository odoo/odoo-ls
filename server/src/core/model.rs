use itertools::Itertools;
use slotmap::SlotMap;
use slotmap::new_key_type;
use crate::constants::BuildSteps;
use crate::core::build_scheduler::BuildScheduler;
use crate::utils::HashMap;
use crate::utils::HashSet;
use std::collections::VecDeque;

use crate::constants::OYarn;
use crate::core::symbols::symbol_keys::ModelSymbolKey;
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::core::symbols::symbol_keys::SymbolKey;
use crate::core::symbols::symbol_keys::XmlRecordKey;
use crate::core::symbols::symbol_keys::{ClassKey, ClassWithModule, ModuleKey};
use crate::core::symbols::storage::SymbolTable;
use crate::threads::SessionInfo;
use crate::weak_collections::{WeakMap, WeakSet};

use super::symbols::ModuleSymbol;
new_key_type! {
    pub struct ModelKey;
}

#[derive(Debug)]
pub struct ModelMgr {
    models_index: HashMap<OYarn, ModelKey>,
    models_arena: SlotMap<ModelKey, Model>,
}

impl Default for ModelMgr {
    fn default() -> Self {
        Self {
            models_index: HashMap::default(),
            models_arena: SlotMap::with_key(),
        }
    }
}

impl ModelMgr {
    pub fn new() -> Self {
        Self::default()
    }

    // Adds a new model, if it already exists, returns the existing one
    pub fn add_or_get_model(&mut self, model_name: &OYarn) -> ModelKey {
        if let Some(&key) = self.models_index.get(model_name) {
            return key;
        }
        let model = Model::new(model_name.clone());
        let key = self.models_arena.insert(model);
        self.models_index.insert(model_name.clone(), key);
        key
    }

    /// Drops every `Model`
    pub fn clear_models(&mut self) {
        self.models_arena.clear();
        self.models_index.clear();
    }

    pub fn iter_models(&self) -> impl Iterator<Item = (&OYarn, &ModelKey)> {
        self.models_index.iter()
    }

    pub fn get_model_key(&self, name: &str) -> Option<ModelKey> {
        self.models_index.get(name).copied()
    }

    pub fn get_model(&self, name: &str) -> Option<&Model> {
        self.models_index.get(name).map(|key| &self[*key])
    }

}

impl std::ops::Index<ModelKey> for ModelMgr {
    type Output = Model;
    // This is *only* safe for two reasons:
    // 1. Models are never removed
    // 2. We call it usually after `get_model_key`
    // Any of these changing will make this indexing potentially unsafe
    fn index(&self, k: ModelKey) -> &Model {
        &self.models_arena[k]
    }
}

impl std::ops::IndexMut<ModelKey> for ModelMgr {
    fn index_mut(&mut self, k: ModelKey) -> &mut Model {
        &mut self.models_arena[k]
    }
}

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

impl Default for ModelData {
    fn default() -> Self {
        Self::new()
    }
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
            computes: HashMap::default(),
        }
    }
}

#[derive(Debug)]
pub struct Model {
    name: OYarn,
    /// The python classes of the model, with what [`ModelClassesWalker`] needs to know about them
    classes: WeakMap<ClassKey, ModelClass>,
    /// The XML records declaring the model
    xml_symbols: WeakSet<XmlRecordKey>,
    xml_field_symbols: WeakSet<XmlRecordKey>,
    pub dependents: WeakSet<SourceFileKey>,
}

/// A class of a model, with its module and the models it points to, resolved when the class is added to the model
/// so that browsing the inheritance of a model does not have to look for them again.
#[derive(Debug)]
struct ModelClass {
    /// The module declaring the class, with its directory name to check the dependencies
    module: Option<(ModuleKey, OYarn)>,
    /// Models of `_inherit`
    inherit: Vec<ModelKey>,
    /// Models of `_inherits`
    inherits: Vec<ModelKey>,
    /// True if the class declares the model instead of extending it (see [`Model::get_main_symbols`])
    is_main: bool,
}

impl ModelClass {
    fn new(session: &mut SessionInfo, class_key: ClassKey) -> Self {
        let st = session.st();
        let module = st.find_module(class_key).map(|module| (module, st[module].dir_name.clone()));
        let (inherit, inherits, is_main) = match &st[class_key]._model {
            Some(model_data) => (
                model_data.inherit.clone(),
                model_data.inherits.iter().map(|(model_name, _field)| model_name.clone()).collect::<Vec<_>>(),
                !model_data.inherit.contains(&model_data.name),
            ),
            None => (vec![], vec![], false),
        };
        // models are never removed, so the keys stay valid, even for a model that has no class yet
        let model_mgr = session.model_mgr_mut();
        Self {
            module,
            inherit: inherit.iter().map(|model_name| model_mgr.add_or_get_model(model_name)).collect(),
            inherits: inherits.iter().map(|model_name| model_mgr.add_or_get_model(model_name)).collect(),
            is_main,
        }
    }

    fn is_visible_from(&self, symbol_table: &SymbolTable, from_module: Option<ModuleKey>) -> bool {
        match (from_module, &self.module) {
            (None, _) => true,
            (Some(from_module), Some((module, module_name))) => {
                *module == from_module || ModuleSymbol::is_in_deps(symbol_table, from_module, module_name)
            },
            (Some(_), None) => false,
        }
    }
}

impl Model {
    pub fn new(name: OYarn) -> Self {
        Self {
            name,
            classes: WeakMap::new(),
            xml_symbols: WeakSet::new(),
            xml_field_symbols: WeakSet::new(),
            dependents: WeakSet::new(),
        }
    }

    pub fn name(&self) -> &OYarn {
        &self.name
    }

    pub fn add_symbol(session: &mut SessionInfo, model_key: ModelKey, symbol: impl Into<ModelSymbolKey>) {
        let key = symbol.into();
        let is_new = match key {
            ModelSymbolKey::Class(class_key) => {
                // set even if the class is already there, its `_inherit` and `_inherits` may have changed
                let model_class = ModelClass::new(session, class_key);
                session.model_mgr_mut()[model_key].classes.insert(class_key, model_class).is_none()
            },
            ModelSymbolKey::XmlRecord(xml_key) => session.model_mgr_mut()[model_key].xml_symbols.insert(xml_key),
        };
        if !is_new {
            return;
        }
        let from_module = session.sync_odoo.symbol_table.find_module(key);
        Self::add_dependents_to_validation(session, model_key, from_module);

        if let ModelSymbolKey::XmlRecord(xml_key) = key {
            let model_name = session.model_mgr()[model_key].name.clone();
            session.st_mut().set_declared_model(xml_key, model_name);
        }
    }

    pub fn has_xml_symbols(&self, symbol_table: &SymbolTable) -> bool {
        !self.xml_symbols.is_empty(symbol_table)
    }

    pub fn add_xml_field_symbol(
        session: &mut SessionInfo,
        model_key: ModelKey,
        xml_field_symbol: XmlRecordKey,
    ) {
        let model = &mut session.model_mgr_mut()[model_key];
        if model.xml_field_symbols.contains(&xml_field_symbol) {
            return;
        }
        model.xml_field_symbols.insert(xml_field_symbol);
        let from_module = session.sync_odoo.symbol_table.find_module(xml_field_symbol);
        Self::add_dependents_to_validation(session, model_key, from_module);
    }

    pub fn remove_symbol(
        session: &mut SessionInfo,
        model_key: ModelKey,
        symbol: impl Into<ModelSymbolKey>,
        from_module: Option<ModuleKey>,
    ) {
        let key = symbol.into();
        let model = &mut session.sync_odoo.model_mgr[model_key];
        match key {
            ModelSymbolKey::Class(class_key) => {
                model.classes.remove(&class_key);
                // drop the classes removed from the symbol table without going through this function
                model.classes.clear_invalid(&session.sync_odoo.symbol_table);
            },
            ModelSymbolKey::XmlRecord(xml_key) => model.xml_symbols.remove(&xml_key),
        }
        Self::add_dependents_to_validation(session, model_key, from_module);
    }

    /// Returns all XML defined fields' symbols
    pub fn get_xml_model_field_symbols(
        &self,
        symbol_table: &SymbolTable,
        from_module: Option<ModuleKey>,
    ) -> impl Iterator<Item = XmlRecordKey> {
        Self::filter_by_module(
            self.xml_field_symbols.iter_valid(symbol_table),
            symbol_table,
            from_module,
        )
    }

    fn get_python_symbols(&self, symbol_table: &SymbolTable) -> std::vec::IntoIter<ClassKey> {
        self.classes.iter_valid(symbol_table).map(|(class_key, _)| class_key).collect::<Vec<_>>().into_iter()
    }

    /// Returns all model symbols, python classes first, then XML records
    fn get_all_symbols(&self, symbol_table: &SymbolTable) -> std::vec::IntoIter<ModelSymbolKey> {
        self.classes.iter_valid(symbol_table)
            .map(|(class_key, _)| ModelSymbolKey::from(class_key))
            .chain(self.xml_symbols.iter_valid(symbol_table).map(ModelSymbolKey::from))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn filter_by_module<T: Into<SymbolKey> + Copy>(
        iter: impl Iterator<Item = T>,
        symbol_table: &SymbolTable,
        from_module: Option<ModuleKey>,
    ) -> impl Iterator<Item = T> {
        iter.filter(move |sym| match from_module {
            None => true,
            Some(module_key) => {
                let module = symbol_table
                    .find_module((*sym).into())
                    .expect("Unreachable: Model should be declared in a module");
                ModuleSymbol::is_in_deps(symbol_table, module_key, &symbol_table[module].dir_name)
            }
        })
    }

    /// Returns all model symbols, python and XML
    pub fn get_model_symbols(
        &self,
        symbol_table: &SymbolTable,
        from_module: Option<ModuleKey>,
    ) -> impl Iterator<Item = ModelSymbolKey> {
        Self::filter_by_module(
            self.get_all_symbols(symbol_table),
            symbol_table,
            from_module,
        )
    }

    pub fn get_main_symbols(&self, session: &SessionInfo, from_module: Option<ModuleKey>) -> impl Iterator<Item = ModelSymbolKey> {
        let st = &session.sync_odoo.symbol_table;
        let main_symbols = self.classes.iter_valid(st)
            .filter(|(_, model_class)| model_class.is_main)
            .map(|(class_key, _)| ModelSymbolKey::from(class_key))
            .chain(self.xml_symbols.iter_valid(st).map(ModelSymbolKey::from))
            .sorted(); // Sort to get Classes first
        Self::filter_by_module(main_symbols, st, from_module)
    }

    pub fn model_in_deps(&self, session: &SessionInfo, from_module: ModuleKey) -> bool {
        self.get_main_symbols(session, Some(from_module))
            .next()
            .is_some()
    }

    pub fn has_symbols(&self, symbol_table: &SymbolTable) -> bool {
        self.classes.has_valid(symbol_table) || !self.xml_symbols.is_empty(symbol_table)
    }

    /// Return all (python) symbols that build this model.
    /// It returns the symbol and an optional string that represents the module name that should be added to dependencies to be used.
    pub fn all_model_classes_dependencies(
        &self,
        session: &SessionInfo,
        from_module: Option<ModuleKey>,
    ) -> impl Iterator<Item = ClassWithModule> {
        Self::attach_module_dependencies(
            self.get_python_symbols(session.st()),
            session.st(),
            from_module,
        )
    }

    fn attach_module_dependencies<T: Into<SymbolKey> + Copy>(
        iter: impl Iterator<Item = T>,
        symbol_table: &SymbolTable,
        from_module: Option<ModuleKey>,
    ) -> impl Iterator<Item = (T, Option<OYarn>)> {
        iter.map(move |sym| {
            let module = symbol_table
                .find_module(sym.into())
                .expect("Unreachable: Model should be declared in a module");
            let module_sym = &symbol_table[module];
            let dep = match from_module {
                None => None,
                Some(module_key) => {
                    if ModuleSymbol::is_in_deps(symbol_table, module_key, &module_sym.dir_name) {
                        None
                    } else {
                        Some(module_sym.dir_name.clone())
                    }
                }
            };
            (sym, dep)
        })
    }

    /// Returns the names of the fields that are computed by a given method, for all classes of the model
    pub fn get_method_computed_field_names(
        &self,
        session: &SessionInfo,
        from_module: Option<ModuleKey>,
        method_name: &str,
    ) -> HashSet<OYarn> {
        Self::filter_by_module(
            self.get_python_symbols(&session.sync_odoo.symbol_table),
            &session.sync_odoo.symbol_table,
            from_module,
        )
        .filter_map(|class| {
            let model_data = session.sync_odoo.symbol_table[class]
                ._model
                .as_ref()
                .unwrap();
            model_data.computes.get(method_name).cloned()
        })
        .flatten()
        .collect()
    }

    pub fn add_dependent(&mut self, symbol: SourceFileKey) {
        self.dependents.insert(symbol);
    }

    pub fn add_dependents_to_validation(session: &mut SessionInfo, model_key: ModelKey, module_change: Option<ModuleKey>) {
        for dep in session.model_mgr()[model_key].dependents.iter_valid(session.st()) {
            let st = session.st_mut();
            let module = st.find_module(dep);
            if module_change.is_none() || module.is_none() || ModuleSymbol::is_in_deps(st, module.unwrap(), &st[module_change.unwrap()].dir_name) {
                SymbolTable::invalidate(session, dep, BuildSteps::VALIDATION);
                BuildScheduler::queue(session, dep);
            }
        }
    }

    // Checks inherits, only needs **python classes**
    pub fn inherits_from(&self, session: &SessionInfo, base: ModelKey) -> bool {
        fn inner(this: &Model, session: &SessionInfo, base: ModelKey, checked: &mut HashSet<OYarn>) -> bool {
            if checked.contains(&this.name) {
                return false;
            }
            checked.insert(this.name.clone());
            let symbol_table = &session.sync_odoo.symbol_table;
            for class_key in this.get_python_symbols(symbol_table) {
                let Some(model_data) = &symbol_table[class_key]._model else {continue};
                for inherit in model_data.inherit.iter() {
                    if inherit == &session.model_mgr()[base].name {
                        return true;
                    }
                    if let Some(model_key) = session.model_mgr().get_model_key(inherit)
                        && inner(&session.model_mgr()[model_key], session, base, checked) {
                            return true;
                        }
                }
            }
            false
        }
        inner(self, session, base, &mut HashSet::default())
    }
}

/// Lazily browses the classes of a model, one model at a time: first the classes of the model and of all the models
/// it inherits from with `_inherit`, then the classes of the models delegated with `_inherits`,
/// whether `_inherits` is declared by the model, by a model it inherits from (ex: a mixin), or by an `_inherits` model itself.
pub struct ModelClassesWalker {
    inherit_queue: VecDeque<ModelKey>,
    inherits_queue: VecDeque<ModelKey>,
    visited: HashSet<ModelKey>,
    through_inherits: bool,
}

impl ModelClassesWalker {
    pub fn new(model_key: ModelKey) -> Self {
        Self {
            inherit_queue: VecDeque::from([model_key]),
            inherits_queue: VecDeque::new(),
            visited: HashSet::default(),
            through_inherits: false,
        }
    }

    /// Returns the classes of the next model with their module, and whether this model is reached through `_inherits`
    /// (in which case only its fields are visible).
    /// Models reached with `_inherit` are always returned before the ones reached through `_inherits`.
    pub fn next_model(&mut self, session: &SessionInfo, from_module: Option<ModuleKey>) -> Option<(Vec<(ClassKey, Option<ModuleKey>)>, bool)> {
        let st = &session.sync_odoo.symbol_table;
        let model_mgr = &session.sync_odoo.model_mgr;
        // visited is checked on pop, so that a model reached with both `_inherit` and `_inherits` is kept in the `_inherit` part
        let model_key = loop {
            let model_key = match self.inherit_queue.pop_front() {
                Some(model_key) => model_key,
                None => {
                    self.through_inherits = true;
                    self.inherits_queue.pop_front()?
                }
            };
            if self.visited.insert(model_key) {
                break model_key;
            }
        };
        let mut classes = vec![];
        for (class_key, model_class) in model_mgr[model_key].classes.iter_valid(st) {
            if !model_class.is_visible_from(st, from_module) {
                continue;
            }
            classes.push((class_key, model_class.module.as_ref().map(|(module, _)| *module)));
            for &inherit_key in model_class.inherit.iter() {
                if !self.visited.contains(&inherit_key) {
                    self.inherit_queue.push_back(inherit_key);
                }
            }
            for &inherits_key in model_class.inherits.iter() {
                if !self.visited.contains(&inherits_key) {
                    self.inherits_queue.push_back(inherits_key);
                }
            }
        }
        Some((classes, self.through_inherits))
    }
}
