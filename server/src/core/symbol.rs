use ruff_text_size::{TextSize, TextRange};
use serde_json::{Value, json};

use crate::constants::*;
use crate::core::evaluation::{Context, Evaluation};
use crate::core::odoo::SyncOdoo;
use crate::core::model::ModelData;
use crate::core::python_arch_eval::PythonArchEval;
use crate::threads::SessionInfo;
use crate::S;
use core::panic;
use std::collections::{HashMap, VecDeque};
use weak_table::PtrWeakHashSet;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::vec;
use lsp_types::Diagnostic;

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
    pub parent: Option<Weak<RefCell<Symbol>>>, //parent can be None only on detached symbol, like proxys (super() for example)
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub evaluation: Option<Evaluation>,
    dependencies: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4],
    dependents: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3],
    pub range: Option<TextRange>,
    pub not_found_paths: Vec<(BuildSteps, Vec<String>)>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub is_import_variable: bool,
    pub doc_string: Option<String>,
    pub ast_indexes: Option<Vec<u16>>, //list of index to reach the corresponding ast node from file ast
    pub in_workspace: bool,

    pub _root: Option<RootSymbol>,
    pub _function: Option<FunctionSymbol>,
    pub _class: Option<ClassSymbol>,
    pub _module: Option<ModuleSymbol>,
    pub _model: Option<ModelData>,
}

