use std::path::Path;
use std::{cmp, path::PathBuf};
use crate::constants::MissingDataSource;
use crate::core::build_scheduler::BuildScheduler;
use crate::core::symbols::storage::FileSystemSymbolParent;
use crate::core::symbols::symbol_table_impl::CreateError;
use crate::utils::HashMap;

use slotmap::{Key, SlotMap};
use tracing::{error, info, warn};

use crate::core::symbols::symbol_keys::{BuildableSymbolKey, EntryPointKey, FileKey, JsFileKey, KeyValidator, RootKey, SourceFileKey, SymbolKey, Wk};
use crate::{
    tree::Tree,
    constants::{BuildSteps, OYarn},
    core::symbols::storage::SymbolTable,
    threads::SessionInfo,
    utils::PathSanitizer,
    warn_or_panic,
    weak_collections::WeakSet,
};

#[derive(Debug)]
pub struct EntryPointMgr {
    entry_points: SlotMap<EntryPointKey, EntryPoint>,
    pub builtins_entry_points: Vec<EntryPointKey>,
    pub public_entry_points: Vec<EntryPointKey>,
    pub main_entry_point: Option<EntryPointKey>,
    pub addons_entry_points: Vec<EntryPointKey>,
    pub custom_entry_points: Vec<EntryPointKey>,
    pub untitled_entry_points: Vec<EntryPointKey>,
}

impl Default for EntryPointMgr {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Index<EntryPointKey> for EntryPointMgr {
    type Output = EntryPoint;
    fn index(&self, k: EntryPointKey) -> &EntryPoint {
        &self.entry_points[k]
    }
}

impl std::ops::IndexMut<EntryPointKey> for EntryPointMgr {
    fn index_mut(&mut self, k: EntryPointKey) -> &mut EntryPoint {
        &mut self.entry_points[k]
    }
}

impl KeyValidator<EntryPointKey> for EntryPointMgr {
    fn is_key_valid(&self, k: EntryPointKey) -> bool {
        self.entry_points.contains_key(k)
    }
}

impl EntryPointMgr {

    pub fn new() -> Self {
        Self {
            entry_points: SlotMap::with_key(),
            builtins_entry_points: vec![],
            public_entry_points: vec![],
            main_entry_point: None,
            addons_entry_points: vec![],
            custom_entry_points: vec![],
            untitled_entry_points: vec![],
        }
    }

    /// Creates a new `EntryPoint` together with its owning `RootSymbol`, wiring the
    /// cross-reference in one place. The only way to create a root-owning entry point
    /// (builtin/public/main/custom/untitled — everything except addons, which share the
    /// main entry's root; see `create_addon_entry_point`).
    pub fn create_entry_point(
        &mut self,
        symbol_table: &mut SymbolTable,
        path: String,
        tree: Vec<OYarn>,
        typ: EntryPointType,
        addon_to_odoo_path: Option<String>,
        addon_to_odoo_tree: Option<Vec<OYarn>>,
    ) -> EntryPointKey {
        let entry_key = self.entry_points.insert(EntryPoint::new(path, tree, typ, addon_to_odoo_path, addon_to_odoo_tree, RootKey::null()));
        let root = symbol_table.insert_root(entry_key);
        self.entry_points[entry_key].root = root;
        entry_key
    }

    /// Creates a new `EntryPoint` that shares an existing root instead of owning one —
    /// addon entry points piggyback on the main entry's root/subtree. Must only ever be
    /// dropped via `drop_entry_point`, which knows not to cascade into the shared root
    /// for `EntryPointType::ADDON`.
    pub fn create_addon_entry_point(
        &mut self,
        path: String,
        tree: Vec<OYarn>,
        addon_to_odoo_path: Option<String>,
        addon_to_odoo_tree: Option<Vec<OYarn>>,
        shared_root: RootKey,
    ) -> EntryPointKey {
        self.entry_points.insert(EntryPoint::new(path, tree, EntryPointType::ADDON, addon_to_odoo_path, addon_to_odoo_tree, shared_root))
    }

