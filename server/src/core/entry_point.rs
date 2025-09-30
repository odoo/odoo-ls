use std::{cell::RefCell, cmp, collections::HashMap, path::PathBuf, rc::{Rc, Weak}, u32};

use tracing::{error, info, warn};
use weak_table::PtrWeakHashSet;

use crate::{constants::{flatten_tree, BuildSteps, OYarn, PackageType, SymType, Tree}, threads::SessionInfo, utils::PathSanitizer, warn_or_panic};

use super::{odoo::SyncOdoo, symbols::symbol::Symbol};

#[derive(Debug)]
pub struct EntryPointMgr {
    pub builtins_entry_points: Vec<Rc<RefCell<EntryPoint>>>,
    pub public_entry_points: Vec<Rc<RefCell<EntryPoint>>>,
    pub main_entry_point: Option<Rc<RefCell<EntryPoint>>>,
    pub addons_entry_points: Vec<Rc<RefCell<EntryPoint>>>,
    pub custom_entry_points: Vec<Rc<RefCell<EntryPoint>>>,
}

impl EntryPointMgr {

    pub fn new() -> Self {
        Self {
            builtins_entry_points: vec![],
            public_entry_points: vec![],
            main_entry_point: None,
            addons_entry_points: vec![],
            custom_entry_points: vec![],
        }
    }

    /**
     * Create each required directory symbols for a given path.
     * /!\ path must point to a directory on disk */
    pub fn create_dir_symbols_from_path_to_entry(session: &mut SessionInfo, path: &PathBuf, entry: Rc<RefCell<EntryPoint>>) -> Option<Rc<RefCell<Symbol>>> {
        let mut iter_path = PathBuf::new();
        let mut current_sym = entry.borrow().root.clone();
        let component_count = path.components().count();
        for component in path.components().take(component_count - 1) {
            iter_path.push(component);
            if let Some(name) = component.as_os_str().to_str() {
                let sym = current_sym.borrow().get_module_symbol(name).clone();
                if let Some(existing_sym) = sym {
                    current_sym = existing_sym.clone();
                } else {
                    let disk_dir = current_sym.borrow_mut().add_new_disk_dir(&name.to_string(), &iter_path.to_str().unwrap().to_string()).clone();
                    current_sym = disk_dir;
                }
            } else {
                error!("Unable to convert path component to string");
                return None;
            }
        }
        Symbol::create_from_path(session, path, current_sym, false)
    }

    /* Create a new main entry_point.
    return the disk_dir symbol of the last FOLDER of the path
     */
    pub fn set_main_entry(session: &mut SessionInfo, path: String) -> Option<Rc<RefCell<Symbol>>> {
        info!("Setting Main entry point: {}", path);
        let entry_point_tree = PathBuf::from(&path).to_tree();
        let entry = EntryPoint::new(path.clone(),
        flatten_tree(&entry_point_tree),
        EntryPointType::MAIN,
        None,
        None);
        session.sync_odoo.entry_point_mgr.borrow_mut().main_entry_point = Some(entry.clone());
        let sym = EntryPointMgr::_create_dir_symbols_for_new_entry(session, &path, entry);
        sym
    }

    /* Create a new entry to builtins.
    return the disk_dir symbol of the last FOLDER of the path
     */
    pub fn add_entry_to_builtins(session: &mut SessionInfo, path: String) -> Option<Rc<RefCell<Symbol>>> {
        info!("Adding new builtins entry point: {}", path);
        let entry_point_tree = PathBuf::from(&path).to_tree();
        let entry = EntryPoint::new(path.clone(),
        flatten_tree(&entry_point_tree),
        EntryPointType::BUILTIN,
        None,
        None);
        session.sync_odoo.entry_point_mgr.borrow_mut().builtins_entry_points.push(entry.clone());
        let sym = EntryPointMgr::_create_dir_symbols_for_new_entry(session, &path, entry);
        sym
    }

