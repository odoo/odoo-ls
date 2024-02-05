use tower_lsp::lsp_types::Range;

use crate::constants::*;
use crate::my_weak::MyWeak;
use core::panic;
use std::collections::{HashSet, HashMap};
use std::ops::Deref;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::vec;


pub trait SymbolTrait {
    
}

#[derive(Debug)]
pub struct Symbol {
    name: String,
    pub sym_type: SymType,
    pub paths: Vec<String>,
    //eval: Option<Evaluation>,
    i_ext: String,
    symbols: HashMap<String, Arc<Mutex<Symbol>>>,
    module_symbols: HashMap<String, Arc<Mutex<Symbol>>>,
    local_symbols: Vec<Arc<Mutex<Symbol>>>,
    parent: Option<Weak<Mutex<Symbol>>>,
    weak_self: Option<Weak<Mutex<Symbol>>>,
    dependencies: Vec<Vec<HashSet<MyWeak<Mutex<Symbol>>>>>,
    dependents: Vec<Vec<HashSet<MyWeak<Mutex<Symbol>>>>>,
    range: Option<Range>,
}

impl Symbol {
    pub fn new(name: String, sym_type: SymType) -> Self {
        Symbol{
            name: name.clone(),
            sym_type: sym_type,
            paths: vec![],
            i_ext: String::new(),
            symbols: HashMap::new(),
            module_symbols: HashMap::new(),
            local_symbols: Vec::new(),
            parent: None,
            weak_self: None,
            dependencies: vec![
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
            dependents: vec![
                vec![
                    HashSet::new(), //ARCH
                    HashSet::new(), //ARCH_EVAL
                    HashSet::new(), //ODOO
                    HashSet::new(), //VALIDATION
                ],
                vec![
                    HashSet::new(), //ODOO
                    HashSet::new() //VALIDATION
                ],
                vec![
                    HashSet::new(), //ODOO
                    HashSet::new()  //VALIDATION
                ]],
            range: None,
        }
    }

    pub fn get_symbol(&self, mut symbol_tree_files: Vec<String>, mut symbol_tree_content: Vec<String>) -> Option<Arc<Mutex<Symbol>>> {
        let mut stf = symbol_tree_files.into_iter();
        let mut content = if let Some(fk) = stf.next() {
            Some(stf.try_fold(
                self.module_symbols.get(&fk)?.clone(),
                |c, f| Some(c.lock().unwrap().module_symbols.get(&f)?.clone())
            )?)
        } else {
            return None
        };
        let mut stc = symbol_tree_content.into_iter();
        content = if let Some(fk) = stc.next() {
            Some(stf.try_fold(
                content.unwrap().lock().unwrap().module_symbols.get(&fk)?.clone(),
                |c, f| Some(c.lock().unwrap().module_symbols.get(&f)?.clone())
            )?)
        } else {
            return None
        };
        content
    }

    pub fn get_tree(&self) -> Vec<Vec<String>> {
        let mut res = vec![vec![]];
        let mut current = self;
        while current.sym_type != SymType::ROOT && current.parent.is_some() {
            if current.is_file_content() {
                res[1].insert(0, current.name.clone());
            } else {
                res[0].insert(0, current.name.clone());
            }
            current = current.parent.unwrap().upgrade().unwrap().lock().unwrap().deref();
        }
        res
    }

    pub fn is_file_content(&self) -> bool{
        return [SymType::NAMESPACE, SymType::PACKAGE, SymType::FILE, SymType::COMPILED].contains(&self.sym_type)
    }

    //Return a HashSet of all symbols (constructed until 'level') that are dependencies for the 'step' of this symbol
    pub fn get_dependencies(&self, step: BuildSteps, level: BuildSteps) -> &HashSet<MyWeak<Mutex<Symbol>>> {
        if step == BuildSteps::SYNTAX || level == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        if level > BuildSteps::ARCH {
            if step < BuildSteps::ODOO {
                panic!("Can't get dependencies for step {:?} and level {:?}", step, level)
            }
            if level == BuildSteps::VALIDATION {
                panic!("Can't get dependencies for step {:?} and level {:?}", step, level)
            }
        }
        &self.dependencies[step as usize][level as usize]
    }

    pub fn get_all_dependencies(&self, step: BuildSteps) -> &Vec<HashSet<MyWeak<Mutex<Symbol>>>> {
        if step == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        &self.dependencies[step as usize]
    }

    //Return a HashSet of all 'step' of symbols that require that this symbol is built until 'level';
    pub fn get_dependents(&self, level: BuildSteps, step: BuildSteps) -> &HashSet<MyWeak<Mutex<Symbol>>> {
        if level == BuildSteps::SYNTAX || step == BuildSteps::SYNTAX {
            panic!("Can't get dependents for syntax step")
        }
        if level == BuildSteps::VALIDATION {
            panic!("Can't get dependents for level {:?}", level)
        }
        if level > BuildSteps::ARCH {
            if step < BuildSteps::ODOO {
                panic!("Can't get dependents for step {:?} and level {:?}", step, level)
            }
        }
        &self.dependents[level as usize][step as usize]
    }

    //Add a symbol as dependency on the step of the other symbol for the build level.
    //-> The build of the 'step' of self requires the build of 'dep_level' of the other symbol to be done
    pub fn add_dependency(&mut self, symbol: &mut Symbol, step:BuildSteps, dep_level:BuildSteps) {
        if step == BuildSteps::SYNTAX || dep_level == BuildSteps::SYNTAX {
            panic!("Can't add dependency for syntax step")
        }
        if dep_level > BuildSteps::ARCH {
            if step < BuildSteps::ODOO {
                panic!("Can't add dependency for step {:?} and level {:?}", step, dep_level)
            }
            if dep_level == BuildSteps::VALIDATION {
                panic!("Can't add dependency for step {:?} and level {:?}", step, dep_level)
            }
        }
        let step_i = step as usize;
        let level_i = dep_level as usize;
        self.dependencies[step_i][level_i].insert(MyWeak::new(Arc::downgrade(&symbol.get_arc().unwrap())));
        symbol.dependents[level_i][step_i].insert(MyWeak::new(Arc::downgrade(&self.get_arc().unwrap())));
    }

    pub fn get_arc(&self) -> Option<Arc<Mutex<Symbol>>> {
        if self.weak_self.is_none() {
            return None;
        }
        if let Some(v) = &self.weak_self {
            return Some(v.upgrade().unwrap());
        }
        None
    }

    pub fn invalidate(&mut self, step: &BuildSteps) {
        //TODO
    }

    pub fn add_symbol(&mut self, symbol: &Arc<Mutex<Symbol>>) {
        let mut sym = &symbol.lock().unwrap();
        if sym.is_file_content() {
            if self.symbols.contains_key(&sym.name) {
                let range: &Option<Range> = &sym.range;
                if range.is_some() && range.unwrap().start.line < self.symbols[&sym.name].lock().unwrap().range.unwrap().start.line {
                    self.local_symbols.push(symbol.clone());
                } else {
                    self.symbols[&sym.name].lock().unwrap().invalidate(&BuildSteps::ARCH);
                    self.local_symbols.push(self.symbols[&sym.name].clone());
                    self.symbols.insert(sym.name.clone(), symbol.clone());
                }
            } else {
                self.symbols.insert(sym.name.clone(), symbol.clone());
            }
        } else {
            self.module_symbols.insert(sym.name.clone(), symbol.clone());
        }
        sym.weak_self = Some(Arc::downgrade(&symbol));
        sym.parent = Some(self.weak_self.unwrap().clone());
    }

    pub async fn create_from_path(path: &str, parent: &Option<Arc<Mutex<Symbol>>>) -> Result<Self, &'static str> {
        if ! path.ends_with(".py") && ! path.ends_with(".pyi") {
            return Err("Path must be a python file");
        }
        let mut symbol = Symbol::new(path.to_string(), SymType::FILE);
        symbol.paths = vec![path.to_string()];
        Ok(symbol)
    }
}