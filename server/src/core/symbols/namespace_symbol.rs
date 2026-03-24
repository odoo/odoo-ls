
use std::{collections::{HashMap, HashSet}, path::PathBuf};

use crate::{constants::OYarn, core::symbols::symbol_table::SymbolKey, oyarn, weak_hash_set::WeakSet};



#[derive(Debug)]
pub struct NamespaceDirectory {
    pub path: String,
    pub module_symbols: HashMap<OYarn, SymbolKey>,
}

#[derive(Debug)]
pub struct NamespaceSymbol {
    pub name: OYarn,
    pub directories: Vec<NamespaceDirectory>,
    pub is_external: bool,
    pub parent: SymbolKey,
    pub(super) in_workspace: bool,
    pub dependencies: Vec<Vec<Option<WeakSet<SymbolKey>>>>,
    pub dependents: Vec<Vec<Option<WeakSet<SymbolKey>>>>,
    // @arena-todo: check if dead code. if not, use WeakSet instead
    pub ext_symbols: HashMap<OYarn, HashSet<SymbolKey>>,
}

impl NamespaceSymbol {

    pub fn new(name: &str, paths: Vec<String>, parent: SymbolKey, is_external: bool) -> Self {
        let directories = paths.into_iter().map(|p| NamespaceDirectory {
            path: p,
            module_symbols: HashMap::new(),
        }).collect();
        Self {
            name: oyarn!("{}", name),
            directories,
            is_external,
            parent,
            in_workspace: false,
            dependencies: vec![],
            dependents: vec![],
            ext_symbols: HashMap::new(),
        }
    }

    // @arena originaly got paths() from symbol and used only the first one (paths()[0])
    pub fn add_file(&mut self, file: SymbolKey, name: &str, path: &str) {
        let mut best_index: i32 = -1;
        let mut best_length: i32 = -1;
        let mut index = 0;
        while index < self.directories.len() {
            if PathBuf::from(path).starts_with(&self.directories[index].path) && self.directories[index].path.len() as i32 > best_length {
                best_index = index as i32;
                best_length = self.directories[index].path.len() as i32;
            }
            index += 1;
        }
        if best_index == -1 {
            panic!("Not valid path found to add the file ({}) to namespace {} with directories {:?}", path, self.name, self.directories);
        } else {
            self.directories[best_index as usize].module_symbols.insert(oyarn!("{}", name), file);
        }
    }

    pub fn paths(&self) -> Vec<String> {
        self.directories.iter().map(|x| {x.path.clone()}).collect()
    }

    // @arena: originally a branch in Symbol::add_path
    pub fn add_path(&mut self, path: String) {
        self.directories.push(NamespaceDirectory { path: path, module_symbols: HashMap::new() });
    }

    // @arena: moved to SymbolTable
    // pub fn get_ext_symbol(&self, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
    //     let mut result = vec![];
    //     if let Some(owners) = self.ext_symbols.get(name) {
    //         for owner in owners.iter() {
    //             let owner = owner.borrow();
    //             result.extend(owner.get_decl_ext_symbol(&self.weak_self.as_ref().unwrap().upgrade().unwrap(), name));
    //         }
    //     }
    //     result
    // }

}