    /* Create a new entry to public.
    return the disk_dir symbol of the last FOLDER of the path
     */
    pub fn add_entry_to_public(session: &mut SessionInfo, path: String) -> Option<Rc<RefCell<Symbol>>> {
        info!("Adding new public entry point: {}", path);
        //Prevent adding entry point from sys.path or other config that is matching odoo or addons paths
        if let Some(odoo_path) = &session.sync_odoo.config.odoo_path {
            if &path == odoo_path {
                warn!("Public entry point {} is equal to odoo path {}, this is not supported and will be ignored", path, odoo_path);
                return None;
            }
        }
        for addon_path in &session.sync_odoo.config.addons_paths {
            if &path == addon_path {
                warn!("Public entry point {} is equal to addon path {}, this is not supported and will be ignored", path, addon_path);
                return None;
            }
        }
        let entry_point_tree = PathBuf::from(&path).to_tree();
        let entry = EntryPoint::new(path.clone(),
        flatten_tree(&entry_point_tree),
        EntryPointType::PUBLIC,
        None,
        None);
        session.sync_odoo.entry_point_mgr.borrow_mut().public_entry_points.push(entry.clone());
        let sym = EntryPointMgr::_create_dir_symbols_for_new_entry(session, &path, entry);
        sym
    }

    /* Create a new entry to public.
    return the disk_dir symbol of the last FOLDER of the path
     */
    pub fn add_entry_to_addons(session: &mut SessionInfo, path: String, related: Option<Rc<RefCell<EntryPoint>>>, related_addition: Option<Vec<OYarn>>) -> Option<Rc<RefCell<Symbol>>> {
        info!("Adding new addon entry point: {}", path);
        let entry_point_tree = PathBuf::from(&path).to_tree();
        let mut addon_to_odoo_path = None;
        let mut addon_to_odoo_tree = None;
        if let Some(ref related) = related {
            let Some(related_addition) = related_addition else {
                panic!("related_addition must be set if related is set");
            };
            addon_to_odoo_path = Some(related.borrow().path.clone() + "/" + related_addition.join("/").as_str());
            addon_to_odoo_tree = Some(related.borrow().tree.iter().chain(&related_addition).map(|x| x.clone()).collect());
        }
        let entry = EntryPoint::new(path.clone(),
        flatten_tree(&entry_point_tree),
        EntryPointType::ADDON,
        addon_to_odoo_path,
        addon_to_odoo_tree);
        session.sync_odoo.entry_point_mgr.borrow_mut().addons_entry_points.push(entry.clone());
        let sym = EntryPointMgr::_create_dir_symbols_for_new_entry(session, &path, entry.clone());
        if let Some(ref related) = related {
            entry.borrow_mut().root = related.borrow().root.clone();
        }
        sym
    }

    /* Create a new entry to public.
    return the symbol at the end of the path
     */
    pub fn add_entry_to_customs(session: &mut SessionInfo, path: String) -> Option<Rc<RefCell<Symbol>>> {
        info!("Adding new custom entry point: {}", path);
        let entry_point_tree = PathBuf::from(&path).to_tree();
        let entry = EntryPoint::new(path.clone(),
        flatten_tree(&entry_point_tree),
        EntryPointType::CUSTOM,
        None,
        None);
        session.sync_odoo.entry_point_mgr.borrow_mut().custom_entry_points.push(entry.clone());
        EntryPointMgr::_create_dir_symbols_for_new_entry(session, &path, entry)
    }

    fn _create_dir_symbols_for_new_entry(session: &mut SessionInfo, path: &String, entry: Rc<RefCell<EntryPoint>>) -> Option<Rc<RefCell<Symbol>>> {
        EntryPointMgr::create_dir_symbols_from_path_to_entry(session, &PathBuf::from(path), entry)
    }

