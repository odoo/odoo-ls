use byteyarn::Yarn;
use serde_json::json;
use weak_table::PtrWeakHashSet;

use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use crate::{constants::{BuildSteps, SymType}, tool_api::to_json::{dependencies_to_json, dependents_to_json}};

use super::symbol::Symbol;


#[derive(Debug)]
pub struct NamespaceDirectory {
    pub path: String,
    pub module_symbols: HashMap<Yarn, Rc<RefCell<Symbol>>>,
}

#[derive(Debug)]
pub struct NamespaceSymbol {
    pub name: Yarn,
    pub directories: Vec<NamespaceDirectory>,
    pub is_external: bool,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    in_workspace: bool,
    pub dependencies: Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>,
    pub dependents: Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>,
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
            name: Yarn::from_string(name),
            directories: directories,
            is_external,
            weak_self: None,
            parent: None,
            in_workspace: false,
            dependencies: vec![],
            dependents: vec![],
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

    pub fn get_dependencies(&self, step: usize, level: usize) -> Option<&PtrWeakHashSet<Weak<RefCell<Symbol>>>>
    {
        self.dependencies.get(step)?.get(level)?.as_ref()
    }

    pub fn get_all_dependencies(&self, step: usize) -> Option<&Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>
    {
        self.dependencies.get(step)
    }

    pub fn dependencies(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &self.dependencies
    }

    pub fn dependencies_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &mut self.dependencies
    }

    pub fn dependents(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &self.dependents
    }

    pub fn dependents_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &mut self.dependents
    }

    pub fn get_dependents(&self, level: usize, step: usize) -> Option<&PtrWeakHashSet<Weak<RefCell<Symbol>>>>
    {
        self.dependents.get(level)?.get(step)?.as_ref()
    }

    pub fn get_all_dependents(&self, level: usize) -> Option<&Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>
    {
        self.dependents.get(level)
    }

    pub fn set_in_workspace(&mut self, in_workspace: bool) {
        self.in_workspace = in_workspace;
        if in_workspace {
            self.dependencies= vec![
                vec![ //ARCH
                    None //ARCH
                ],
                vec![ //ARCH_EVAL
                    None //ARCH
                ],
                vec![
                    None, // ARCH
                    None, //ARCH_EVAL
                    None  //ODOO
                ],
                vec![
                    None, // ARCH
                    None, //ARCH_EVAL
                    None  //ODOO
                ]];
            self.dependents = vec![
                vec![ //ARCH
                    None, //ARCH
                    None, //ARCH_EVAL
                    None, //ODOO
                    None, //VALIDATION
                ],
                vec![ //ARCH_EVAL
                    None, //ODOO
                    None //VALIDATION
                ],
                vec![ //ODOO
                    None, //ODOO
                    None  //VALIDATION
                ]];
        }
    }

    pub fn is_in_workspace(&self) -> bool {
        self.in_workspace
    }
    
    pub fn to_json(&self) -> serde_json::Value {
        let mut directories = vec![];
        for directory in self.directories.iter() {
            let module_sym: Vec<serde_json::Value> = directory.module_symbols.values().map(|sym| {
                json!({
                    "name": sym.borrow().name().clone(),
                    "type": sym.borrow().typ().to_string(),
                })
            }).collect();
            directories.push(json!({
                "path": directory.path,
                "module_symbols": module_sym,
            }));
        }
        json!({
            "type": SymType::NAMESPACE.to_string(),
            "is_external": self.is_external,
            "in_workspace": self.in_workspace,
            "directories": directories,
            "dependencies": dependencies_to_json(&self.dependencies),
            "dependents": dependents_to_json(&self.dependents),
        })
    }

}