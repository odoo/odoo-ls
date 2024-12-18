use weak_table::PtrWeakHashSet;

use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use crate::constants::BuildSteps;

use super::symbol::Symbol;


#[derive(Debug)]
pub struct NamespaceDirectory {
    pub path: String,
    pub module_symbols: HashMap<String, Rc<RefCell<Symbol>>>,
}

#[derive(Debug)]
pub struct NamespaceSymbol {
    pub name: String,
    pub directories: Vec<NamespaceDirectory>,
    pub is_external: bool,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub in_workspace: bool,
    pub dependencies: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4],
    pub dependents: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3],
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
            weak_self: None,
            parent: None,
            in_workspace: false,
            dependencies: [
                vec![ //ARCH
                    PtrWeakHashSet::new() //ARCH
                ],
                vec![ //ARCH_EVAL
                    PtrWeakHashSet::new() //ARCH
                ],
                vec![
                    PtrWeakHashSet::new(), // ARCH
                    PtrWeakHashSet::new(), //ARCH_EVAL
                    PtrWeakHashSet::new()  //ODOO
                ],
                vec![
                    PtrWeakHashSet::new(), // ARCH
                    PtrWeakHashSet::new(), //ARCH_EVAL
                    PtrWeakHashSet::new()  //ODOO
                ]],
            dependents: [
                vec![ //ARCH
                    PtrWeakHashSet::new(), //ARCH
                    PtrWeakHashSet::new(), //ARCH_EVAL
                    PtrWeakHashSet::new(), //ODOO
                    PtrWeakHashSet::new(), //VALIDATION
                ],
                vec![ //ARCH_EVAL
                    PtrWeakHashSet::new(), //ODOO
                    PtrWeakHashSet::new() //VALIDATION
                ],
                vec![ //ODOO
                    PtrWeakHashSet::new(), //ODOO
                    PtrWeakHashSet::new()  //VALIDATION
                ]],
        }
    }

    pub fn add_file(&mut self, file: &Rc<RefCell<Symbol>>) {
        let mut best_index: i32 = -1;
        let mut best_length: i32 = -1;
        let mut index = 0;
        while index < self.directories.len() {
            if file.borrow().paths()[0].starts_with(&self.directories[index].path) && self.directories[index].path.len() as i32 > best_length {
                best_index = index as i32;
                best_length = self.directories[index].path.len() as i32;
            }
            index += 1;
        }
        if best_index == -1 {
            panic!("Not valid path found to add the file ({}) to namespace {} with directories {:?}", file.borrow().paths()[0], self.name, self.directories);
        } else {
            self.directories[best_index as usize].module_symbols.insert(file.borrow().name().clone(), file.clone());
        }
    }

    pub fn paths(&self) -> Vec<String> {
        self.directories.iter().map(|x| {x.path.clone()}).collect()
    }

}