    /// Create a new custom entry point for a given tree path and file path.
    /// tree_path can possibly be the path stripped from __manifest__/__init__.py
    pub fn create_new_custom_entry_for_path(session: &mut SessionInfo, tree_path: &String, file_path: &String) -> bool {
        let new_sym = EntryPointMgr::add_entry_to_customs(session, tree_path.clone());
        if let Some(new_sym) = new_sym {
            new_sym.borrow_mut().set_is_external(false);
            let new_sym_typ = new_sym.borrow().typ();
            match new_sym_typ {
                SymType::PACKAGE(PackageType::PYTHON_PACKAGE) => {
                    new_sym.borrow_mut().as_python_package_mut().self_import = true;
                },
                SymType::FILE => {
                    new_sym.borrow_mut().as_file_mut().self_import = true;
                },
                SymType::NAMESPACE => {
                    if file_path.ends_with("__manifest__.py") {
                        warn!("new custom entry point for manifest without related init.py is not supported outside of main entry point. skipping...");
                        session.sync_odoo.entry_point_mgr.borrow_mut().remove_entries_with_path(tree_path);
                    } else {
                        // There was an __init__.py, that was renamed or deleted.
                        // Another notification will come for the deletion of the file, so we just warn here.
                        warn_or_panic!("Trying to create a custom entrypoint on a namespace symbol: {:?}", new_sym.borrow().paths());
                    }
                    return false;
                }
                _ => {panic!("Unexpected symbol type: {:?}", new_sym_typ);}
            }
            SyncOdoo::add_to_rebuild_arch(session.sync_odoo, new_sym);
        }
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
        self.addons_entry_points.iter().chain(
        self.main_entry_point.iter()).chain(
        self.builtins_entry_points.iter()).chain(
        self.public_entry_points.iter()).chain(
        self.custom_entry_points.iter()
        )
    }

    //iter through all main entry points, sorted by tree length (from bigger to smaller)
    pub fn iter_main(&self) -> impl Iterator<Item = &Rc<RefCell<EntryPoint>>>
    {
        let mut collected = self.main_entry_point.iter().chain(self.addons_entry_points.iter()).collect::<Vec<_>>();
        collected.sort_by(|x, y| y.borrow().tree.len().cmp(&x.borrow().tree.len()));
        collected.into_iter()
    }

    pub fn iter_all_but_main(&self) -> impl Iterator<Item = &Rc<RefCell<EntryPoint>>> {
        self.builtins_entry_points.iter().chain(
        self.public_entry_points.iter()).chain(
        self.custom_entry_points.iter()
        )
    }

    pub fn reset_entry_points(&mut self, with_custom_entries: bool) {
        self.builtins_entry_points.clear();
        self.public_entry_points.clear();
        self.main_entry_point = None;
        self.addons_entry_points.clear();
        if with_custom_entries {
            self.custom_entry_points.clear();
        }
    }

    pub fn remove_entries_with_path(&mut self, path: &String) {
        for entry in self.iter_all() {
            if PathBuf::from(entry.borrow().path.clone()).starts_with(path) { //delete any entrypoint that would be in a subdirectory too
                entry.borrow_mut().to_delete = true;
            }
        }
        self.clean_entries();
    }

    pub fn check_custom_entry_to_delete_with_path(&mut self, path: &String) {
        for entry in self.custom_entry_points.iter() {
            if entry.borrow().path == *path {
                entry.borrow_mut().to_delete = true;
            }
        }
    }

    pub fn clean_entries(&mut self) {
        if let Some(main) = self.main_entry_point.as_ref() {
            if main.borrow().to_delete {
                info!("Dropping main entry point");
                self.main_entry_point = None;
                self.addons_entry_points.clear();
            }
        }
        let mut entry_index = 0;
        while entry_index < self.builtins_entry_points.len() {
            let entry = self.builtins_entry_points[entry_index].clone();
            if entry.borrow().to_delete {
                info!("Dropping builtin entry point {}", entry.borrow().path);
                self.builtins_entry_points.remove(entry_index);
            } else {
                entry_index += 1;
            }
        }
        entry_index = 0;
        while entry_index < self.public_entry_points.len() {
            let entry = self.public_entry_points[entry_index].clone();
            if entry.borrow().to_delete {
                info!("Dropping public entry point {}", entry.borrow().path);
                self.public_entry_points.remove(entry_index);
            } else {
                entry_index += 1;
            }
        }
        let mut entry_index = 0;
        while entry_index < self.custom_entry_points.len() {
            let entry = self.custom_entry_points[entry_index].clone();
            if entry.borrow().to_delete {
                info!("Dropping custom entry point {}", entry.borrow().path);
                self.custom_entry_points.remove(entry_index);
            } else {
                entry_index += 1;
            }
        }
    }

