use std::path::Path;
use std::{cell::RefCell, cmp, path::PathBuf, rc::Rc, u32};
use crate::constants::MissingDataSource;
use crate::utils::HashMap;

use slotmap::Key;
use tracing::{error, info, warn};

use crate::core::symbols::symbol_keys::{FileKey, JsFileKey, RootKey, SourceFileKey, SymbolKey, Wk};
use crate::{
    tree::Tree,
    constants::{BuildSteps, OYarn},
    core::symbols::storage::SymbolTable,
    threads::SessionInfo,
    utils::PathSanitizer,
    warn_or_panic,
    weak_collections::WeakSet,
};

use super::odoo::SyncOdoo;

#[derive(Debug)]
pub struct EntryPointMgr {
    pub builtins_entry_points: Vec<Rc<RefCell<EntryPoint>>>,
    pub public_entry_points: Vec<Rc<RefCell<EntryPoint>>>,
    pub main_entry_point: Option<Rc<RefCell<EntryPoint>>>,
    pub addons_entry_points: Vec<Rc<RefCell<EntryPoint>>>,
    pub custom_entry_points: Vec<Rc<RefCell<EntryPoint>>>,
    pub untitled_entry_points: Vec<Rc<RefCell<EntryPoint>>>,
}

impl Default for EntryPointMgr {
    fn default() -> Self {
        Self::new()
    }
}

impl EntryPointMgr {

    pub fn new() -> Self {
        Self {
            builtins_entry_points: vec![],
            public_entry_points: vec![],
            main_entry_point: None,
            addons_entry_points: vec![],
            custom_entry_points: vec![],
            untitled_entry_points: vec![],
        }
    }
    /// Create a new entry for an untitled (in-memory) file.
    /// Returns the file symbol for the untitled entry.
    pub fn add_entry_to_untitled(session: &mut SessionInfo, path: String) -> FileKey {
        // For untitled files, we use a minimal tree: just the name as a single OYarn
        info!("Adding new untitled entry point: {}", path);
        let tree = vec![OYarn::from(path.clone())];
        let entry = EntryPoint::new(
            session.st_mut(),
            path.clone(),
            tree,
            EntryPointType::UNTITLED,
            None,
            None,
        );
        session.sync_odoo.entry_point_mgr.borrow_mut().untitled_entry_points.push(entry.clone());
        // Create one file symbol under the root for the untitled file
        let name: String = PathBuf::from(&path).with_extension("").components().next_back().unwrap().as_os_str().to_str().unwrap().to_string();

        session.st_mut().add_new_file(entry.borrow().root.into(), &name, &path)
    }

    /**
     * Create each required directory symbols for a given path.
     * /!\ path must point to a directory on disk */
    pub fn create_dir_symbols_from_path_to_entry(session: &mut SessionInfo, path: &Path, entry: Rc<RefCell<EntryPoint>>) -> Option<SymbolKey> {
        let mut iter_path = PathBuf::new();
        let mut current_sym: SymbolKey = entry.borrow().root.into();
        let component_count = path.components().count();
        for component in path.components().take(component_count - 1) {
            iter_path.push(component);
            if let Some(name) = component.as_os_str().to_str() {
                let sym = session.st().get_module_symbol(current_sym, name);
                if let Some(existing_sym) = sym {
                    current_sym = existing_sym;
                } else {
                    let disk_dir = session.st_mut().add_new_disk_dir(current_sym, name, iter_path.to_str().unwrap());
                    current_sym = disk_dir.into();
                }
            } else {
                error!("Unable to convert path component to string");
                return None;
            }
        }
        SymbolTable::create_from_path(session, path, current_sym, false)
    }

    /* Create a new main entry_point.
    return the disk_dir symbol of the last FOLDER of the path
     */
    pub fn set_main_entry(session: &mut SessionInfo, path: String) -> Option<SymbolKey> {
        info!("Setting Main entry point: {}", path);
        let entry_point_tree = PathBuf::from(&path).to_tree();
        let entry = EntryPoint::new(
            &mut session.sync_odoo.symbol_table,
            path.clone(),
            entry_point_tree.flatten(),
            EntryPointType::MAIN,
            None,
            None);
        session.sync_odoo.entry_point_mgr.borrow_mut().main_entry_point = Some(entry.clone());
        
        EntryPointMgr::_create_dir_symbols_for_new_entry(session, &path, entry)
    }

