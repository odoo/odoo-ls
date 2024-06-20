use crate::core::config::{Config, ConfigRequest};
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use tower_lsp::lsp_types::*;
use tower_lsp::Client;

use std::collections::HashSet;
use weak_table::PtrWeakHashSet;
use std::process::Command;
use std::str::FromStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::env;
use std::cmp;
use regex::Regex;
use crate::constants::*;
use super::config::{DiagMissingImportsMode, RefreshMode};
use super::file_mgr::FileMgr;
use super::symbol::Symbol;
use crate::core::model::Model;
use crate::core::python_arch_builder::PythonArchBuilder;
use crate::core::python_arch_eval::PythonArchEval;
use crate::core::python_odoo_builder::PythonOdooBuilder;
use crate::core::python_validator::PythonValidator;
use crate::core::messages::{Msg, MsgHandler};
use crate::S;
//use super::python_arch_builder::PythonArchBuilder;

#[derive(Debug)]
pub struct SyncOdoo {
    pub version_major: u32,
    pub version_minor: u32,
    pub version_micro: u32,
    pub full_version: String,
    pub config: Config,
    pub symbols: Option<Rc<RefCell<Symbol>>>,
    pub stubs_dirs: Vec<String>,
    pub stdlib_dir: String,
    file_mgr: Rc<RefCell<FileMgr>>,
    pub modules: HashMap<String, Weak<RefCell<Symbol>>>,
    pub models: HashMap<String, Rc<RefCell<Model>>>,
    rebuild_arch: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_arch_eval: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_odoo: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    rebuild_validation: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub not_found_symbols: PtrWeakHashSet<Weak<RefCell<Symbol>>>,
    pub msg_sender: MsgHandler,
    pub load_odoo_addons: bool //indicate if we want to load odoo addons or not
}

unsafe impl Send for SyncOdoo {}

impl SyncOdoo {

    pub fn new(msg_sender: MsgHandler) -> Self {
        let symbols = Rc::new(RefCell::new(Symbol::new_root("root".to_string(), SymType::ROOT)));
        symbols.borrow_mut().weak_self = Some(Rc::downgrade(&symbols)); // manually set weakself for root symbols
        let sync_odoo = Self {
            version_major: 0,
            version_minor: 0,
            version_micro: 0,
            full_version: "0.0.0".to_string(),
            config: Config::new(),
            symbols: Some(symbols),
            file_mgr: Rc::new(RefCell::new(FileMgr::new())),
            stubs_dirs: vec![env::current_dir().unwrap().join("typeshed").join("stubs").to_str().unwrap().to_string(),
            env::current_dir().unwrap().join("additional_stubs").to_str().unwrap().to_string()],
            stdlib_dir: env::current_dir().unwrap().join("typeshed").join("stdlib").to_str().unwrap().to_string(),
            modules: HashMap::new(),
            models: HashMap::new(),
            rebuild_arch: PtrWeakHashSet::new(),
            rebuild_arch_eval: PtrWeakHashSet::new(),
            rebuild_odoo: PtrWeakHashSet::new(),
            rebuild_validation: PtrWeakHashSet::new(),
            not_found_symbols: PtrWeakHashSet::new(),
            msg_sender,
            load_odoo_addons: true
        };
        sync_odoo
    }

    pub fn init(&mut self, config: Config) {
        println!("Initializing odoo");
        self.config = config;
        if self.config.no_typeshed {
            self.stubs_dirs.clear();
        }
        for stub in self.config.additional_stubs.iter() {
            self.stubs_dirs.push(stub.clone());
        }
        if !self.config.stdlib.is_empty() {
            self.stdlib_dir = self.config.stdlib.clone();
        }
        println!("Using stdlib path: {}", self.stdlib_dir);
        for stub in self.stubs_dirs.iter() {
            let path = Path::new(stub);
            let found = match path.exists() {
                true  => "found",
                false => "not found",
            };
            println!("stub {:?} - {}", stub, found)
        }
        {
            let mut root_symbol = self.symbols.as_ref().unwrap().borrow_mut();
            root_symbol.paths.push(self.stdlib_dir.clone());
            for stub_dir in self.stubs_dirs.iter() {
                root_symbol.paths.push(stub_dir.clone());
            }
            let output = Command::new(self.config.python_path.clone()).args(&["-c", "import sys; print(sys.path)"]).output().expect("Can't exec python3");
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                self.msg_sender.send(Msg::LOG_INFO(format!("Detected sys.path: {}", stdout)));
                // extract vec of string from output
                if stdout.len() > 5 {
                    let values = String::from((stdout[2..stdout.len()-3]).to_string());
                    for value in values.split("', '") {
                        let value = value.to_string();
                        if value.len() > 0 {
                            let pathbuf = PathBuf::from(value.clone());
                            if pathbuf.is_dir() {
                                self.msg_sender.send(Msg::LOG_INFO(format!("Adding sys.path: {}", stdout)));
                                root_symbol.paths.push(value.clone());
                                root_symbol._root.as_mut().unwrap().sys_path.push(value.clone());
                            }
                        }
                    }
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("{}", stderr);
            }
        }
        self.load_builtins();
        self.build_database();
    }