    /// Transform the path of an addon to the odoo relative path.
    /// Otherwise, return the path as is.
    pub fn transform_addon_path(&self, path: &PathBuf) -> String {
        for entry in self.addons_entry_points.iter() {
            if entry.borrow().is_valid_for(path) {
                let path_str = path.sanitize();
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
    CUSTOM
}

#[derive(Debug, Clone)]
pub struct EntryPoint {
    pub path: String,
    pub tree: Vec<OYarn>,
    pub typ: EntryPointType,
    pub addon_to_odoo_path: Option<String>, //contains the odoo path if this is an addon entry point
    pub addon_to_odoo_tree: Option<Vec<OYarn>>, //contains the odoo tree if this is an addon entry point
    pub root: Rc<RefCell<Symbol>>,
    pub not_found_symbols: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub not_found_symbols_for_models: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub to_delete: bool,
    pub data_symbols: HashMap<String, Weak<RefCell<Symbol>>>, //key is path, weak to Rc that is hold by the module symbol
}
impl EntryPoint {
    pub fn new(path: String, tree: Vec<OYarn>, typ:EntryPointType, addon_to_odoo_path: Option<String>, addon_to_odoo_tree: Option<Vec<OYarn>>) -> Rc<RefCell<Self>> {
        let root = Symbol::new_root();
        root.borrow_mut().as_root_mut().weak_self = Some(Rc::downgrade(&root)); // manually set weakself for root symbols
        let res = Rc::new(RefCell::new(Self { path,
            tree,
            typ,
            addon_to_odoo_path,
            addon_to_odoo_tree,
            not_found_symbols: PtrWeakHashSet::new(),
            not_found_symbols_for_models: PtrWeakHashSet::new(),
            root: root.clone(),
            to_delete: false,
            data_symbols: HashMap::new(),
        }));
        root.borrow_mut().as_root_mut().entry_point = Some(res.clone());
        res
    }

    pub fn is_valid_for(&self, path: &PathBuf) -> bool {
        path.starts_with(&self.path)
    }

    pub fn is_public(&self) -> bool {
        self.typ == EntryPointType::PUBLIC || self.typ == EntryPointType::BUILTIN
    }

    pub fn is_main(&self) -> bool {
        self.typ == EntryPointType::MAIN || self.typ == EntryPointType::ADDON
    }

    pub fn get_symbol(&self) -> Option<Rc<RefCell<Symbol>>> {
        let tree = self.addon_to_odoo_tree.as_ref().unwrap_or(&self.tree).clone();
        let symbol = self.root.borrow().get_symbol(&(tree, vec![]), u32::MAX);
        match symbol.len() {
            0 => None,
            1 => Some(symbol[0].clone()),
            _ => panic!("Multiple symbols found for entry point {:?}", self)
        }
    }

    //it assumes that the path is valid for the entry
    pub fn get_tree_for_entry(&self, path: &PathBuf) -> Tree {
        if let Some(addon_to_odoo_path) = self.addon_to_odoo_path.as_ref() {
            let path = path.strip_prefix(&self.path).unwrap();
            let path = PathBuf::from(addon_to_odoo_path.clone()).join(path.to_str().unwrap());
            return path.to_tree();
        }
        //no transformation needed, let's return the tree
        path.to_tree()
    }

    /* Consider the given 'tree' path as updated (or new) and move all symbols that were searching for it
    from the not_found_symbols list to the rebuild list. Return True is something should be rebuilt */
    pub fn search_symbols_to_rebuild(&mut self, session: &mut SessionInfo, path: &String, tree: &Tree) {
        let flat_tree = [tree.0.clone(), tree.1.clone()].concat();
        let mut to_add = [vec![], vec![], vec![], vec![]]; //list of symbols to add after the loop (borrow issue)
        for s in self.not_found_symbols.iter() {
            if s.borrow().typ() == SymType::PACKAGE(PackageType::MODULE) {
                let mut sym = s.borrow_mut();
                if let Some(step) = sym.as_module_package().not_found_data.get(path) {
                    match step {
                        BuildSteps::ARCH | BuildSteps::ARCH_EVAL | BuildSteps::VALIDATION => {
                            to_add[*step as usize].push(s.clone());
                        }
                        _ => {}
                    }
                    sym.as_module_package_mut().not_found_data.remove(path);
                    continue; //as if a data has been found, we won't find anything later, so we can continue the loop
                }
            }
            let mut index: i32 = 0; //i32 sa we could go in negative values
            while (index as usize) < s.borrow().not_found_paths().len() {
                let (step, not_found_tree) = s.borrow().not_found_paths()[index as usize].clone();
                if flat_tree[..cmp::min(not_found_tree.len(), flat_tree.len())] == not_found_tree[..cmp::min(not_found_tree.len(), flat_tree.len())] {
                    match step {
                        BuildSteps::ARCH | BuildSteps::ARCH_EVAL | BuildSteps::VALIDATION => {
                            to_add[step as usize].push(s.clone());
                        }
                        _ => {}
                    }
                    s.borrow_mut().not_found_paths_mut().remove(index as usize);
                    index -= 1;
                }
                index += 1;
            }
        }
        for s in to_add[BuildSteps::ARCH as usize].iter() {
            session.sync_odoo.add_to_rebuild_arch(s.clone());
        }
        for s in to_add[BuildSteps::ARCH_EVAL as usize].iter() {
            session.sync_odoo.add_to_rebuild_arch_eval(s.clone());
        }
        for s in to_add[BuildSteps::VALIDATION as usize].iter() {
            s.borrow_mut().invalidate_sub_functions(session);
            session.sync_odoo.add_to_validations(s.clone());
        }
        self.not_found_symbols.retain(|sym| {
            !sym.borrow().not_found_paths().is_empty() || (sym.borrow().typ() == SymType::PACKAGE(PackageType::MODULE) && !sym.borrow().as_module_package().not_found_data.is_empty())
        });
    }

    pub fn search_rebuild_for_models(&mut self, session: &mut SessionInfo, model_name: OYarn){
        let mut to_add: [Vec<Rc<RefCell<Symbol>>>; 4] = [vec![], vec![], vec![], vec![]]; //list of symbols to add after the loop (borrow issue)
        for sym_rc in self.not_found_symbols_for_models.iter() {
            let mut sym_ref = sym_rc.borrow_mut();
            let Some(not_found_models) = sym_ref.not_found_models_mut() else {
                continue;
            };
            let Some(step) = not_found_models.get(&model_name) else {
                continue;
            };
            match step {
                BuildSteps::ARCH | BuildSteps::ARCH_EVAL | BuildSteps::VALIDATION => {
                    to_add[*step as usize].push(sym_rc.clone());
                }
                _ => {} // unreachable
            }
            not_found_models.remove(&model_name);

        }
        for s in to_add[BuildSteps::ARCH as usize].iter() {
            session.sync_odoo.add_to_rebuild_arch(s.clone());
        }
        for s in to_add[BuildSteps::ARCH_EVAL as usize].iter() {
            session.sync_odoo.add_to_rebuild_arch_eval(s.clone());
        }
        for s in to_add[BuildSteps::VALIDATION as usize].iter() {
            s.borrow_mut().invalidate_sub_functions(session);
            session.sync_odoo.add_to_validations(s.clone());
        }
        self.not_found_symbols_for_models.retain(|sym| {
            !sym.borrow().not_found_models().map(|models| models.is_empty()).unwrap_or(true)
        });

    }
}