impl Symbol {
    pub fn new(name: String, sym_type: SymType) -> Self {
        if name == "Command" && sym_type == SymType::FILE {
            println!("HO");
        }
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
            odoo_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            is_import_variable: false,
            doc_string: None,
            ast_indexes: None,
            in_workspace: false,

            _root: None,
            _function: None,
            _class: None,
            _module: None,
            _model: None,
        }
    }

    pub fn new_root(name: String, sym_type: SymType) -> Self {
        let mut new_sym = Symbol::new(name, sym_type);
        new_sym._root = Some(RootSymbol{sys_path: vec![]});
        new_sym
    }

    pub fn new_class(name: String, sym_type: SymType) -> Self {
        let mut new_sym = Symbol::new(name, sym_type);
        new_sym._class = Some(ClassSymbol{bases: PtrWeakHashSet::new(), diagnostics: vec![]});
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
        self.dependencies[step_i][level_i].insert(symbol.get_rc().unwrap());
        symbol.dependents[level_i][step_i].insert(self.get_rc().unwrap());
    }

    pub fn get_rc(&self) -> Option<Rc<RefCell<Symbol>>> {
        if self.weak_self.is_none() {
            return None;
        }
        if let Some(v) = &self.weak_self {
            return Some(v.upgrade().unwrap());
        }
        None
    }

    pub fn is_symbol_in_parents(&self, symbol: &Rc<RefCell<Symbol>>) -> bool {
        if Rc::ptr_eq(&symbol, &self.get_rc().unwrap()) {
            return true;
        }
        if self.parent.is_none() {
            return false;
        }
        let parent = self.parent.as_ref().unwrap().upgrade().unwrap();
        return parent.borrow_mut().is_symbol_in_parents(symbol);
    }

    pub fn invalidate(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>, step: &BuildSteps) {
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
                                    session.sync_odoo.add_to_rebuild_arch(sym.clone());
                                } else if index == BuildSteps::ARCH_EVAL as usize {
                                    session.sync_odoo.add_to_rebuild_arch_eval(sym.clone());
                                } else if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    session.sync_odoo.add_to_validations(sym.clone());
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
                                    session.sync_odoo.add_to_rebuild_arch_eval(sym.clone());
                                } else if index == BuildSteps::ODOO as usize {
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    session.sync_odoo.add_to_validations(sym.clone());
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
                                    session.sync_odoo.add_to_init_odoo(sym.clone());
                                } else if index == BuildSteps::VALIDATION as usize {
                                    session.sync_odoo.add_to_validations(sym.clone());
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

    pub fn unload(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>) {
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
                session.sync_odoo.modules.remove(mut_symbol._module.as_ref().unwrap().dir_name.as_str());
            }
            mut_symbol.sym_type = SymType::DIRTY;
            if vec![SymType::FILE, SymType::PACKAGE].contains(&mut_symbol.sym_type) {
                Symbol::invalidate(session, ref_to_unload.clone(), &BuildSteps::ARCH);
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

    pub fn next_ref(&self, session: &mut SessionInfo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> Option<Weak<RefCell<Symbol>>> {
        if SymType::is_instance(&self.sym_type) &&
            self.evaluation.is_some() &&
            self.evaluation.as_ref().unwrap().symbol.get_symbol(session, context, diagnostics).0.upgrade().is_some() {
            return Some(self.evaluation.as_ref().unwrap().symbol.get_symbol(session, context, diagnostics).0.clone());
        }
        return None;
    }

    pub fn follow_ref(symbol: Rc<RefCell<Symbol>>, session: &mut SessionInfo, context: &mut Option<Context>, stop_on_type: bool, stop_on_value: bool, diagnostics: &mut Vec<Diagnostic>) -> (Weak<RefCell<Symbol>>, bool) {
        //return a weak ptr to the final symbol, and a bool indicating if this is an instance or not
        let mut sym = Rc::downgrade(&symbol);
        let mut _sym_upgraded = sym.upgrade().unwrap();
        let mut _sym = symbol.borrow();
        let mut next_ref = _sym.next_ref(session, context, diagnostics);
        let can_eval_external = !_sym.is_external;
        let mut instance = SymType::is_instance(&_sym.sym_type);
        while next_ref.is_some() {
            instance = _sym.evaluation.as_ref().unwrap().symbol.instance;
            if _sym.evaluation.as_ref().unwrap().symbol.context.len() > 0 && context.is_some() {
                context.as_mut().unwrap().extend(_sym.evaluation.as_ref().unwrap().symbol.context.clone());
            }
            if stop_on_type && ! instance && !_sym.is_import_variable {
                return (sym, instance)
            }
            if stop_on_value && _sym.evaluation.as_ref().unwrap().value.is_some() {
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
                        session.sync_odoo.is_in_rebuild(&file_symbol.upgrade().unwrap(), BuildSteps::ARCH_EVAL) { //TODO check ARCH ?
                            let mut builder = PythonArchEval::new(file_symbol.upgrade().unwrap());
                            builder.eval_arch(session);
                        }
                        _sym = _sym_upgraded.borrow();
                    },
                    None => {}
                }
            }
            next_ref = _sym.next_ref(session, context, diagnostics);
        }
        return (sym, instance)
    }

    pub fn add_symbol(&mut self, session: &mut SessionInfo, mut symbol: Symbol) -> Rc<RefCell<Symbol>> {
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
                    Symbol::invalidate(session, self.symbols[&symbol_name].clone(), &BuildSteps::ARCH);
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
            self._root.as_ref().unwrap().add_symbol(session, &self, &mut locked_symbol);
        }
        if locked_symbol._module.is_some() {
            session.sync_odoo.modules.insert(locked_symbol._module.as_ref().unwrap().dir_name.clone(), Rc::downgrade(&rc));
        }
        rc.clone()
    }

    pub fn add_symbol_to_locals(&mut self, odoo: &mut SyncOdoo, mut symbol: Symbol) -> Rc<RefCell<Symbol>> {
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
        self.local_symbols.push(rc.clone());
        rc.clone()
    }

    pub fn create_from_path(session: &mut SessionInfo, path: &PathBuf, parent: Rc<RefCell<Symbol>>, require_module: bool) -> Option<Rc<RefCell<Symbol>>> {
        let name: String = path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string();
        let path_str = path.to_str().unwrap().to_string();
        if path_str.ends_with(".py") || path_str.ends_with(".pyi") {
            let mut symbol = Symbol::new(name, SymType::FILE);
            symbol.paths = vec![path_str.clone()];
            let ref_sym = (*parent).borrow_mut().add_symbol(session, symbol);
            return Some(ref_sym);
        } else {
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                let mut new_sym = Symbol::new(name, SymType::PACKAGE);
                new_sym.paths = vec![path_str.clone()];
                let ref_sym = (*parent).borrow_mut().add_symbol(session, new_sym);
                if path.join("__init__.py").exists() {
                    //?
                } else {
                    (*ref_sym).borrow_mut().i_ext = "i".to_string();
                }
                if (*parent).borrow().get_tree().clone() == tree(vec!["odoo", "addons"], vec![]) && path.join("__manifest__.py").exists() {
                    (*ref_sym).borrow_mut().paths = vec![path_str.clone()];
                    let module = ModuleSymbol::new(session, path);
                    if module.is_some() {
                        (*ref_sym).borrow_mut()._module = module;
                        ModuleSymbol::load_module_info(ref_sym.clone(), session, parent);
                        //as the symbol has been added to parent before module creation, it has not been added to modules
                        session.sync_odoo.modules.insert((*ref_sym).borrow()._module.as_ref().unwrap().dir_name.clone(), Rc::downgrade(&ref_sym));
                    } else {
                        return None;
                    }
                } else if require_module {
                    (*parent).borrow_mut().remove_symbol(ref_sym);
                    return None;
                }
                return Some(ref_sym);
            } else if !require_module{ //TODO should handle module with only __manifest__.py (see odoo/addons/test_data-module)
                let mut symbol = Symbol::new(name, SymType::NAMESPACE);
                symbol.paths = vec![path_str.clone()];
                let ref_sym = (*parent).borrow_mut().add_symbol(session, symbol);
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

    //infer a name, given a position
    pub fn infer_name(odoo: &mut SyncOdoo, on_symbol: &Rc<RefCell<Symbol>>, name: &String, position: Option<TextSize>) -> Option<Rc<RefCell<Symbol>>> {
        let mut selected: Option<Rc<RefCell<Symbol>>> = None;
        if name == "__doc__" {
            //return self.doc; //TODO
        }
        if name == "super" { //build temporary super Symbol
            let class = on_symbol.borrow().get_in_parents(&vec![SymType::CLASS], true);
            if let Some(class) = class {
                let class = class.upgrade();
                if let Some(class) = class {
                    let mut symbol = Symbol::new(
                        S!("super"),
                        SymType::FUNCTION
                    );
                    symbol.parent = None;
                    symbol._function = Some(FunctionSymbol{
                        is_static: true,
                        is_property: false,
                        diagnostics: vec![]
                    });
                    symbol.evaluation = Some(Evaluation::eval_from_symbol(&class));
                    selected = Some(Rc::new(RefCell::new(symbol)));
                    return selected;
                }
            }
        }
        if let Some(rc) = on_symbol.borrow().symbols.get(name) {
            if position.is_none() || rc.borrow().range.unwrap().start() < position.unwrap() {
                selected = Some(rc.clone());
            }
        }
        if selected.is_none() && position.is_some() {
            let position = position.unwrap();
            for local_symbol in on_symbol.borrow().local_symbols.iter() {
                if local_symbol.borrow().name.eq(name) && local_symbol.borrow_mut().range.unwrap().start() < position {
                    if selected.is_none() || selected.as_ref().unwrap().borrow().range.unwrap().start() < local_symbol.borrow_mut().range.unwrap().start() {
                        selected = Some(local_symbol.clone());
                    }
                }
            }
        }
        if selected.is_none() && !vec![SymType::FILE, SymType::PACKAGE, SymType::ROOT].contains(&on_symbol.borrow().sym_type) {
            let parent = on_symbol.borrow().parent.as_ref().unwrap().upgrade().unwrap();
            return Symbol::infer_name(odoo, &parent, name, position);
        }
        if selected.is_none() && (on_symbol.borrow().name != "builtins" || on_symbol.borrow().sym_type != SymType::FILE) {
            let builtins = odoo.get_symbol(&(vec![S!("builtins")], vec![])).as_ref().unwrap().clone();
            return Symbol::infer_name(odoo, &builtins, name, None);
        }
        selected
    }

    /* similar to get_symbol: will return the symbol that is under this one with the specified name.
        However, if the symbol is a class or a model, it will search in the base class or in comodel classes
        if not all, it will return the first found. If all, the all found symbols are returned, but the first one
        is the one that is overriding others.
        :param: from_module: optional, can change the from_module of the given class */
    pub fn get_member_symbol(&self, session: &mut SessionInfo, name: &String, from_module: Option<Rc<RefCell<Symbol>>>, prevent_local: bool, prevent_comodel: bool, all: bool, diagnostics: &mut Vec<Diagnostic>) -> Vec<Rc<RefCell<Symbol>>> {
        let mut result: Vec<Rc<RefCell<Symbol>>> = vec![];
        if self.module_symbols.contains_key(name) {
            if all {
                result.push(self.module_symbols[name].clone());
            } else {
                return vec![self.module_symbols[name].clone()];
            }
        }
        if !prevent_local {
            if self.symbols.contains_key(name) {
                if all {
                    result.push(self.symbols[name].clone());
                } else {
                    return vec![self.symbols[name].clone()];
                }
            }
        }
        if self._model.is_some() && !prevent_comodel {
            let model = session.sync_odoo.models.get(&self._model.as_ref().unwrap().name);
            if let Some(model) = model {
                let symbols = model.clone().borrow().get_symbols(session, from_module.clone().unwrap_or(self.get_module_sym().expect("unable to find module")));
                for sym in symbols {
                    if Rc::ptr_eq(&sym, &self.get_rc().unwrap()) {
                        continue;
                    }
                    let attribut = sym.borrow().get_member_symbol(session, name, None, false, true, all, diagnostics);
                    if all {
                        result.extend(attribut);
                    } else {
                        return attribut;
                    }
                }
            }
        }
        if !all && result.len() != 0 {
            return result;
        }
        if self._class.is_some() {
            for base in self._class.as_ref().unwrap().bases.iter() {
                let s = base.borrow().get_member_symbol(session, name, from_module.clone(), prevent_local, prevent_comodel, all, diagnostics);
                if s.len() != 0 {
                    if all {
                        result.extend(s);
                    } else {
                        return s;
                    }
                }
            }
        }
        result
    }

    pub fn get_sorted_symbols(&self) -> impl Iterator<Item = Rc<RefCell<Symbol>>> {
        let mut symbols: Vec<Rc<RefCell<Symbol>>> = Vec::new();
        symbols.extend(self.local_symbols.iter().cloned());
        symbols.extend(self.symbols.values().cloned());
        symbols.sort_by_key(|s| s.borrow().range.unwrap().start());
        symbols.into_iter()
    }

    pub fn get_module_sym(&self) -> Option<Rc<RefCell<Symbol>>> {
        if self._module.is_some() {
            return self.get_rc();
        }
        if let Some(parent) = self.parent.as_ref() {
            return parent.upgrade().unwrap().borrow().get_module_sym();
        }
        return None;
    }

    /* return the symbol (class or function) the closest to the given offset */
    pub fn get_scope_symbol(sym: Rc<RefCell<Symbol>>, offset: u32) -> Rc<RefCell<Symbol>> {
        //TODO search in localSymbols too
        let mut symbol = sym.clone();
        for s in sym.borrow().symbols.values() {
            if s.borrow().range.unwrap().start().to_u32() < offset && s.borrow().range.unwrap().end().to_u32() >= offset && vec![SymType::CLASS, SymType::FUNCTION].contains(&s.borrow().sym_type) {
                symbol = Symbol::get_scope_symbol(s.clone(), offset);
                break
            }
        }
        return symbol
    }

    pub fn is_type_alias(&self) -> bool {
        return self.evaluation.is_some() && !self.evaluation.as_ref().unwrap().symbol.instance && !self.is_import_variable;
    }
}