    pub fn load_builtins(&mut self) {
        let path = PathBuf::from(&self.stdlib_dir);
        let builtins_path = path.join("builtins.pyi");
        if !builtins_path.exists() {
            self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to find builtins.pyi")));
            println!("Unable to find builtins at: {}", builtins_path.as_os_str().to_str().unwrap());
            return;
        };
        let _builtins_rc_symbol = Symbol::create_from_path(self, &builtins_path, self.symbols.as_ref().unwrap().clone(), false);
        self.add_to_rebuild_arch(_builtins_rc_symbol.unwrap());
        self.process_rebuilds();
    }

    pub fn build_database(&mut self) {
        self.msg_sender.send(Msg::LOG_INFO(String::from("Building Database")));
        let result = self.build_base();
        if result {
            self.build_modules();
        }
    }

    fn build_base(&mut self) -> bool {
        let odoo_path = self.config.odoo_path.clone();
        let release_path = PathBuf::from(odoo_path.clone()).join("odoo/release.py");
        let odoo_addon_path = PathBuf::from(odoo_path.clone()).join("addons");
        if !release_path.exists() {
            self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to find release.py - Aborting")));
            return false;
        }
        // open release.py and get version
        let release_file = fs::read_to_string(release_path);
        let release_file = match release_file {
            Ok(release_file) => release_file,
            Err(_) => {
                self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to read release.py - Aborting")));
                return false;
            }
        };
        let mut _version_major: u32 = 14;
        let mut _version_minor: u32 = 0;
        let mut _version_micro: u32 = 0;
        let mut _full_version: String = "14.0.0".to_string();
        for line in release_file.lines() {
            if line.starts_with("version_info = (") {
                let re = Regex::new(r#"version_info = \((['\"]?(\D+~)?\d+['\"]?, \d+, \d+, \w+, \d+, \D+)\)"#).unwrap();
                let result = re.captures(line);
                match result {
                    Some(result) => {
                        let version_info = result.get(1).unwrap().as_str();
                        let version_info = version_info.split(", ").collect::<Vec<&str>>();
                        let version_major = version_info[0].replace("saas~", "").replace("'", "").replace(r#"""#, "");
                        _version_major = version_major.parse().unwrap();
                        _version_minor = version_info[1].parse().unwrap();
                        _version_micro = version_info[2].parse().unwrap();
                        _full_version = format!("{}.{}.{}", _version_major, _version_minor, _version_micro);
                        break;
                    },
                    None => {
                        self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to detect the Odoo version. Running the tool for the version 14")));
                        return false;
                    }
                }
            }
        }
        self.msg_sender.send(Msg::LOG_INFO(format!("Odoo version: {}", _full_version)));
        if _version_major < 14 {
            self.msg_sender.send(Msg::LOG_ERROR(String::from("Odoo version is less than 14. The tool only supports version 14 and above. Aborting...")));
            return false;
        }
        self.version_major = _version_major;
        self.version_minor = _version_minor;
        self.version_micro = _version_micro;
        self.full_version = _full_version;
        //build base
        self.symbols.as_ref().unwrap().borrow_mut().paths.push(self.config.odoo_path.clone());
        if self.symbols.is_none() {
            panic!("Odoo root symbol not found")
        }
        let root_symbol = self.symbols.as_ref().unwrap().clone();
        let config_odoo_path = self.config.odoo_path.clone();
        let added_symbol = Symbol::create_from_path(self, &PathBuf::from(config_odoo_path).join("odoo"),  root_symbol, false);
        self.add_to_rebuild_arch(added_symbol.unwrap());
        self.process_rebuilds();
        //search common odoo addons path
        let addon_symbol = self.get_symbol(&tree(vec!["odoo", "addons"], vec![]));
        if addon_symbol.is_none() {
            let odoo = self.get_symbol(&tree(vec!["odoo"], vec![]));
            if odoo.is_none() {
                panic!("Not able to find odoo. Please check your configuration");
            }
            panic!("Not able to find odoo/addons. Please check your configuration");
        }
        if odoo_addon_path.exists() {
            if self.load_odoo_addons {
                addon_symbol.as_ref().unwrap().borrow_mut().paths.push(
                    odoo_addon_path.to_str().unwrap().to_string()
                );
            }
        } else {
            let odoo_addon_path = PathBuf::from(odoo_path.clone()).join("addons");
            self.msg_sender.send(Msg::LOG_ERROR(format!("Unable to find odoo addons path at {}", odoo_addon_path.to_str().unwrap().to_string())));
            return false;
        }
        for addon in self.config.addons.iter() {
            let addon_path = PathBuf::from(addon);
            if addon_path.exists() {
                addon_symbol.as_ref().unwrap().borrow_mut().paths.push(
                    addon_path.to_str().unwrap().to_string()
                );
            }
        }
        return true;
    }

    fn build_modules(&mut self) {
        {
            let addons_symbol = self.get_symbol(&tree(vec!["odoo", "addons"], vec![])).expect("Unable to find odoo addons symbol");
            let addons_path = addons_symbol.borrow_mut().paths.clone();
            for addon_path in addons_path.iter() {
                println!("searching modules in {}", addon_path);
                if PathBuf::from(addon_path).exists() {
                    //browse all dir in path
                    for item in PathBuf::from(addon_path).read_dir().expect("Unable to find odoo addons path") {
                        match item {
                            Ok(item) => {
                                if item.file_type().unwrap().is_dir() && !self.modules.contains_key(&item.file_name().to_str().unwrap().to_string()) {
                                    let module_symbol = Symbol::create_from_path(self, &item.path(), addons_symbol.clone(), true);
                                    if module_symbol.is_some() {
                                        self.add_to_rebuild_arch(module_symbol.unwrap());
                                    }
                                }
                            },
                            Err(_) => {}
                        }
                    }
                }
            }
        }
        self.process_rebuilds();
        //println!("{}", self.symbols.as_ref().unwrap().borrow_mut().debug_print_graph());
        //fs::write("out_architecture.json", self.get_symbol(&tree(vec!["odoo", "addons", "module_1"], vec![])).as_ref().unwrap().borrow().debug_to_json().to_string()).expect("Unable to write file");
        println!("End building modules");
        self.msg_sender.send(Msg::LOG_INFO(String::from("End building modules.")));
    }

    pub fn get_symbol(&self, tree: &Tree) -> Option<Rc<RefCell<Symbol>>> {
        self.symbols.as_ref().unwrap().borrow_mut().get_symbol(&tree)
    }

    fn pop_item(&mut self, step: BuildSteps) -> Option<Rc<RefCell<Symbol>>> {
        let mut arc_sym: Option<Rc<RefCell<Symbol>>> = None;
        //Part 1: Find the symbol with a unmutable set
        {
            let set =  if step == BuildSteps::ARCH_EVAL {
                &self.rebuild_arch_eval
            } else if step == BuildSteps::ODOO {
                &self.rebuild_odoo
            } else if step == BuildSteps::VALIDATION {
                &self.rebuild_validation
            } else {
                &self.rebuild_arch
            };
            let mut selected_sym: Option<Rc<RefCell<Symbol>>> = None;
            let mut selected_count: u32 = 999999999;
            let mut current_count: u32;
            for sym in &*set {
                current_count = 0;
                let symbol = sym.borrow_mut();
                for (index, dep_set) in symbol.get_all_dependencies(step).iter().enumerate() {
                    if index == BuildSteps::ARCH as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_arch.contains(&dep) {
                                current_count += 1;
                            }
                        }
                    }
                    if index == BuildSteps::ARCH_EVAL as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_arch_eval.contains(&dep) {
                                current_count += 1;
                            }
                        }
                    }
                    if index == BuildSteps::ODOO as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_odoo.contains(&dep) {
                                current_count += 1;
                            }
                        }
                    }
                    if index == BuildSteps::VALIDATION as usize {
                        for dep in dep_set.iter() {
                            if self.rebuild_validation.contains(&dep) {
                                current_count += 1;
                            }
                        }
                    }
                }
                if current_count < selected_count {
                    selected_sym = Some(sym.clone());
                    selected_count = current_count;
                    if current_count == 0 {
                        break;
                    }
                }
            }
            if selected_sym.is_some() {
                arc_sym = selected_sym.map(|x| x.clone());
            }
        }
        {
            let set =  if step == BuildSteps::ARCH_EVAL {
                &mut self.rebuild_arch_eval
            } else if step == BuildSteps::ODOO {
                &mut self.rebuild_odoo
            } else if step == BuildSteps::VALIDATION {
                &mut self.rebuild_validation
            } else {
                &mut self.rebuild_arch
            };
            if arc_sym.is_none() {
                set.clear(); //remove any potential dead weak ref
                return None;
            }
            let arc_sym_unwrapped = arc_sym.unwrap();
            if !set.remove(&arc_sym_unwrapped) {
                panic!("Unable to remove selected symbol from rebuild set")
            }
            return Some(arc_sym_unwrapped);
        }
    }

    fn process_rebuilds(&mut self) {
        let mut already_arch_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_arch_eval_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_odoo_rebuilt: HashSet<Tree> = HashSet::new();
        let mut already_validation_rebuilt: HashSet<Tree> = HashSet::new();
        while !self.rebuild_arch.is_empty() || !self.rebuild_arch_eval.is_empty() || !self.rebuild_odoo.is_empty() || !self.rebuild_validation.is_empty(){
            println!("remains: {:?} - {:?} - {:?} - {:?}", self.rebuild_arch.len(), self.rebuild_arch_eval.len(), self.rebuild_odoo.len(), self.rebuild_validation.len());
            let sym = self.pop_item(BuildSteps::ARCH);
            if let Some(sym_rc) = sym {
                let tree = sym_rc.borrow().get_tree();
                if already_arch_rebuilt.contains(&tree) {
                    println!("Already arch rebuilt, skipping");
                    continue;
                }
                already_arch_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut builder = PythonArchBuilder::new(sym_rc);
                builder.load_arch(self);
                continue;
            }
            let sym = self.pop_item(BuildSteps::ARCH_EVAL);
            if let Some(sym_rc) = sym {
                let tree = sym_rc.borrow().get_tree();
                if already_arch_eval_rebuilt.contains(&tree) {
                    println!("Already arch eval rebuilt, skipping");
                    continue;
                }
                already_arch_eval_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut builder = PythonArchEval::new(sym_rc);
                builder.eval_arch(self);
                continue;
            }
            let sym = self.pop_item(BuildSteps::ODOO);
            if let Some(sym_rc) = sym {
                let tree = sym_rc.borrow().get_tree();
                if already_odoo_rebuilt.contains(&tree) {
                    println!("Already odoo rebuilt, skipping");
                    continue;
                }
                already_odoo_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut builder = PythonOdooBuilder::new(sym_rc);
                builder.load_odoo_content(self);
                continue;
            }
            let sym = self.pop_item(BuildSteps::VALIDATION);
            if let Some(sym_rc) = sym {
                let tree = sym_rc.borrow_mut().get_tree();
                if already_validation_rebuilt.contains(&tree) {
                    println!("Already validation rebuilt, skipping");
                    continue;
                }
                already_validation_rebuilt.insert(tree);
                //TODO should delete previous first
                let mut validator = PythonValidator::new(sym_rc);
                validator.validate(self);
                continue;
            }
        }
    }

    pub fn rebuild_arch_now(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_arch.remove(symbol);
        let mut builder = PythonArchBuilder::new(symbol.clone());
        builder.load_arch(self);
    }

    pub fn add_to_rebuild_arch(&mut self, symbol: Rc<RefCell<Symbol>>) {
        //println!("ADDED TO ARCH - {}", symbol.borrow().paths.first().unwrap());
        self.rebuild_arch.insert(symbol);
    }

    pub fn add_to_rebuild_arch_eval(&mut self, symbol: Rc<RefCell<Symbol>>) {
        //println!("ADDED TO EVAL - {}", symbol.borrow().paths.first().unwrap());
        self.rebuild_arch_eval.insert(symbol);
    }

    pub fn add_to_init_odoo(&mut self, symbol: Rc<RefCell<Symbol>>) {
        //println!("ADDED TO ODOO - {}", symbol.borrow().paths.first().unwrap());
        self.rebuild_odoo.insert(symbol);
    }

    pub fn add_to_validations(&mut self, symbol: Rc<RefCell<Symbol>>) {
        //println!("ADDED TO VALIDATION - {}", symbol.borrow().paths.first().unwrap());
        self.rebuild_validation.insert(symbol);
    }

    pub fn remove_from_rebuild_arch(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_arch.remove(symbol);
    }

    pub fn remove_from_rebuild_arch_eval(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_arch_eval.remove(symbol);
    }

    pub fn remove_from_rebuild_odoo(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_odoo.remove(symbol);
    }

    pub fn remove_from_rebuild_validation(&mut self, symbol: &Rc<RefCell<Symbol>>) {
        self.rebuild_validation.remove(symbol);
    }

    pub fn is_in_rebuild(&self, symbol: &Rc<RefCell<Symbol>>, step: BuildSteps) -> bool {
        if step == BuildSteps::ARCH {
            return self.rebuild_arch.contains(symbol);
        }
        if step == BuildSteps::ARCH_EVAL {
            return self.rebuild_arch_eval.contains(symbol);
        }
        if step == BuildSteps::ODOO {
            return self.rebuild_odoo.contains(symbol);
        }
        if step == BuildSteps::VALIDATION {
            return self.rebuild_validation.contains(symbol);
        }
        false
    }

    pub fn get_file_mgr(&mut self) -> Rc<RefCell<FileMgr>> {
        self.file_mgr.clone()
    }

    /* Path must be absolute. Return a valid tree according the root paths and odoo/addons path. The given
    tree may not be in the graph however */
    pub fn tree_from_path(&self, path: &PathBuf) -> Result<Tree, &str> {
        //First check in odoo, before anywhere else
        {
            let odoo_sym = self.symbols.as_ref().unwrap().borrow().get_symbol(&tree(vec!["odoo", "addons"], vec![]));
            for addon_path in odoo_sym.unwrap().borrow().paths.iter() {
                if path.starts_with(addon_path) {
                    let path = path.strip_prefix(addon_path).unwrap().to_path_buf();
                    let mut tree: Tree = (vec![S!("odoo"), S!("addons")], vec![]);
                    path.components().for_each(|c| {
                        tree.0.push(c.as_os_str().to_str().unwrap().replace(".py", "").replace(".pyi", "").to_string());
                    });
                    if vec!["__init__", "__manifest__"].contains(&tree.0.last().unwrap().as_str()) {
                        tree.0.pop();
                    } 
                    return Ok(tree);
                }
            }
        }
        for root_path in self.symbols.as_ref().unwrap().borrow().paths.iter() {
            if path.starts_with(root_path) {
                let path = path.strip_prefix(root_path).unwrap().to_path_buf();
                let mut tree: Tree = (vec![], vec![]);
                path.components().for_each(|c| {
                    tree.0.push(c.as_os_str().to_str().unwrap().replace(".py", "").to_string());
                });
                if tree.0.len() > 0 && vec!["__init__", "__manifest__"].contains(&tree.0.last().unwrap().as_str()) {
                    tree.0.pop();
                } 
                return Ok(tree);
            }
        }
        Err("Path not found in any module")
    }

    pub fn _unload_path(&mut self, path: &PathBuf, clean_cache: bool) -> Result<Rc<RefCell<Symbol>>, &str> {
        let symbol = self.symbols.as_ref().unwrap().borrow();
        let path_symbol = symbol.get_symbol(&self.tree_from_path(&path).unwrap());
        if path_symbol.is_none() {
            return Err("Symbol not found");
        }
        let path_symbol = path_symbol.unwrap();
        let parent = path_symbol.borrow().parent.clone().unwrap().upgrade().unwrap();
        if clean_cache {
            let mut file_mgr = self.file_mgr.borrow_mut();
            file_mgr.delete_path(self, &path.as_os_str().to_str().unwrap().to_string());
            let mut to_del = Vec::from_iter(path_symbol.borrow_mut().module_symbols.values().map(|x| x.clone()));
            let mut index = 0;
            while index < to_del.len() {
                file_mgr.delete_path(self, &to_del[index].borrow().paths[0]);
                let mut to_del_child = Vec::from_iter(to_del[index].borrow().module_symbols.values().map(|x| x.clone()));
                to_del.append(&mut to_del_child);
                index += 1;
            }
        }
        drop(symbol);
        Symbol::unload(self, path_symbol);
        Ok(parent)
    }

    pub fn create_new_symbol(&mut self, path: PathBuf, parent: Rc<RefCell<Symbol>>, require_module: bool) -> Option<(Rc<RefCell<Symbol>>,Tree)> {
        let mut path = path.clone();
        if path.ends_with("__init__.py") || path.ends_with("__init__.pyi") || path.ends_with("__manifest__.py") {
            path.pop();
        }
        let _arc_symbol = Symbol::create_from_path(self, &path, parent, require_module);
        if _arc_symbol.is_some() {
            let _arc_symbol = _arc_symbol.unwrap();
            self.add_to_rebuild_arch(_arc_symbol.clone());
            return Some((_arc_symbol.clone(), _arc_symbol.borrow().get_tree().clone()));
        }
        None
    }

    /* Consider the given 'tree' path as updated (or new) and move all symbols that were searching for it
        from the not_found_symbols list to the rebuild list. Return True is something should be rebuilt */
    pub fn search_symbols_to_rebuild(&mut self, tree: &Tree) -> bool {
        let flat_tree = vec![tree.0.clone(), tree.1.clone()].concat();
        let mut found_sym: PtrWeakHashSet<Weak<RefCell<Symbol>>> = PtrWeakHashSet::new();
        let mut need_rebuild = false;
        for s in self.not_found_symbols.iter() {
            let mut index: i32 = 0; //i32 sa we could go in negative values
            while (index as usize) < s.borrow().not_found_paths.len() {
                let (step, not_found_tree) = s.borrow().not_found_paths[index as usize].clone();
                if flat_tree[..cmp::min(not_found_tree.len(), flat_tree.len())] == not_found_tree[..cmp::min(not_found_tree.len(), flat_tree.len())] {
                    need_rebuild = true;
                    match step {
                        BuildSteps::ARCH => {
                            self.rebuild_arch.insert(s.clone());
                        },
                        BuildSteps::ARCH_EVAL => {
                            self.rebuild_arch_eval.insert(s.clone());
                        },
                        BuildSteps::ODOO => {
                            self.rebuild_odoo.insert(s.clone());
                        },
                        BuildSteps::VALIDATION => {
                            self.rebuild_validation.insert(s.clone());
                        },
                        _ => {}
                    }
                    s.borrow_mut().not_found_paths.remove(index as usize);
                    index -= 1;
                }
                index += 1;
            }
            if s.borrow().not_found_paths.len() == 0 {
                found_sym.insert(s.clone());
            }
        }
        for sym in found_sym.iter() {
            self.not_found_symbols.remove(&sym);
        }
        need_rebuild
    }

    pub fn get_file_symbol(&self, path: &PathBuf) -> Option<Rc<RefCell<Symbol>>> {
        let symbol = self.symbols.as_ref().unwrap().borrow();
        let tree = &self.tree_from_path(&path);
        if let Ok(tree) = tree {
            return symbol.get_symbol(tree);
        } else {
            println!("Path {} not found", path.to_str().expect("unable to stringify path"));
            None
        }
    }
}

