use rustpython_parser::text_size::TextRange;
use rustpython_parser::ast::{Expr, TextSize};
use serde_json::{Value, json};

use crate::constants::*;
use crate::core::evaluation::Evaluation;
use crate::core::odoo::SyncOdoo;
use crate::core::python_arch_eval::PythonArchEval;
use core::panic;
use std::collections::{HashMap, HashSet, VecDeque};
use weak_table::PtrWeakHashSet;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::cell::{RefCell, RefMut};
use std::vec;

use super::symbols::function_symbol::FunctionSymbol;
use super::symbols::module_symbol::ModuleSymbol;
use super::symbols::root_symbol::RootSymbol;
use super::symbols::class_symbol::ClassSymbol;

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
    dependencies: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4],
    dependents: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3],
    pub range: Option<TextRange>,
    pub not_found_paths: Vec<(BuildSteps, Vec<String>)>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: bool,
    pub validation_status: bool,
    pub is_import_variable: bool,
    pub ast: Option<Expr<TextRange>>,
    pub doc_string: Option<String>,

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
            range: None,
            not_found_paths: Vec::new(),
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            odoo_status: false,
            validation_status: false,
            is_import_variable: false,
            ast: None,
            doc_string: None,

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
    pub fn get_dependencies(&self, step: BuildSteps, level: BuildSteps) -> &PtrWeakHashSet<Weak<RefCell<Symbol>>> {
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

    pub fn get_all_dependencies(&self, step: BuildSteps) -> &Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>> {
        if step == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        &self.dependencies[step as usize]
    }

    //Return a HashSet of all 'step' of symbols that require that this symbol is built until 'level';
    pub fn get_dependents(&self, level: BuildSteps, step: BuildSteps) -> &PtrWeakHashSet<Weak<RefCell<Symbol>>> {
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
        self.dependencies[step_i][level_i].insert(symbol.get_arc().unwrap());
        symbol.dependents[level_i][step_i].insert(self.get_arc().unwrap());
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

    pub fn is_symbol_in_parents(&self, symbol: &Rc<RefCell<Symbol>>) -> bool {
        if Rc::ptr_eq(&symbol, &self.get_arc().unwrap()) {
            return true;
        }
        if self.parent.is_none() {
            return false;
        }
        let parent = self.parent.as_ref().unwrap().upgrade().unwrap();
        return parent.borrow_mut().is_symbol_in_parents(symbol);
    }

    pub fn invalidate(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>, step: &BuildSteps) {
        //signals that a change occured to this symbol. "step" indicates which level of change occured.
        //It will trigger rebuild on all dependencies
        let mut vec_to_invalidate: VecDeque<Rc<RefCell<Symbol>>> = VecDeque::from([symbol.clone()]);
        while let Some(ref_to_inv) = vec_to_invalidate.pop_front() {
            let mut_symbol = ref_to_inv.borrow_mut();
            if [SymType::FILE, SymType::PACKAGE].contains(&mut_symbol.sym_type) {
                if *step == BuildSteps::ARCH {
                    for (index, hashset) in mut_symbol.dependents[BuildSteps::ARCH as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Rc::ptr_eq(&sym, &symbol) && !sym.borrow().is_symbol_in_parents(&symbol) {
                                if index == BuildSteps::ARCH as usize {
                                    odoo.add_to_rebuild_arch(sym.clone());
                                } else if index == BuildSteps::ARCH_EVAL as usize {
                                    odoo.add_to_rebuild_arch_eval(sym.clone());
                                } else if index == BuildSteps::ODOO as usize {
                                    odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
                if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL].contains(step) {
                    for (index, hashset) in mut_symbol.dependents[BuildSteps::ARCH_EVAL as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Rc::ptr_eq(&sym, &symbol) && !sym.borrow().is_symbol_in_parents(&symbol) {
                                if index == BuildSteps::ARCH_EVAL as usize {
                                    odoo.add_to_rebuild_arch_eval(sym.clone());
                                } else if index == BuildSteps::ODOO as usize {
                                    odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
                if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL, BuildSteps::ODOO].contains(step) {
                    for (index, hashset) in mut_symbol.dependents[BuildSteps::ODOO as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Rc::ptr_eq(&sym, &symbol) && !sym.borrow().is_symbol_in_parents(&symbol) {
                                if index == BuildSteps::ODOO as usize {
                                    odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    odoo.add_to_validations(sym.clone());
                                }
                            }
                        }
                    }
                }
            }
            for sym in mut_symbol.all_symbols(Some(TextRange::new(TextSize::new(u32::MAX-1), TextSize::new(u32::MAX))), false) {
                vec_to_invalidate.push_back(sym.clone());
            }
        }
    }

    pub fn unload(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        /* Unload the symbol and its children. Mark all dependents symbols as 'to_revalidate' */
        if symbol.borrow().sym_type == SymType::DIRTY {
            panic!("Can't unload dirty symbol");
        }
        let mut vec_to_unload: VecDeque<Rc<RefCell<Symbol>>> = VecDeque::from([symbol.clone()]);
        while vec_to_unload.len() > 0 {
            let ref_to_unload = vec_to_unload.front().unwrap().clone();
            let mut mut_symbol = ref_to_unload.borrow_mut();
            // Unload children first
            let mut found_one = false;
            for sym in mut_symbol.all_symbols(Some(TextRange::new(TextSize::new(u32::MAX-1), TextSize::new(u32::MAX))), false) {
                found_one = true;
                vec_to_unload.push_front(sym.clone());
            }
            if found_one {
                continue;
            } else {
                vec_to_unload.pop_front();
            }
            if DEBUG_MEMORY {
                println!("Unloading symbol {:?} at {:?}", mut_symbol.name, mut_symbol.paths);
            }
            //unload symbol
            let parent = mut_symbol.parent.as_ref().unwrap().upgrade().unwrap().clone();
            let mut parent = parent.borrow_mut();
            drop(mut_symbol);
            parent.remove_symbol(ref_to_unload.clone());
            let mut mut_symbol = ref_to_unload.borrow_mut();
            if mut_symbol._module.is_some() {
                odoo.modules.remove(mut_symbol._module.as_ref().unwrap().dir_name.as_str());
            }
            mut_symbol.sym_type = SymType::DIRTY;
            if vec![SymType::FILE, SymType::PACKAGE].contains(&mut_symbol.sym_type) {
                Symbol::invalidate(odoo, ref_to_unload.clone(), &BuildSteps::ARCH);
            }
        }
    }

    pub fn remove_symbol(&mut self, symbol: Rc<RefCell<Symbol>>) {
        if symbol.borrow().is_file_content() {
            let in_symbols = self.symbols.get(&symbol.borrow().name);
            if in_symbols.is_some() && Rc::ptr_eq(&in_symbols.unwrap(), &symbol) {
                self.symbols.remove(&symbol.borrow().name);
                let mut last: Option<Rc<RefCell<Symbol>>> = None;
                let mut pos: usize = 0;
                for s in self.local_symbols.iter() {
                    pos += 1;
                    if Rc::ptr_eq(s, &symbol) {
                        last = Some(s.clone());
                    }
                }
                if let Some(last) = last {
                    pos -= 1;
                    self.symbols.insert(symbol.borrow().name.clone(), last.clone());
                    self.local_symbols.remove(pos);
                }
            } else {
                let position = self.local_symbols.iter().position(|x| Rc::ptr_eq(x, &symbol));
                if let Some(pos) = position {
                    self.local_symbols.remove(pos);
                }
            }
        } else {
            let in_modules = self.module_symbols.get(&symbol.borrow().name);
            if in_modules.is_some() && Rc::ptr_eq(&in_modules.unwrap(), &symbol) {
                self.module_symbols.remove(&symbol.borrow().name);
            }
        }
        symbol.borrow_mut().parent = None;
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
        if SymType::is_instance(&self.sym_type) && self.evaluation.is_some() && self.evaluation.as_ref().unwrap().is_symbol() && self.evaluation.as_ref().unwrap().as_symbol().unwrap().get_symbol().upgrade().is_some() {
            return Some(self.evaluation.as_ref().unwrap().as_symbol().unwrap().get_symbol().clone());
        }
        return None;
    }

    pub fn follow_ref(symbol: Rc<RefCell<Symbol>>, odoo: &mut SyncOdoo, stop_on_type: bool) -> (Weak<RefCell<Symbol>>, bool) {
        //return a weak ptr to the final symbol, and a bool indicating if this is an instance or not
        let mut sym = symbol.borrow().weak_self.clone().expect("Can't follow ref on symbol that is not in the tree !");
        let mut _sym_upgraded = sym.upgrade().unwrap();
        let mut _sym = symbol.borrow();
        let mut next_ref = _sym.next_ref();
        let can_eval_external = !_sym.is_external;
        let mut instance = SymType::is_instance(&_sym.sym_type);
        while next_ref.is_some() && _sym.evaluation.as_ref().unwrap().is_symbol() {
            instance = _sym.evaluation.as_ref().unwrap().as_symbol().unwrap().instance;
            //TODO update context
            if stop_on_type && ! instance && !_sym.is_import_variable {
                return (sym, instance)
            }
            sym = next_ref.as_ref().unwrap().clone();
            drop(_sym);
            _sym_upgraded = sym.upgrade().unwrap();
            _sym = _sym_upgraded.borrow();
            if _sym.evaluation.is_none() && (!_sym.is_external || can_eval_external) {
                let file_symbol = sym.upgrade().unwrap().borrow().get_in_parents(&vec![SymType::FILE, SymType::PACKAGE], true);
                match file_symbol {
                    Some(file_symbol) => {
                        drop(_sym);
                        if file_symbol.upgrade().expect("invalid weak value").borrow().arch_eval_status == BuildStatus::PENDING &&
                        odoo.is_in_rebuild(&file_symbol.upgrade().unwrap(), BuildSteps::ARCH_EVAL) { //TODO check ARCH ?
                            let mut builder = PythonArchEval::new(file_symbol.upgrade().unwrap());
                            builder.eval_arch(odoo);
                        }
                        _sym = _sym_upgraded.borrow();
                    },
                    None => {}
                }
            }
            next_ref = _sym.next_ref();
        }
        return (sym, instance)
    }

    pub fn add_symbol(&mut self, odoo: &mut SyncOdoo, mut symbol: Symbol) -> Rc<RefCell<Symbol>> {
        let symbol_name = symbol.name.clone();
        if self.is_external {
            symbol.is_external = true;
        }
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
                    Symbol::invalidate(odoo, self.symbols[&symbol_name].clone(), &BuildSteps::ARCH);
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
        if locked_symbol._module.is_some() {
            odoo.modules.insert(locked_symbol._module.as_ref().unwrap().dir_name.clone(), Rc::downgrade(&rc));
        }
        rc.clone()
    }

    pub fn create_from_path(odoo: &mut SyncOdoo, path: &PathBuf, parent: Rc<RefCell<Symbol>>, require_module: bool) -> Option<Rc<RefCell<Symbol>>> {
        let name: String = path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string();
        let path_str = path.to_str().unwrap().to_string();
        if path_str.ends_with(".py") || path_str.ends_with(".pyi") {
            let mut symbol = Symbol::new(name, SymType::FILE);
            symbol.paths = vec![path_str.clone()];
            let ref_sym = (*parent).borrow_mut().add_symbol(odoo, symbol);
            return Some(ref_sym);
        } else {
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                let ref_sym = (*parent).borrow_mut().add_symbol(odoo, Symbol::new(name, SymType::PACKAGE));
                if path.join("__init__.py").exists() {
                    //?
                } else {
                    (*ref_sym).borrow_mut().i_ext = "i".to_string();
                }
                if (*parent).borrow().get_tree().clone() == tree(vec!["odoo", "addons"], vec![]) && path.join("__manifest__.py").exists() {
                    (*ref_sym).borrow_mut().paths = vec![path_str.clone()];
                    let module = ModuleSymbol::new(odoo, path);
                    if module.is_some() {
                        (*ref_sym).borrow_mut()._module = module;
                        ModuleSymbol::load_module_info(ref_sym.clone(), odoo, parent);
                    } else {
                        return None;
                    }
                } else if !require_module {
                    (*ref_sym).borrow_mut().paths = vec![path_str.clone()];
                } else {
                    (*parent).borrow_mut().remove_symbol(ref_sym);
                    return None;
                }
                return Some(ref_sym);
            } else if !require_module{ //TODO should handle module with only __manifest__.py (see odoo/addons/test_data-module)
                let mut symbol = Symbol::new(name, SymType::NAMESPACE);
                symbol.paths = vec![path_str.clone()];
                let ref_sym = (*parent).borrow_mut().add_symbol(odoo, symbol);
                return Some(ref_sym);
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

    pub fn debug_to_json(&self) -> Value {
        let mut modules = vec![];
        let mut symbols = vec![];
        let mut locals = vec![];
        for s in self.module_symbols.values() {
            modules.push(s.borrow_mut().debug_to_json());
        }
        for s in self.symbols.values() {
            symbols.push(s.borrow_mut().debug_to_json());
        }
        for s in self.local_symbols.iter() {
            locals.push(s.borrow_mut().debug_to_json());
        }
        json!({
            "name": self.name,
            "type": self.sym_type.to_string(),
            "module_symbols": modules,
            "symbols": symbols,
            "local_symbols": locals
        })
    }

    pub fn debug_print_graph(&self) -> String {
        println!("starting log");
        let mut res: String = String::new();
        self._debug_print_graph_node(&mut res, 0);
        res
    }

    pub fn all_symbols<'a>(&'a self, position:Option<TextRange>, include_inherits:bool) -> impl Iterator<Item= &'a Rc<RefCell<Symbol>>> + 'a {
        //return an iterator on all symbols of self. If position is set, search in local_symbols too, otherwise only symbols in symbols and module_symbols will
        //be returned. If include_inherits is set, symbols from parent will be included.
        let mut iter: Vec<Box<dyn Iterator<Item = &Rc<RefCell<Symbol>>>>> = Vec::new();
        if position.is_some() {
            let pos = position.as_ref().unwrap().clone();
            let iter = self.local_symbols.iter().filter(move |&x| (**x).borrow().range.unwrap().start() < pos.start());
        }
        if include_inherits {
            //TODO inherits
        }
        if position.is_some() {
            let pos = position.as_ref().unwrap().clone();
            iter.push(Box::new(self.symbols.values().filter(move |&x| (**x).borrow().range.unwrap().start() < pos.start())));
        } else {
            iter.push(Box::new(self.symbols.values()));
        }
        iter.push(Box::new(self.module_symbols.values()));
        iter.into_iter().flatten()
    }

    pub fn infer_name(&self, odoo: &mut SyncOdoo, name: String, position: TextRange) -> Option<Rc<RefCell<Symbol>>> {
        let mut selected: Option<Rc<RefCell<Symbol>>> = None;
        if name == "__doc__" {
            //return self.doc; //TODO
        }
        for symbol in self.all_symbols(Some(position), false) {
            let deref_symbol = (**symbol).borrow();
            let selected_range = selected.as_ref().unwrap();
            let selected_range = (**selected_range).borrow().range;
            if deref_symbol.name == name && (selected.is_none() || deref_symbol.range.unwrap().start() > selected_range.unwrap().start()) {
                selected = Some(symbol.clone());
            }
        }
        if selected.is_none() && !vec![SymType::FILE, SymType::PACKAGE].contains(&self.sym_type) {
            let parent = self.parent.as_ref().unwrap().upgrade().unwrap();
            let parent = (*parent).borrow();
            return parent.infer_name(odoo, name, position);
        }
        if selected.is_none() && (self.name != "builtins" || self.sym_type != SymType::FILE) {
            let builtins = odoo.builtins.as_ref().unwrap().clone(); // clone rc to drop odoo borrow
            let builtins = (*builtins).borrow();
            return builtins.infer_name(odoo, name, position);
        }
        selected
    }
}