use rustpython_parser::text_size::TextRange;

use crate::constants::*;
use crate::my_weak::MyWeak;
use crate::core::evaluation::Evaluation;
use crate::core::odoo::SyncOdoo;
use crate::core::python_arch_eval::PythonArchEval;
use core::panic;
use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::cell::{RefCell, RefMut};
use std::vec;

use super::symbols::function_symbol::FunctionSymbol;
use super::symbols::module_symbol::ModuleSymbol;
use super::symbols::root_symbol::RootSymbol;
use super::symbols::class_symbol::ClassSymbol;


pub trait SymbolTrait {
    
}

#[derive(Debug)]
pub struct Symbol {
    pub name: String,
    pub sym_type: SymType,
    pub paths: Vec<String>,
    //eval: Option<Evaluation>,
    pub i_ext: String,
    pub is_external: bool,
    pub symbols: HashMap<String, Rc<RefCell<Symbol>>>,
    pub module_symbols: HashMap<String, Rc<RefCell<Symbol>>>,
    pub local_symbols: Vec<Rc<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub evaluation: Option<Evaluation>,
    dependencies: Vec<Vec<HashSet<MyWeak<RefCell<Symbol>>>>>,
    dependents: Vec<Vec<HashSet<MyWeak<RefCell<Symbol>>>>>,
    pub range: Option<TextRange>,
    pub not_found_paths: HashMap<BuildSteps, String>,
    pub arch_status: bool,
    pub arch_eval_status: bool,
    pub odoo_status: bool,
    pub validation_status: bool,
    pub is_import_variable: bool,

    pub _root: Option<RootSymbol>,
    pub _function: Option<FunctionSymbol>,
    pub _class: Option<ClassSymbol>,
    pub _module: Option<ModuleSymbol>,
}

impl Symbol {
    pub fn new(name: String, sym_type: SymType) -> Self {
        Symbol{
            name: name.clone(),
            sym_type: sym_type,
            paths: vec![],
            i_ext: String::new(),
            is_external: false,
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
            not_found_paths: HashMap::new(),
            arch_status: false,
            arch_eval_status: false,
            odoo_status: false,
            validation_status: false,
            is_import_variable: false,

            _root: None,
            _function: None,
            _class: None,
            _module: None,
        }
    }

    pub fn new_root(name: String, sym_type: SymType) -> Self {
        let mut new_sym = Symbol::new(name, sym_type);
        new_sym._root = Some(RootSymbol{sys_path: vec![]});
        new_sym
    }

    pub fn new_function(name: String, sym_type: SymType, is_property: bool) -> Self {
        let mut new_sym = Symbol::new(name, sym_type);
        new_sym._function = Some(FunctionSymbol{is_property: is_property});
        new_sym
    }

    pub fn new_class(name: String, sym_type: SymType) -> Self {
        let mut new_sym = Symbol::new(name, sym_type);
        new_sym._class = Some(ClassSymbol{bases: HashSet::new()});
        new_sym
    }

    pub fn new_module(name: String, sym_type: SymType) -> Self {
        let mut new_sym = Symbol::new(name, sym_type);
        new_sym._module = Some(ModuleSymbol::new());
        new_sym
    }

    pub fn get_symbol(&self, tree: &Tree) -> Option<Rc<RefCell<Symbol>>> {
        let symbol_tree_files: &Vec<String> = &tree.0;
        let symbol_tree_content: &Vec<String> = &tree.1;
        let mut iter_sym: Option<Rc<RefCell<Symbol>>> = None;
        if symbol_tree_files.len() != 0 {
            iter_sym = self.module_symbols.get(&symbol_tree_files[0]).cloned();
            if iter_sym.is_none() {
                return None;
            }
            if symbol_tree_files.len() > 1 {
                for fk in symbol_tree_files[1..symbol_tree_files.len()].iter() {
                    if let Some(s) = iter_sym.unwrap().borrow_mut().module_symbols.get(fk) {
                        iter_sym = Some(s.clone());
                    } else {
                        return None;
                    }
                }
            }
            if symbol_tree_content.len() != 0 {
                for fk in symbol_tree_content.iter() {
                    if let Some(s) = iter_sym.unwrap().borrow_mut().symbols.get(fk) {
                        iter_sym = Some(s.clone());
                    } else {
                        return None;
                    }
                }
            }
        } else {
            if symbol_tree_content.len() == 0 {
                return None;
            }
            iter_sym = self.symbols.get(&symbol_tree_content[0]).cloned();
            if iter_sym.is_none() {
                return None;
            }
            if symbol_tree_content.len() >1 {
                for fk in symbol_tree_content[1..symbol_tree_content.len()].iter() {
                    if let Some(s) = iter_sym.unwrap().borrow_mut().symbols.get(fk) {
                        iter_sym = Some(s.clone());
                    } else {
                        return None;
                    }
                }
            }
        }
        iter_sym
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
        let mut current = current_arc.borrow_mut();
        while current.sym_type != SymType::ROOT && current.parent.is_some() {
            if current.is_file_content() {
                res.1.insert(0, current.name.clone());
            } else {
                res.0.insert(0, current.name.clone());
            }
            let parent = current.parent.clone();
            drop(current);
            current_arc = parent.as_ref().unwrap().upgrade().unwrap();
            current = current_arc.borrow_mut();
        }
        res
    }