    /// Drops an `EntryPoint`. Unless it's an addon (which shares its root with the main
    /// entry and never owns one), also cascades to drop its `RootSymbol` and every
    /// symbol beneath it. The only place an `EntryPoint` is ever removed — this and the
    /// two constructors above are the sole way to touch `self.entry_points`.
    pub fn drop_entry_point(&mut self, symbol_table: &mut SymbolTable, key: EntryPointKey, _: EntryPointCleanupToken) {
        if self.entry_points[key].typ != EntryPointType::ADDON {
            let root = self.entry_points[key].root;
            symbol_table.drop_root_if_present(root);
        }
        self.entry_points.remove(key);
    }

    /// Create a new entry for an untitled (in-memory) file.
    /// Returns the file symbol for the untitled entry.
    pub fn add_entry_to_untitled(session: &mut SessionInfo, path: String) -> FileKey {
        // For untitled files, we use a minimal tree: just the name as a single OYarn
        info!("Adding new untitled entry point: {}", path);
        let tree = vec![OYarn::from(path.clone())];
        let entry_key = session.sync_odoo.entry_point_mgr.create_entry_point(
            &mut session.sync_odoo.symbol_table,
            path.clone(),
            tree,
            EntryPointType::UNTITLED,
            None,
            None,
        );
        session.sync_odoo.entry_point_mgr.untitled_entry_points.push(entry_key);
        // Create one file symbol under the root for the untitled file
        let path_stem = Path::new(&path).with_extension("");
        let name = path_stem.components().next_back().unwrap().as_os_str().to_str().unwrap();

        let root = session.ep_mgr()[entry_key].root;
        session.st_mut().add_new_file(root.into(), name, &path).expect("fresh root has no children")
    }

    /**
     * Create each required directory symbols for a given path.
     * /!\ path must point to a directory on disk */
    fn create_dir_symbols_for_new_entry(session: &mut SessionInfo, path: &str, entry: EntryPointKey) -> Option<SymbolKey> {
        let path = Path::new(path);
        let mut iter_path = PathBuf::new();
        let mut current_sym: FileSystemSymbolParent = session.ep_mgr()[entry].root.into();
        let component_count = path.components().count();
        for component in path.components().take(component_count - 1) {
            iter_path.push(component);
            if let Some(name) = component.as_os_str().to_str() {
                let disk_dir = session.st_mut().add_new_disk_dir(current_sym, name, iter_path.to_str().unwrap());
                current_sym = disk_dir.expect("Starting from fresh root, no name collision expected").into();
            } else {
                error!("Unable to convert path component to string");
                return None;
            }
        }
        match SymbolTable::create_from_path(session, path, current_sym, false) {
            Ok(sym) => Some(sym),
            Err(CreateError::NothingOnDisk) => None,
            Err(CreateError::Existing(key)) =>
                unreachable!("callers pass a freshly created EntryPoint, so the chain should be empty; found {key:?}"),
        }
    }

    /* Create a new main entry_point.
    return the disk_dir symbol of the last FOLDER of the path
     */
    pub fn set_main_entry(session: &mut SessionInfo, path: String) -> Option<SymbolKey> {
        info!("Setting Main entry point: {}", path);
        let entry_point_tree = Path::new(&path).to_tree();
        let entry_key = session.sync_odoo.entry_point_mgr.create_entry_point(
            &mut session.sync_odoo.symbol_table,
            path.clone(),
            entry_point_tree.flatten(),
            EntryPointType::MAIN,
            None,
            None);
        session.sync_odoo.entry_point_mgr.main_entry_point = Some(entry_key);

        EntryPointMgr::create_dir_symbols_for_new_entry(session, &path, entry_key)
    }

