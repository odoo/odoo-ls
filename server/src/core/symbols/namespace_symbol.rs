use generational_arena::Index;
use weak_table::PtrWeakHashSet;

use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::{Rc, Weak}};

use crate::threads::SessionInfo;

use super::symbol::Symbol;


#[derive(Debug)]
pub struct NamespaceDirectory {
    pub path: String,
    pub module_symbols: HashMap<String, Index>,
}

#[derive(Debug)]
pub struct NamespaceSymbol {
    pub name: String,
    pub directories: Vec<NamespaceDirectory>,
    pub is_external: bool,
    pub self_index: Option<Index>,
    pub parent: Option<Index>,
    pub in_workspace: bool,
    pub dependencies: [Vec<HashSet<Index>>; 4],
    pub dependents: [Vec<HashSet<Index>>; 3],
}

impl NamespaceSymbol {

    pub fn new(name: String, paths: Vec<String>, is_external: bool) -> Self {
        let mut directories = vec![];
        for p in paths.iter() {
            directories.push(NamespaceDirectory {
                path: p.clone(),
                module_symbols: HashMap::new(),
            })
        }
        Self {
            name,
            directories: directories,
            is_external,
            self_index: None,
            parent: None,
            in_workspace: false,
            dependencies: [
                vec![ //ARCH
                    HashSet::new() //ARCH
                ],
                vec![ //ARCH_EVAL
                    HashSet::new() //ARCH
                ],
                vec![
                    HashSet::new(), // ARCH
                    HashSet::new(), //ARCH_EVAL
                    HashSet::new()  //ODOO
                ],
                vec![
                    HashSet::new(), // ARCH
                    HashSet::new(), //ARCH_EVAL
                    HashSet::new()  //ODOO
                ]],
            dependents: [
                vec![ //ARCH
                    HashSet::new(), //ARCH
                    HashSet::new(), //ARCH_EVAL
                    HashSet::new(), //ODOO
                    HashSet::new(), //VALIDATION
                ],
                vec![ //ARCH_EVAL
                    HashSet::new(), //ODOO
                    HashSet::new() //VALIDATION
                ],
                vec![ //ODOO
                    HashSet::new(), //ODOO
                    HashSet::new()  //VALIDATION
                ]],
        }
    }

    pub fn add_file(&mut self, file: &Symbol) {
        let mut best_index: i32 = -1;
        let mut best_length: i32 = -1;
        let mut index = 0;
        while index < self.directories.len() {
            if file.paths()[0].starts_with(&self.directories[index as usize].path) && self.directories[index as usize].path.len() as i32 > best_length {
                best_index = index as i32;
                best_length = self.directories[index as usize].path.len() as i32;
            }
            index += 1;
        }
        if best_index == -1 {
            panic!("Not valid path found to add the file ({}) to namespace {} with directories {:?}", file.paths()[0], self.name, self.directories);
        } else {
            self.directories[best_index as usize].module_symbols.insert(file.name().clone(), file.self_index().unwrap());
        }
    }

    pub fn paths(&self) -> Vec<String> {
        self.directories.iter().map(|x| {x.path.clone()}).collect()
    }

}