#[derive(Debug)]
pub struct Odoo {
    pub odoo: Arc<Mutex<SyncOdoo>>,
    pub msg_sender: tokio::sync::mpsc::Sender<Msg>,
}

impl Odoo {
    pub fn new(sx: tokio::sync::mpsc::Sender<Msg>) -> Self {
        let odoo = Arc::new(Mutex::new(SyncOdoo::new(MsgHandler::TOKIO_MPSC(sx.clone()))));
        Self {
            odoo: odoo,
            msg_sender: sx.clone(),
        }
    }

    pub async fn init(&mut self, client: &Client) {
        self.msg_sender.send(Msg::LOG_INFO(String::from("Building new Odoo knowledge database"))).await.expect("Unable to send message");
        let response = client.send_request::<ConfigRequest>(()).await.unwrap();
        let _odoo = self.odoo.clone();
        let configuration_item = ConfigurationItem{
            scope_uri: None,
            section: Some("Odoo".to_string()),
        };
        let config = client.configuration(vec![configuration_item]).await.unwrap();
        let config = config.get(0);
        if !config.is_some() {
            self.msg_sender.send(Msg::LOG_ERROR(String::from("No config found for Odoo. Exiting..."))).await.expect("Unable to send message");
            std::process::exit(1);
        }
        let config = config.unwrap();
        //values for sync block
        let mut _refresh_mode : RefreshMode = RefreshMode::OnSave;
        let mut _auto_save_delay : u64 = 2000;
        let mut _diag_missing_imports : DiagMissingImportsMode = DiagMissingImportsMode::All;
        if let Some(map) = config.as_object() {
            for (key, value) in map {
                match key.as_str() {
                    "autoRefresh" => {
                        if let Some(refresh_mode) = value.as_str() {
                            _refresh_mode = match RefreshMode::from_str(refresh_mode) {
                                Ok(mode) => mode,
                                Err(_) => {
                                    self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to parse RefreshMode. Setting it to onSave"))).await.expect("Unable to send message");
                                    RefreshMode::OnSave
                                }
                            };
                        }
                    },
                    "autoRefreshDelay" => {
                        if let Some(refresh_delay) = value.as_u64() {
                            _auto_save_delay = refresh_delay;
                        } else {
                            self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to parse auto_save_delay. Setting it to 2000"))).await.expect("Unable to send message");
                            _auto_save_delay = 2000
                        }
                    },
                    "diagMissingImportLevel" => {
                        if let Some(diag_import_level) = value.as_str() {
                            _diag_missing_imports = match DiagMissingImportsMode::from_str(diag_import_level) {
                                Ok(mode) => mode,
                                Err(_) => {
                                    self.msg_sender.send(Msg::LOG_ERROR(String::from("Unable to parse diag_import_level. Setting it to all"))).await.expect("Unable to send message");
                                    DiagMissingImportsMode::All
                                }
                            };
                        }
                    },
                    _ => {
                        self.msg_sender.send(Msg::LOG_ERROR(format!("Unknown config key: {}", key))).await.expect("Unable to send message");
                    },
                }
            }
        }
        tokio::task::spawn_blocking(move || {
            let mut sync_odoo = _odoo.lock().unwrap();
            let mut config = Config::new();
            config.addons = response.addons.clone();
            config.odoo_path = response.odoo_path.clone();
            config.python_path = response.python_path.clone();
            config.refresh_mode = _refresh_mode;
            config.auto_save_delay = _auto_save_delay;
            config.diag_missing_imports = _diag_missing_imports;
            sync_odoo.init(config);
        }).await.unwrap();
    }