    pub fn is_file_content(&self) -> bool{
        return ! [SymType::NAMESPACE, SymType::PACKAGE, SymType::FILE, SymType::COMPILED].contains(&self.sym_type)
    }

    //Return a HashSet of all symbols (constructed until 'level') that are dependencies for the 'step' of this symbol
    pub fn get_dependencies(&self, step: BuildSteps, level: BuildSteps) -> &HashSet<MyWeak<RefCell<Symbol>>> {
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

    pub fn get_all_dependencies(&self, step: BuildSteps) -> &Vec<HashSet<MyWeak<RefCell<Symbol>>>> {
        if step == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        &self.dependencies[step as usize]
    }

    //Return a HashSet of all 'step' of symbols that require that this symbol is built until 'level';
    pub fn get_dependents(&self, level: BuildSteps, step: BuildSteps) -> &HashSet<MyWeak<RefCell<Symbol>>> {
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
        self.dependencies[step_i][level_i].insert(MyWeak::new(Rc::downgrade(&symbol.get_arc().unwrap())));
        symbol.dependents[level_i][step_i].insert(MyWeak::new(Rc::downgrade(&self.get_arc().unwrap())));
    }

    pub fn get_arc(&self) -> Option<Rc<RefCell<Symbol>>> {
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

    pub fn get_in_parents(&self, sym_types: &Vec<SymType>, stop_same_file: bool) -> Option<Weak<RefCell<Symbol>>> {
        if sym_types.contains(&self.sym_type) {
            return self.weak_self.clone();
        }
        if stop_same_file && vec![SymType::FILE, SymType::PACKAGE].contains(&self.sym_type) {
            return None;
        }
        if self.parent.is_some() {
            return self.parent.as_ref().unwrap().upgrade().unwrap().borrow_mut().get_in_parents(sym_types, stop_same_file);
        }
        return None;
    }

    pub fn next_ref(&self) -> Option<Weak<RefCell<Symbol>>> {
        if SymType::is_instance(&self.sym_type) && self.evaluation.is_some() && self.evaluation.as_ref().unwrap().get_symbol().upgrade().is_some() {
            return Some(self.evaluation.as_ref().unwrap().get_symbol());
        }
        return None;
    }

    pub fn follow_ref(symbol: Rc<RefCell<Symbol>>, odoo: &mut SyncOdoo, stop_on_type: bool) -> (Weak<RefCell<Symbol>>, bool) {
        let mut sym = symbol.borrow_mut().weak_self.clone().expect("Can't follow ref on symbol that is not in the tree !");
        let mut _sym_upgraded = sym.upgrade().unwrap();
        let mut _sym = symbol.borrow_mut();
        let mut next_ref = _sym.next_ref();
        let can_eval_external = !_sym.is_external;
        let mut instance = SymType::is_instance(&_sym.sym_type);
        while next_ref.is_some() {
            instance = _sym.evaluation.as_ref().unwrap().instance;
            //TODO update context
            if stop_on_type && ! instance && !_sym.is_import_variable {
                return (sym, instance)
            }
            sym = next_ref.as_ref().unwrap().clone();
            drop(_sym);
            _sym_upgraded = sym.upgrade().unwrap();
            _sym = _sym_upgraded.borrow_mut();
            if _sym.evaluation.is_none() && (!_sym.is_external || can_eval_external) {
                let file_symbol = sym.upgrade().unwrap().borrow_mut().get_in_parents(&vec![SymType::FILE, SymType::PACKAGE], true);
                match file_symbol {
                    Some(file_symbol) => {
                        drop(_sym);
                        if !file_symbol.upgrade().expect("invalid weak value").borrow_mut().arch_eval_status &&
                        odoo.is_in_rebuild(&file_symbol, BuildSteps::ARCH_EVAL) { //TODO check ARCH ?
                            let mut builder = PythonArchEval::new(file_symbol.upgrade().unwrap());
                            builder.eval_arch(odoo);
                        }
                        _sym = _sym_upgraded.borrow_mut();
                    },
                    None => {}
                }
            }
            next_ref = _sym.next_ref();
        }
        return (sym, instance)
    }

    pub fn add_symbol(&mut self, odoo: &SyncOdoo, mut symbol: Symbol) -> Rc<RefCell<Symbol>> {
        let symbol_name = symbol.name.clone();
        let symbol_range = symbol.range.clone();
        let rc = Rc::new(RefCell::new(symbol));
        let mut locked_symbol = rc.borrow_mut();
        locked_symbol.weak_self = Some(Rc::downgrade(&rc));
        locked_symbol.parent = match self.weak_self {
            Some(ref weak_self) => Some(weak_self.clone()),
            None => panic!("no weak_self set")
        };
        if locked_symbol.is_file_content() {
            if self.symbols.contains_key(&symbol_name) {
                let range: &Option<TextRange> = &symbol_range;
                if range.is_some() && range.unwrap().start() < self.symbols[&symbol_name].borrow_mut().range.unwrap().start() {
                    self.local_symbols.push(rc.clone());
                } else {
                    self.symbols[&symbol_name].borrow_mut().invalidate(&BuildSteps::ARCH);
                    self.local_symbols.push(self.symbols[&symbol_name].clone());
                    self.symbols.insert(symbol_name.clone(), rc.clone());
                }
            } else {
                self.symbols.insert(symbol_name.clone(), rc.clone());
            }
        } else {
            self.module_symbols.insert(symbol_name.clone(), rc.clone());
        }
        if self._root.is_some() {
            self._root.as_ref().unwrap().add_symbol(odoo, &self, &mut locked_symbol);
        }
        rc.clone()
    }

    pub fn create_from_path(path: &PathBuf, parent: &RefMut<Symbol>, require_module: bool) -> Option<Symbol> {
        let name: String = path.components().last().unwrap().as_os_str().to_str().unwrap().to_string();
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
                    symbol.paths = vec![path_str.clone()];
                } else {
                    return None;
                }
                if path.join("__init__.py").exists() {
                    //?
                } else {
                    symbol.i_ext = "i".to_string();
                }
                return Some(symbol);
            } else if !require_module{ //TODO should handle module with only __manifest__.py (see odoo/addons/test_data-module)
                let mut symbol = Symbol::new(name, SymType::NAMESPACE);
                symbol.paths = vec![path_str.clone()];
                return Some(symbol);
            } else {
                return None
            }
        }
    }

    pub fn get_positioned_symbol(&self, name: &String, range: &TextRange) -> Option<Rc<RefCell<Symbol>>> {
        if let Some(symbol) = self.symbols.get(name) {
            if symbol.borrow_mut().range.unwrap().start() == range.start() {
                return Some(symbol.clone());
            }
        }
        if let Some(symbol) = self.module_symbols.get(name) {
            if symbol.borrow_mut().range.unwrap().start() == range.start() {
                return Some(symbol.clone());
            }
        }
        for local_symbol in self.local_symbols.iter() {
            if local_symbol.borrow_mut().range.unwrap().start() == range.start() {
                return Some(local_symbol.clone());
            }
        }
        None
    }

    fn _debug_print_graph_node(&self, acc: &mut String, level: u32) {
        for _ in 0..level {
            acc.push_str(" ");
        }
        acc.push_str(format!("{:?} {:?}\n", self.sym_type, self.name).as_str());
        if self.module_symbols.len() > 0 {
            for _ in 0..level {
                acc.push_str(" ");
            }
            acc.push_str("MODULES:\n");
            for (_, module) in self.module_symbols.iter() {
                module.borrow_mut()._debug_print_graph_node(acc, level + 1);
            }
        }
        if self.symbols.len() > 0 {
            for _ in 0..level {
                acc.push_str(" ");
            }
            acc.push_str("SYMBOLS:\n");
            for (_, module) in self.symbols.iter() {
                module.borrow_mut()._debug_print_graph_node(acc, level + 1);
            }
        }
        if self.module_symbols.len() > 0 {
            for _ in 0..level {
                acc.push_str(" ");
            }
            acc.push_str("LOCALS:\n");
            for symbol in self.local_symbols.iter() {
                symbol.borrow_mut()._debug_print_graph_node(acc, level + 1);
            }
        }
    }

    pub fn debug_print_graph(&self) -> String {
        let mut res: String = String::new();
        self._debug_print_graph_node(&mut res, 0);
        res
    }
}