    /* Create a new entry to builtins.
    return the disk_dir symbol of the last FOLDER of the path
     */
    pub fn add_entry_to_builtins(session: &mut SessionInfo, path: String) -> Option<SymbolKey> {
        info!("Adding new builtins entry point: {}", path);
        let entry_point_tree = PathBuf::from(&path).to_tree();
        let entry = EntryPoint::new(
            &mut session.sync_odoo.symbol_table,
            path.clone(),
            entry_point_tree.flatten(),
            EntryPointType::BUILTIN,
            None,
            None);
        session.sync_odoo.entry_point_mgr.borrow_mut().builtins_entry_points.push(entry.clone());
        
        EntryPointMgr::_create_dir_symbols_for_new_entry(session, &path, entry)
    }

    /* Create a new entry to public.
    return the disk_dir symbol of the last FOLDER of the path
     */
    pub fn add_entry_to_public(session: &mut SessionInfo, path: String) -> Option<SymbolKey> {
        info!("Adding new public entry point: {}", path);
        //Prevent adding entry point from sys.path or other config that is matching odoo or addons paths
        if let Some(odoo_path) = &session.sync_odoo.config.odoo_path()
            && &path == odoo_path
        {
            warn!("Public entry point {} is equal to odoo path {}, this is not supported and will be ignored", path, odoo_path);
            return None;
        }
        for addon_path in &session.sync_odoo.config.addons_paths() {
            if &path == addon_path {
                warn!("Public entry point {} is equal to addon path {}, this is not supported and will be ignored", path, addon_path);
                return None;
            }
        }
        let entry_point_tree = PathBuf::from(&path).to_tree();
        let entry = EntryPoint::new(
            &mut session.sync_odoo.symbol_table,
            path.clone(),
            entry_point_tree.flatten(),
            EntryPointType::PUBLIC,
            None,
            None);
        session.sync_odoo.entry_point_mgr.borrow_mut().public_entry_points.push(entry.clone());
        
        EntryPointMgr::_create_dir_symbols_for_new_entry(session, &path, entry)
    }

    /* Create a new entry to addons.
     * This function, unlike its siblings, does not a produce a symbol.
     */
    pub fn add_entry_to_addons(session: &mut SessionInfo, path: String, main_entry: Rc<RefCell<EntryPoint>>, added_tree: Vec<OYarn>) {
        info!("Adding new addon entry point: {}", path);
        let entry_point_tree = PathBuf::from(&path).to_tree();
        let addon_to_odoo_path = Some(main_entry.borrow().path.clone() + "/" + added_tree.join("/").as_str());
        let addon_to_odoo_tree = Some(main_entry.borrow().tree.iter().chain(&added_tree).cloned().collect());
        let shared_root = main_entry.borrow().root;
        let entry = EntryPoint::new_with_shared_root(
            path,
            entry_point_tree.flatten(),
            EntryPointType::ADDON,
            addon_to_odoo_path,
            addon_to_odoo_tree,
            shared_root
        );
        session.sync_odoo.entry_point_mgr.borrow_mut().addons_entry_points.push(entry.clone());
    }

    /* Create a new entry to public.
    return the symbol at the end of the path
     */
    pub fn add_entry_to_customs(session: &mut SessionInfo, path: &str) -> Option<SymbolKey> {
        info!("Adding new custom entry point: {}", path);
        let entry_point_tree = PathBuf::from(path).to_tree();
        let entry = EntryPoint::new(
            &mut session.sync_odoo.symbol_table,
            path.to_string(),
            entry_point_tree.flatten(),
            EntryPointType::CUSTOM,
            None,
            None);
        session.sync_odoo.entry_point_mgr.borrow_mut().custom_entry_points.push(entry.clone());
        EntryPointMgr::_create_dir_symbols_for_new_entry(session, path, entry)
    }

    fn _create_dir_symbols_for_new_entry(session: &mut SessionInfo, path: &str, entry: Rc<RefCell<EntryPoint>>) -> Option<SymbolKey> {
        EntryPointMgr::create_dir_symbols_from_path_to_entry(session, Path::new(path), entry)
    }