    pub async fn reload_file(&mut self, path: PathBuf, content: Vec<TextDocumentContentChangeEvent>, version: i32, force: bool) {
        if path.extension().is_some() && path.extension().unwrap() == "py" {
            self.msg_sender.blocking_send(Msg::LOG_INFO(format!("File Change Event: {}, version {}", path.to_str().unwrap(), version)));
            let _odoo = self.odoo.clone();
            tokio::task::spawn_blocking(move || {
                let mut sync_odoo = _odoo.lock().unwrap();
                let odoo = &mut sync_odoo;
                let file_info = odoo.get_file_mgr().borrow_mut().update_file_info(odoo, &path.as_os_str().to_str().unwrap().to_string(), Some(&content), Some(version), force);
                let mut mut_file_info = file_info.borrow_mut();
                mut_file_info.publish_diagnostics(odoo); //To push potential syntax errors or refresh previous one
                drop(mut_file_info);
                let parent = odoo._unload_path(&path, false);
                if parent.is_err() {
                    println!("An error occured while reloading file. Ignoring");
                    return;
                }
                let parent = parent.unwrap();
                //build new
                let result = odoo.create_new_symbol(path.clone(), parent, false);
                if let Some((symbol, tree)) = result {
                    //search for missing symbols
                    odoo.search_symbols_to_rebuild(&tree);
                }
                odoo.process_rebuilds();
            }).await.expect("An error occured while executing reload_file sync block");
        }
    }

}