    /* Create a new entry to builtins.
    return the disk_dir symbol of the last FOLDER of the path
     */
    pub fn add_entry_to_builtins(session: &mut SessionInfo, path: String) -> Option<SymbolKey> {
        info!("Adding new builtins entry point: {}", path);
        let entry_point_tree = Path::new(&path).to_tree();
        let entry_key = session.sync_odoo.entry_point_mgr.create_entry_point(
            &mut session.sync_odoo.symbol_table,
            path.clone(),
            entry_point_tree.flatten(),
            EntryPointType::BUILTIN,
            None,
            None);
        session.sync_odoo.entry_point_mgr.builtins_entry_points.push(entry_key);

        EntryPointMgr::create_dir_symbols_for_new_entry(session, &path, entry_key)
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
        let entry_point_tree = Path::new(&path).to_tree();
        let entry_key = session.sync_odoo.entry_point_mgr.create_entry_point(
            &mut session.sync_odoo.symbol_table,
            path.clone(),
            entry_point_tree.flatten(),
            EntryPointType::PUBLIC,
            None,
            None);
        session.sync_odoo.entry_point_mgr.public_entry_points.push(entry_key);

        EntryPointMgr::create_dir_symbols_for_new_entry(session, &path, entry_key)
    }

    /* Create a new entry to addons.
     * This function, unlike its siblings, does not a produce a symbol.
     */
    pub fn add_entry_to_addons(session: &mut SessionInfo, path: String, main_entry: EntryPointKey, added_tree: Vec<OYarn>) {
        info!("Adding new addon entry point: {}", path);
        let entry_point_tree = Path::new(&path).to_tree();
        let (main_path, main_tree, shared_root) = {
            let main = &session.ep_mgr()[main_entry];
            (main.path.clone(), main.tree.clone(), main.root)
        };
        let addon_to_odoo_path = Some(main_path + "/" + added_tree.join("/").as_str());
        let addon_to_odoo_tree = Some(main_tree.iter().chain(&added_tree).cloned().collect());
        let entry_key = session.sync_odoo.entry_point_mgr.create_addon_entry_point(
            path,
            entry_point_tree.flatten(),
            addon_to_odoo_path,
            addon_to_odoo_tree,
            shared_root
        );
        session.sync_odoo.entry_point_mgr.addons_entry_points.push(entry_key);
    }