    /// Create a new custom entry point for a given tree path and file path.
    /// tree_path can possibly be the path stripped from __manifest__/__init__.py
    pub fn create_new_custom_entry_for_path(session: &mut SessionInfo, tree_path: &str, file_path: &str) -> bool {
        let new_sym = EntryPointMgr::add_entry_to_customs(session, tree_path);
        if let Some(new_sym) = new_sym {
            session.st_mut().set_is_external(new_sym, false);
            match new_sym {
                SymbolKey::PythonPackage(p) => {
                    session.st_mut()[p].self_import = true;
                },
                SymbolKey::File(f) => {
                    session.st_mut()[f].self_import = true;
                },
                SymbolKey::Namespace(n) => {
                    if file_path.ends_with("__manifest__.py") {
                        warn!("new custom entry point for manifest without related init.py is not supported outside of main entry point. skipping...");
                        session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(&mut session.sync_odoo.symbol_table, tree_path);
                    } else {
                        // There was an __init__.py, that was renamed or deleted.
                        // Another notification will come for the deletion of the file, so we just warn here.
                        warn_or_panic!("Trying to create a custom entrypoint on a namespace symbol: {:?}", session.st()[n].paths());
                    }
                    return false;
                }
                SymbolKey::JsFile(f) => {
                    session.st_mut()[f].self_import = true;
                    //arch of js files is done in build_ast of file_info, so we have to directly reload validations instead
                    SyncOdoo::add_to_validations(session.sync_odoo, new_sym);
                    return true;
                }
                _ => {panic!("Unexpected symbol type: {:?}", new_sym);}
            }
            SyncOdoo::add_to_rebuild_arch(session.sync_odoo, new_sym);
        }
        true
    }

    pub fn create_new_untitled_entry_for_path(session: &mut SessionInfo, file_name: &str) -> bool {
        let new_sym = EntryPointMgr::add_entry_to_untitled(session, file_name.to_string());
        session.sync_odoo.symbol_table[new_sym].self_import = true;
        SyncOdoo::add_to_rebuild_arch(session.sync_odoo, new_sym);
        true
    }

