use rustpython_parser::text_size::TextRange;

use crate::constants::*;
use crate::my_weak::MyWeak;
use crate::core::evaluation::Evaluation;
use core::panic;
use std::collections::{HashSet, HashMap};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak, MutexGuard};
use std::vec;


pub trait SymbolTrait {
    
}

#[derive(Debug)]
pub struct Symbol {
    pub name: String,
    pub sym_type: SymType,
    pub paths: Vec<String>,
    //eval: Option<Evaluation>,
    i_ext: String,
    pub symbols: HashMap<String, Arc<Mutex<Symbol>>>,
    pub module_symbols: HashMap<String, Arc<Mutex<Symbol>>>,
    pub local_symbols: Vec<Arc<Mutex<Symbol>>>,
    parent: Option<Weak<Mutex<Symbol>>>,
    pub weak_self: Option<Weak<Mutex<Symbol>>>,
    pub evaluation: Option<Evaluation>,
    dependencies: Vec<Vec<HashSet<MyWeak<Mutex<Symbol>>>>>,
    dependents: Vec<Vec<HashSet<MyWeak<Mutex<Symbol>>>>>,
    pub range: Option<TextRange>,
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
            evaluation: None,
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

    pub fn get_symbol(&self, tree: &Tree) -> Option<Arc<Mutex<Symbol>>> {
        let symbol_tree_files: &Vec<String> = &tree.0;
        let symbol_tree_content: &Vec<String> = &tree.1;
        let mut stf = symbol_tree_files.iter();
        let mut content = if let Some(fk) = stf.next() {
            Some(stf.try_fold(
                self.module_symbols.get(fk)?.clone(),
                |c, f| Some(c.lock().unwrap().module_symbols.get(f)?.clone())
            )?)
        } else {
            return None
        };
        let mut stc = symbol_tree_content.into_iter();
        content = if let Some(fk) = stc.next() {
            Some(stf.try_fold(
                content.unwrap().lock().unwrap().module_symbols.get(fk)?.clone(),
                |c, f| Some(c.lock().unwrap().module_symbols.get(f)?.clone())
            )?)
        } else {
            return None
        };
        content
    }

    pub fn get_tree(&self) -> Tree {
        let mut res = (vec![], vec![]);
        if self.is_file_content() {
            res.1.insert(0, self.name.clone());
        } else {
            res.0.insert(0, self.name.clone());
        }
        if self.sym_type == SymType::ROOT || self.parent.is_none() {
            return res
        }
        let parent = self.parent.clone();
        let mut current_arc = parent.as_ref().unwrap().upgrade().unwrap();
        let mut current = current_arc.lock().unwrap();
        while current.sym_type != SymType::ROOT && current.parent.is_some() {
            if current.is_file_content() {
                res.1.insert(0, current.name.clone());
            } else {
                res.0.insert(0, current.name.clone());
            }
            let parent = current.parent.clone();
            drop(current);
            current_arc = parent.as_ref().unwrap().upgrade().unwrap();
            current = current_arc.lock().unwrap();
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

    pub fn add_symbol(&mut self, mut symbol: Symbol) -> Arc<Mutex<Symbol>> {
        let symbol_name = symbol.name.clone();
        let symbol_range = symbol.range.clone();
        let arc = Arc::new(Mutex::new(symbol));
        let mut locked_symbol = arc.lock().unwrap();
        locked_symbol.weak_self = Some(Arc::downgrade(&arc));
        locked_symbol.parent = match self.weak_self {
            Some(ref weak_self) => Some(weak_self.clone()),
            None => panic!("no weak_self set")
        };
        if locked_symbol.is_file_content() {
            if self.symbols.contains_key(&symbol_name) {
                let range: &Option<TextRange> = &symbol_range;
                if range.is_some() && range.unwrap().start() < self.symbols[&symbol_name].lock().unwrap().range.unwrap().start() {
                    self.local_symbols.push(arc.clone());
                } else {
                    self.symbols[&symbol_name].lock().unwrap().invalidate(&BuildSteps::ARCH);
                    self.local_symbols.push(self.symbols[&symbol_name].clone());
                    self.symbols.insert(symbol_name.clone(), arc.clone());
                }
            } else {
                self.symbols.insert(symbol_name.clone(), arc.clone());
            }
        } else {
            self.module_symbols.insert(symbol_name.clone(), arc.clone());
        }
        arc.clone()
    }

    pub fn create_from_path(path: &PathBuf, parent: &MutexGuard<Symbol>, require_module: bool) -> Option<Symbol> {
        let name = path.components().last().unwrap().as_os_str().to_str().unwrap().to_string();
        let path_str = path.to_str().unwrap().to_string();
        if path_str.ends_with(".py") || path_str.ends_with(".pyi") {
            let mut symbol = Symbol::new(name, SymType::FILE);
            symbol.paths = vec![path_str.clone()];
            return Some(symbol);
        } else {
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                let mut symbol = Symbol::new(name, SymType::PACKAGE);
                if parent.get_tree() == tree(vec!["odoo", "addons"], vec![]) && path.join("__manifest__.py").exists() {
                    //TODO adapt to MODULE, not PACKAGE
                    symbol.paths = vec![path_str.clone()];
                    //TODO symbol.load_module_info
                } else if !require_module {
                    //Nothing to do
                } else {
                    return None;
                }
                if path.join("__init__.py").exists() {
                    //?
                } else {
                    symbol.i_ext = "i".to_string();
                }
                return Some(symbol);
            } else if !require_module{
                let mut symbol = Symbol::new(name, SymType::NAMESPACE);
                symbol.paths = vec![path_str.clone()];
                return Some(symbol);
            } else {
                return None
            }
        }
    }

    pub fn get_positioned_symbol(&self, name: &String, range: &TextRange) -> Option<Arc<Mutex<Symbol>>> {
        if let Some(symbol) = self.symbols.get(name) {
            if symbol.lock().unwrap().range.unwrap().start() == range.start() {
                return Some(symbol.clone());
            }
        }
        if let Some(symbol) = self.module_symbols.get(name) {
            if symbol.lock().unwrap().range.unwrap().start() == range.start() {
                return Some(symbol.clone());
            }
        }
        for local_symbol in self.local_symbols.iter() {
            if local_symbol.lock().unwrap().range.unwrap().start() == range.start() {
                return Some(local_symbol.clone());
            }
        }
        None
    }
}