    /* Create a new entry to public.
    return the symbol at the end of the path
     */
    pub fn add_entry_to_customs(session: &mut SessionInfo, path: &str) -> Option<SymbolKey> {
        info!("Adding new custom entry point: {}", path);
        let entry_point_tree = Path::new(path).to_tree();
        let entry_key = session.sync_odoo.entry_point_mgr.create_entry_point(
            &mut session.sync_odoo.symbol_table,
            path.to_string(),
            entry_point_tree.flatten(),
            EntryPointType::CUSTOM,
            None,
            None);
        session.sync_odoo.entry_point_mgr.custom_entry_points.push(entry_key);
        EntryPointMgr::create_dir_symbols_for_new_entry(session, path, entry_key)
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
                        session.sync_odoo.entry_point_mgr.remove_entries_with_path(&mut session.sync_odoo.symbol_table, tree_path);
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
                    BuildScheduler::queue(session, new_sym.unwrap_buildable_key());
                    return true;
                }
                _ => {panic!("Unexpected symbol type: {:?}", new_sym);}
            }
            BuildScheduler::queue(session, new_sym.unwrap_buildable_key());
        }
        true
    }

    pub fn create_new_untitled_entry_for_path(session: &mut SessionInfo, file_name: &str) -> bool {
        let new_sym = EntryPointMgr::add_entry_to_untitled(session, file_name.to_string());
        session.sync_odoo.symbol_table[new_sym].self_import = true;
        BuildScheduler::queue(session, BuildableSymbolKey::File(new_sym));
        true
    }

    pub fn iter_for_import(&self, current_entry: EntryPointKey) -> Box<dyn Iterator<Item = EntryPointKey> + '_> {
        let is_main = self.iter_main().any(|entry| entry == current_entry);
        if is_main {
            Box::new(self.addons_entry_points.iter().copied().chain(
            self.main_entry_point.iter().copied()).chain(
            self.builtins_entry_points.iter().copied()).chain(
            self.public_entry_points.iter().copied()))
        } else {
            Box::new(self.custom_entry_points.iter().copied().chain(
            self.builtins_entry_points.iter().copied()).chain(
            self.public_entry_points.iter().copied()))
        }
    }

    pub fn iter_all(&self) -> impl Iterator<Item = EntryPointKey> {
        self.addons_entry_points.iter().copied()
            .chain(self.main_entry_point.iter().copied())
            .chain(self.builtins_entry_points.iter().copied())
            .chain(self.public_entry_points.iter().copied())
            .chain(self.custom_entry_points.iter().copied())
            .chain(self.untitled_entry_points.iter().copied())
    }

    //iter through all main entry points, sorted by tree length (from bigger to smaller)
    pub fn iter_main(&self) -> impl Iterator<Item = EntryPointKey>
    {
        let mut collected = self.main_entry_point.iter().copied().chain(self.addons_entry_points.iter().copied()).collect::<Vec<_>>();
        collected.sort_by_key(|&ep| std::cmp::Reverse(self[ep].tree.len()));
        collected.into_iter()
    }

    pub fn iter_all_but_main(&self) -> impl Iterator<Item = EntryPointKey> {
        self.builtins_entry_points.iter().copied()
        .chain(self.public_entry_points.iter().copied())
        .chain(self.custom_entry_points.iter().copied())
        .chain(self.untitled_entry_points.iter().copied())
    }

    pub fn iter_all_but_public(&self) -> impl Iterator<Item = EntryPointKey> {
        self.main_entry_point.iter().copied().chain(
        self.addons_entry_points.iter().copied()).chain(
        self.custom_entry_points.iter().copied()
        )
    }

    pub fn reset_entry_points(&mut self, symbol_table: &mut SymbolTable, with_custom_entries: bool) {
        let builtins = std::mem::take(&mut self.builtins_entry_points);
        for ep in builtins {
            self.drop_entry(symbol_table, ep);
        }
        let public = std::mem::take(&mut self.public_entry_points);
        for ep in public {
            self.drop_entry(symbol_table, ep);
        }
        if let Some(main_ep) = self.main_entry_point {
            self.drop_entry(symbol_table, main_ep);
            self.main_entry_point = None;
            // addons entries share the same root as the main entry — they never own a
            // root themselves, but each still holds its own `EntryPointKey` slot, so it
            // must go through `drop_entry` too, not just be cleared from the Vec.
            let addons = std::mem::take(&mut self.addons_entry_points);
            for ep in addons {
                self.drop_entry(symbol_table, ep);
            }
        }
        if with_custom_entries {
            let custom = std::mem::take(&mut self.custom_entry_points);
            for ep in custom {
                self.drop_entry(symbol_table, ep);
            }
        }
    }

    pub fn remove_entries_with_path(&mut self, symbol_table: &mut SymbolTable, path: &str) {
        for entry in self.iter_all().collect::<Vec<_>>() {
            if (self[entry].typ == EntryPointType::UNTITLED && self[entry].path == *path)
            || (self[entry].typ != EntryPointType::UNTITLED
            && Path::new(&self[entry].path).starts_with(path)){  //delete any entrypoint that would be in a subdirectory too
                self[entry].to_delete = true;
            }
        }
        self.clean_entries(symbol_table);
    }

    pub fn clean_entries(&mut self, symbol_table: &mut SymbolTable) {
        if let Some(main) = self.main_entry_point
            && self[main].to_delete {
                info!("Dropping main entry point");
                self.drop_entry(symbol_table, main);
                self.main_entry_point = None;
                // addons entries share the same root as the main entry — see the same
                // note in `reset_entry_points`.
                let addons = std::mem::take(&mut self.addons_entry_points);
                for ep in addons {
                    self.drop_entry(symbol_table, ep);
                }
            }
        let builtins = std::mem::take(&mut self.builtins_entry_points);
        self.builtins_entry_points = self.drop_flagged(symbol_table, "builtin", builtins);
        let public = std::mem::take(&mut self.public_entry_points);
        self.public_entry_points = self.drop_flagged(symbol_table, "public", public);
        let custom = std::mem::take(&mut self.custom_entry_points);
        self.custom_entry_points = self.drop_flagged(symbol_table, "custom", custom);
        let untitled = std::mem::take(&mut self.untitled_entry_points);
        self.untitled_entry_points = self.drop_flagged(symbol_table, "untitled", untitled);
    }

    fn drop_flagged(&mut self, symbol_table: &mut SymbolTable, label: &str, mut entries: Vec<EntryPointKey>) -> Vec<EntryPointKey> {
        entries.retain(|&entry| {
            if self[entry].to_delete {
                info!("Dropping {} entry point {}", label, self[entry].path);
                self.drop_entry(symbol_table, entry);
                false
            } else {
                true
            }
        });
        entries
    }

    /// Drops an entry point (root/subtree cascade included, unless it's an addon —
    /// see `drop_entry_point`). Should be called instead of removing an
    /// `EntryPointKey` from a `Vec` bare — the only place an `EntryPoint` is destroyed.
    fn drop_entry(&mut self, symbol_table: &mut SymbolTable, ep: EntryPointKey) {
        self.drop_entry_point(symbol_table, ep, EntryPointCleanupToken(()));
    }

    /// Transform the path of an addon to the odoo relative path.
    /// Otherwise, return the path as is.
    pub fn transform_addon_path(&self, path: &Path) -> String {
        for &entry in self.addons_entry_points.iter() {
            if self[entry].is_valid_for(path) {
                let path_str = path.sanitize_cow();
                return path_str.replace(&self[entry].path, self[entry].addon_to_odoo_path.as_ref().unwrap());
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
    pub data_file_symbols: HashMap<String, Wk<SourceFileKey>>, //key is path, value is weak key. Strong key is hold by the module symbol
    pub js_symbols: HashMap<String, Wk<JsFileKey>>, //key is path, value is weak key. Strong key is hold by the module symbol
}
impl EntryPoint {
    /// Plain value constructor. `root` is a placeholder (`RootKey::null()`) for a
    /// root-owning entry point that `EntryPointMgr::create_entry_point` patches once the
    /// `RootSymbol` exists, or the real shared root for an addon entry point
    /// (`EntryPointMgr::create_addon_entry_point`). Do not call directly — those two are
    /// the only sanctioned ways to obtain an `EntryPointKey`.
    pub fn new(path: String, tree: Vec<OYarn>, typ: EntryPointType, addon_to_odoo_path: Option<String>, addon_to_odoo_tree: Option<Vec<OYarn>>, root: RootKey) -> Self {
        Self {
            path,
            tree,
            typ,
            addon_to_odoo_path,
            addon_to_odoo_tree,
            not_found_symbols: WeakSet::new(),
            not_found_symbols_for_models: WeakSet::new(),
            not_found_data_ids: WeakSet::new(),
            root,
            to_delete: false,
            data_file_symbols: HashMap::default(),
            js_symbols: HashMap::default(),
        }
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
            let path = Path::new(addon_to_odoo_path).join(path.to_str().unwrap());
            return path.to_tree();
        }
        //no transformation needed, let's return the tree
        path.to_tree()
    }

    /// Move symbols whose pending build step is `ARCH`/`ARCH_EVAL`/`VALIDATION` into the
    /// corresponding rebuild queue (validation additionally invalidates sub functions).
    fn dispatch_rebuild(session: &mut SessionInfo, to_add: HashMap<SourceFileKey, BuildSteps>) {
        for (source, step) in to_add {
            SymbolTable::invalidate(session, source, step);
            BuildScheduler::queue(session, source);
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
        let mut to_add: HashMap<SourceFileKey, BuildSteps> = HashMap::default();
        for sym_key in symbols.iter_valid(session.st()) {
            let Some(not_found_map) = not_found_mut(session.st_mut(), sym_key) else {
                continue;
            };
            let Some(step) = not_found_map.get(key) else {
                continue;
            };
            to_add.insert(sym_key, *step);
            not_found_map.remove(key);
        }
        Self::dispatch_rebuild(session, to_add);
        symbols.retain_valid(session.st(), |&sym| {
            !not_found(session.st(), sym).map(|map| map.is_empty()).unwrap_or(true)
        });
    }

    /* Consider the given 'tree' path as updated (or new) and move all symbols that were searching for it
    from the not_found_symbols list to the rebuild list. Return True is something should be rebuilt */
    pub fn search_symbols_to_rebuild(session: &mut SessionInfo, entry: EntryPointKey, path: &str, tree: Tree) {
        let flat_tree = tree.flatten();
        let mut to_add = HashMap::default();
        let to_process: Vec<SourceFileKey> = session.ep_mgr()[entry].not_found_symbols.iter_valid(session.st()).collect();
        for s in to_process {
            if let SourceFileKey::Module(p) = s {
                let module_package = &mut session.st_mut()[p];
                if let Some(step) = module_package.not_found_data.get(path) {
                    if let Some(previous) = to_add.get(&s) {
                        if *step < *previous {
                            to_add.insert(s, *step);
                        }
                    } else {
                        to_add.insert(s, *step);
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
                if let Some(previous) = to_add.get(&s) {
                    if *step < *previous {
                        to_add.insert(s, *step);
                    }
                } else {
                    to_add.insert(s, *step);
                }
                false // drop
            });
        }
        Self::dispatch_rebuild(session, to_add);
        let mut not_found_symbols = std::mem::take(&mut session.ep_mgr_mut()[entry].not_found_symbols);
        not_found_symbols.retain_valid(session.st(), |&sym| {
            if !session.st().not_found_paths(sym).is_empty() {
                return true;
            }
            if let SourceFileKey::Module(module_key) = sym {
                return !session.st()[module_key].not_found_data.is_empty();
            }
            false
        });
        session.ep_mgr_mut()[entry].not_found_symbols = not_found_symbols;
    }

    pub fn search_rebuild_for_models(session: &mut SessionInfo, entry: EntryPointKey, model_name: OYarn) {
        let mut not_found_symbols_for_models = std::mem::take(&mut session.ep_mgr_mut()[entry].not_found_symbols_for_models);
        Self::search_rebuild_for_key(
            session,
            &mut not_found_symbols_for_models,
            &model_name,
            SymbolTable::not_found_models_mut,
            SymbolTable::not_found_models,
        );
        session.ep_mgr_mut()[entry].not_found_symbols_for_models = not_found_symbols_for_models;
    }

    pub fn search_rebuild_for_data_id(session: &mut SessionInfo, entry: EntryPointKey, data: MissingDataSource) {
        let mut not_found_data_ids = std::mem::take(&mut session.ep_mgr_mut()[entry].not_found_data_ids);
        Self::search_rebuild_for_key(
            session,
            &mut not_found_data_ids,
            &data,
            SymbolTable::not_found_data_ids_mut,
            SymbolTable::not_found_data_ids,
        );
        session.ep_mgr_mut()[entry].not_found_data_ids = not_found_data_ids;
    }
}

/// Capability token restricting [`EntryPointMgr::drop_entry_point`] to this module.
///
/// The `()` field is private, so the tuple-struct constructor
/// `EntryPointCleanupToken(())` only compiles inside this module; Rust
/// treats a tuple-struct constructor as private if any field is.
pub struct EntryPointCleanupToken(());