    pub fn iter_for_import(&self, current_entry: &Rc<RefCell<EntryPoint>>) -> Box<dyn Iterator<Item = &Rc<RefCell<EntryPoint>>> + '_> {
        let mut is_main = false;
        for entry in self.iter_main() {
            if Rc::ptr_eq(current_entry, entry) {
                is_main = true;
                break;
            }
        }
        if is_main {
            Box::new(self.addons_entry_points.iter().chain(
            self.main_entry_point.iter()).chain(
            self.builtins_entry_points.iter()).chain(
            self.public_entry_points.iter()))
        } else {
            Box::new(self.custom_entry_points.iter().chain(
            self.builtins_entry_points.iter()).chain(
            self.public_entry_points.iter()))
        }
    }

    pub fn iter_all(&self) -> impl Iterator<Item = &Rc<RefCell<EntryPoint>>> {
        self.addons_entry_points.iter()
            .chain(self.main_entry_point.iter())
            .chain(self.builtins_entry_points.iter())
            .chain(self.public_entry_points.iter())
            .chain(self.custom_entry_points.iter())
            .chain(self.untitled_entry_points.iter())
    }

    //iter through all main entry points, sorted by tree length (from bigger to smaller)
    pub fn iter_main(&self) -> impl Iterator<Item = &Rc<RefCell<EntryPoint>>>
    {
        let mut collected = self.main_entry_point.iter().chain(self.addons_entry_points.iter()).collect::<Vec<_>>();
        collected.sort_by(|x, y| y.borrow().tree.len().cmp(&x.borrow().tree.len()));
        collected.into_iter()
    }

    pub fn iter_all_but_main(&self) -> impl Iterator<Item = &Rc<RefCell<EntryPoint>>> {
        self.builtins_entry_points.iter()
        .chain(self.public_entry_points.iter())
        .chain(self.custom_entry_points.iter())
        .chain(self.untitled_entry_points.iter())
    }

    pub fn iter_all_but_public(&self) -> impl Iterator<Item = &Rc<RefCell<EntryPoint>>> {
        self.main_entry_point.iter().chain(
        self.addons_entry_points.iter()).chain(
        self.custom_entry_points.iter()
        )
    }

    pub fn reset_entry_points(&mut self, symbol_table: &mut SymbolTable, with_custom_entries: bool) {
        self.builtins_entry_points.drain(..).for_each(|ep| Self::drop_entry(symbol_table, &ep));
        self.public_entry_points.drain(..).for_each(|ep| Self::drop_entry(symbol_table, &ep));
        if let Some(main_ep) = &self.main_entry_point {
            Self::drop_entry(symbol_table, main_ep);
            self.main_entry_point = None;
            // addons entries share the same root as the main entry
            self.addons_entry_points.clear();
        }
        if with_custom_entries {
            self.custom_entry_points.drain(..).for_each(|ep| Self::drop_entry(symbol_table, &ep));
        }
    }

    pub fn remove_entries_with_path(&mut self, symbol_table: &mut SymbolTable, path: &str) {
        for entry in self.iter_all() {
            if (entry.borrow().typ == EntryPointType::UNTITLED && entry.borrow().path == *path)
            || (entry.borrow().typ != EntryPointType::UNTITLED
            && PathBuf::from(entry.borrow().path.clone()).starts_with(path)){  //delete any entrypoint that would be in a subdirectory too
                entry.borrow_mut().to_delete = true;
            }
        }
        self.clean_entries(symbol_table);
    }

    pub fn check_custom_entry_to_delete_with_path(&mut self, path: &str) {
        for entry in self.custom_entry_points.iter() {
            if entry.borrow().path == *path {
                entry.borrow_mut().to_delete = true;
            }
        }
    }

    pub fn clean_entries(&mut self, symbol_table: &mut SymbolTable) {
        if let Some(main) = self.main_entry_point.as_ref()
            && main.borrow().to_delete {
                info!("Dropping main entry point");
                Self::drop_entry(symbol_table, main);
                self.main_entry_point = None;
                // addons entries share the same root as the main entry
                self.addons_entry_points.clear();
            }
        let mut drop_if_flagged = |label: &str, entries: &mut Vec<Rc<RefCell<EntryPoint>>>| {
            entries.retain(|entry| {
                if entry.borrow().to_delete {
                    info!("Dropping {} entry point {}", label, entry.borrow().path);
                    Self::drop_entry(symbol_table, entry);
                    false
                } else {
                    true
                }
            });
        };
        drop_if_flagged("builtin",  &mut self.builtins_entry_points);
        drop_if_flagged("public",   &mut self.public_entry_points);
        drop_if_flagged("custom",   &mut self.custom_entry_points);
        drop_if_flagged("untitled", &mut self.untitled_entry_points);
    }

    /// Clears the tree rooted at the entry point from the symbol table.
    /// Shoud be called on every entry point removal.
    fn drop_entry(symbol_table: &mut SymbolTable, ep: &Rc<RefCell<EntryPoint>>) {
        let root = ep.borrow().root;
        symbol_table.drop_root(root, EntryPointCleanupToken(()));
    }

    /// Transform the path of an addon to the odoo relative path.
    /// Otherwise, return the path as is.
    pub fn transform_addon_path(&self, path: &Path) -> String {
        for entry in self.addons_entry_points.iter() {
            if entry.borrow().is_valid_for(path) {
                let path_str = path.sanitize_cow();
                return path_str.replace(&entry.borrow().path, entry.borrow().addon_to_odoo_path.as_ref().unwrap());
            }
        }
        path.sanitize()
    }

}

#[derive(Debug, Clone, PartialEq)]
pub enum EntryPointType {
    MAIN,
    BUILTIN,
    PUBLIC,
    ADDON,
    CUSTOM,
    UNTITLED,
}

