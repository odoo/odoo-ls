
use std::collections::{HashMap, HashSet};

use crate::{constants::OYarn, core::symbols::symbol_keys::SymbolKey, oyarn, weak_collections::WeakSet};



#[derive(Debug)]
pub struct NamespaceDirectory {
    pub path: String,
    pub(super) module_symbols: HashMap<OYarn, SymbolKey>,
}

#[derive(Debug)]
pub struct NamespaceSymbol {
    pub name: OYarn,
    pub(super) directories: Vec<NamespaceDirectory>,
    pub is_external: bool,
    parent: SymbolKey,
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

    pub fn paths(&self) -> Vec<String> {
        self.directories.iter().map(|x| {x.path.clone()}).collect()
    }

    // @arena: originally a branch in Symbol::add_path
    pub fn add_path(&mut self, path: String) {
        self.directories.push(NamespaceDirectory { path: path, module_symbols: HashMap::new() });
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
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