#[derive(Debug, Clone)]
pub struct EntryPoint {
    pub path: String,
    pub tree: Vec<OYarn>,
    pub typ: EntryPointType,
    pub addon_to_odoo_path: Option<String>, //contains the odoo path if this is an addon entry point
    pub addon_to_odoo_tree: Option<Vec<OYarn>>, //contains the odoo tree if this is an addon entry point
    pub root: RootKey,
    pub not_found_symbols: WeakSet<SourceFileKey>,
    pub not_found_data_ids: WeakSet<SourceFileKey>,
    /// files with pending model lookups
    pub not_found_symbols_for_models: WeakSet<SourceFileKey>,
    pub to_delete: bool,
    pub data_symbols: HashMap<String, Wk<SourceFileKey>>, //key is path, value is weak key. Strong key is hold by the module symbol
    pub js_symbols: HashMap<String, Wk<JsFileKey>>, //key is path, value is weak key. Strong key is hold by the module symbol
}
impl EntryPoint {
    pub fn new(symbol_table: &mut SymbolTable, path: String, tree: Vec<OYarn>, typ: EntryPointType, addon_to_odoo_path: Option<String>, addon_to_odoo_tree: Option<Vec<OYarn>>) -> Rc<RefCell<Self>> {
        let entry = Rc::new(RefCell::new(Self { path,
            tree,
            typ,
            addon_to_odoo_path,
            addon_to_odoo_tree,
            not_found_symbols: WeakSet::new(),
            not_found_symbols_for_models: WeakSet::new(),
            not_found_data_ids: WeakSet::new(),
            root: RootKey::null(), // set below
            to_delete: false,
            data_symbols: HashMap::default(),
            js_symbols: HashMap::default(),
        }));
        let root = symbol_table.new_root(entry.clone());
        entry.borrow_mut().root = root;
        entry
    }

    pub fn new_with_shared_root(
        path: String,
        tree: Vec<OYarn>,
        typ: EntryPointType,
        addon_to_odoo_path: Option<String>,
        addon_to_odoo_tree: Option<Vec<OYarn>>,
        root: RootKey
    ) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            path,
            tree,
            typ,
            addon_to_odoo_path,
            addon_to_odoo_tree,
            not_found_symbols: WeakSet::new(),
            not_found_data_ids: WeakSet::new(),
            not_found_symbols_for_models: WeakSet::new(),
            root,
            to_delete: false,
            data_symbols: HashMap::default(),
            js_symbols: HashMap::default(),
        }))
    }

    pub fn is_valid_for(&self, path: &Path) -> bool {
        if self.typ == EntryPointType::UNTITLED {
            return self.path == path.sanitize_cow();
        }
        path.starts_with(&self.path)
    }

    pub fn is_public(&self) -> bool {
        self.typ == EntryPointType::PUBLIC || self.typ == EntryPointType::BUILTIN
    }

    pub fn is_main(&self) -> bool {
        self.typ == EntryPointType::MAIN || self.typ == EntryPointType::ADDON
    }

    pub fn get_symbol(&self, symbol_table: &SymbolTable) -> Option<SymbolKey> {
        let tree = self.addon_to_odoo_tree.as_ref().unwrap_or(&self.tree);
        let symbol = symbol_table.get_symbol(self.root.into(), (tree, &[]), u32::MAX);
        match symbol.len() {
            0 => None,
            1 => Some(symbol[0]),
            _ => panic!("Multiple symbols found for entry point {:?}", self)
        }
    }

    //it assumes that the path is valid for the entry
    pub fn get_tree_for_entry(&self, path: &Path) -> Tree {
        if let Some(addon_to_odoo_path) = self.addon_to_odoo_path.as_ref() {
            let path = path.strip_prefix(&self.path).unwrap();
            let path = PathBuf::from(addon_to_odoo_path.clone()).join(path.to_str().unwrap());
            return path.to_tree();
        }
        //no transformation needed, let's return the tree
        path.to_tree()
    }

    /// Move symbols whose pending build step is `ARCH`/`ARCH_EVAL`/`VALIDATION` into the
    /// corresponding rebuild queue (validation additionally invalidates sub functions).
    fn dispatch_rebuild(session: &mut SessionInfo, to_add: [Vec<SourceFileKey>; 3]) {
        let [arch, arch_eval, validation] = to_add;
        for s in arch {
            session.sync_odoo.add_to_rebuild_arch(s);
        }
        for s in arch_eval {
            session.sync_odoo.add_to_rebuild_arch_eval(s);
        }
        for s in validation {
            SymbolTable::invalidate_sub_functions(session, s);
            session.sync_odoo.add_to_validations(s);
        }
    }

    /// Shared implementation of [`Self::search_rebuild_for_models`] and
    /// [`Self::search_rebuild_for_data_id`]: for every symbol in `symbols` that was waiting
    /// on `key`, move it to the appropriate rebuild queue and drop the pending entry.
    fn search_rebuild_for_key<K: Eq + std::hash::Hash>(
        session: &mut SessionInfo,
        symbols: &mut WeakSet<SourceFileKey>,
        key: &K,
        not_found_mut: fn(&mut SymbolTable, SourceFileKey) -> Option<&mut HashMap<K, BuildSteps>>,
        not_found: fn(&SymbolTable, SourceFileKey) -> Option<&HashMap<K, BuildSteps>>,
    ) {
        let mut to_add: [Vec<SourceFileKey>; 3] = [vec![], vec![], vec![]];
        for sym_key in symbols.iter_valid(session.st()) {
            let Some(not_found_map) = not_found_mut(session.st_mut(), sym_key) else {
                continue;
            };
            let Some(step) = not_found_map.get(key) else {
                continue;
            };
            if let BuildSteps::ARCH | BuildSteps::ARCH_EVAL | BuildSteps::VALIDATION = step {
                to_add[*step as usize].push(sym_key);
            }
            not_found_map.remove(key);
        }
        Self::dispatch_rebuild(session, to_add);
        symbols.retain_valid(session.st(), |&sym| {
            !not_found(session.st(), sym).map(|map| map.is_empty()).unwrap_or(true)
        });
    }

    /* Consider the given 'tree' path as updated (or new) and move all symbols that were searching for it
    from the not_found_symbols list to the rebuild list. Return True is something should be rebuilt */
    pub fn search_symbols_to_rebuild(&mut self, session: &mut SessionInfo, path: &str, tree: Tree) {
        let flat_tree = tree.flatten();
        let mut to_add = [vec![], vec![], vec![]];
        for s in self.not_found_symbols.iter_valid(session.st()) {
            if let SourceFileKey::Module(p) = s {
                let module_package = &mut session.st_mut()[p];
                if let Some(step) = module_package.not_found_data.get(path) {
                    match step {
                        BuildSteps::ARCH | BuildSteps::ARCH_EVAL | BuildSteps::VALIDATION => {
                            to_add[*step as usize].push(s);
                        }
                        _ => {}
                    }
                    module_package.not_found_data.remove(path);
                    continue; //as if a data has been found, we won't find anything later, so we can continue the loop
                }
            }
            let not_found = session.st_mut().not_found_paths_mut(s);
            not_found.retain(|(step, not_found_tree)| {
                let prefix = cmp::min(not_found_tree.len(), flat_tree.len());
                if flat_tree[..prefix] != not_found_tree[..prefix] {
                    return true; // keep
                }
                if let BuildSteps::ARCH | BuildSteps::ARCH_EVAL | BuildSteps::VALIDATION = step {
                    to_add[*step as usize].push(s);
                }
                false // drop
            });
        }
        Self::dispatch_rebuild(session, to_add);
        self.not_found_symbols.retain_valid(session.st(), |&sym| {
            if !session.st().not_found_paths(sym).is_empty() {
                return true;
            }
            if let SourceFileKey::Module(module_key) = sym {
                return !session.st()[module_key].not_found_data.is_empty();
            }
            false
        });
    }

    pub fn search_rebuild_for_models(&mut self, session: &mut SessionInfo, model_name: OYarn) {
        Self::search_rebuild_for_key(
            session,
            &mut self.not_found_symbols_for_models,
            &model_name,
            SymbolTable::not_found_models_mut,
            SymbolTable::not_found_models,
        );
    }

    pub fn search_rebuild_for_data_id(&mut self, session: &mut SessionInfo, data: MissingDataSource) {
        Self::search_rebuild_for_key(
            session,
            &mut self.not_found_data_ids,
            &data,
            SymbolTable::not_found_data_ids_mut,
            SymbolTable::not_found_data_ids,
        );
    }
}

/// Capability token restricting [`SymbolTable::drop_root`] to this module.
///
/// The `()` field is private, so the tuple-struct constructor
/// `EntryPointCleanupToken(())` only compiles inside this module; Rust
/// treats a tuple-struct constructor as private if any field is.
pub struct EntryPointCleanupToken(());
