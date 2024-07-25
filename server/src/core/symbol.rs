use ruff_text_size::{TextSize, TextRange};
use serde_json::{Value, json};
use tracing::info;

use crate::constants::*;
use crate::core::evaluation::{Context, Evaluation};
use crate::core::odoo::SyncOdoo;
use crate::core::python_arch_eval::PythonArchEval;
use crate::threads::SessionInfo;
use crate::utils::{MaxTextSize, PathSanitizer as _};
use crate::S;
use core::panic;
use std::collections::{HashMap, VecDeque};
use weak_table::PtrWeakHashSet;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::vec;
use lsp_types::Diagnostic;

use super::evaluation::SymbolRef;
use super::localized_symbol::LocalizedSymbol;
use super::symbol_location::{self, SymbolLocation};
use super::symbols::function_symbol::FunctionSymbol;
use super::symbols::module_symbol::ModuleSymbol;
use super::symbols::root_symbol::RootSymbol;


#[derive(Debug)]
pub struct Symbol {
    pub name: String,
    pub sym_type: SymType,
    pub paths: Vec<String>,
    //eval: Option<Evaluation>,
    pub i_ext: String,
    pub is_external: bool,
    pub symbols: Option<SymbolLocation>,
    pub module_symbols: HashMap<String, Rc<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>, //parent can be None only on detached symbol, like proxys (super() for example)
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub localized_sym: Vec<Vec<Rc<RefCell<LocalizedSymbol>>>>, //by sections, then by offset order
    dependencies: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4],
    dependents: [Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3],
    pub not_found_paths: Vec<(BuildSteps, Vec<String>)>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub in_workspace: bool,

    pub _root: Option<RootSymbol>,
    pub _module: Option<ModuleSymbol>,
}

impl Symbol {
    fn new(name: String, sym_type: SymType) -> Self {
        let mut sym = Symbol{
            name: name.clone(),
            sym_type: sym_type,
            paths: vec![],
            i_ext: String::new(),
            is_external: false,
            symbols: None,
            module_symbols: HashMap::new(),
            parent: None,
            weak_self: None,
            localized_sym: vec![],
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
            not_found_paths: Vec::new(),
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            odoo_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            in_workspace: false,

            _root: None,
            _module: None,
        };
        match sym_type {
            SymType::FILE | SymType::PACKAGE | SymType::CONTENT => {
                sym.symbols = Some(SymbolLocation::new());
            },
            _ => {}
        }
        sym
    }

    pub fn new_root(name: String, sym_type: SymType) -> Self {
        let mut new_sym = Symbol::new(name, sym_type);
        new_sym._root = Some(RootSymbol{sys_path: vec![]});
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
                    if let Some(s) = iter_sym.unwrap().borrow_mut().symbols.as_ref().unwrap().get(fk) {
                        iter_sym = Some(s.clone());
                    } else {
                        return None;
                    }
                }
            }
        } else {
            if symbol_tree_content.len() == 0 || self.symbols.is_none() {
                return None;
            }
            iter_sym = self.symbols.as_ref().unwrap().get(&symbol_tree_content[0]);
            if iter_sym.is_none() {
                return None;
            }
            if symbol_tree_content.len() >1 {
                for fk in symbol_tree_content[1..symbol_tree_content.len()].iter() {
                    if let Some(s) = iter_sym.unwrap().borrow_mut().symbols.as_ref().unwrap().get(fk) {
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
        if self.sym_type != SymType::FILE && self.sym_type != SymType::PACKAGE {
            panic!("Dependencies should be only on files");
        }
        if symbol.sym_type != SymType::FILE && symbol.sym_type != SymType::PACKAGE {
            panic!("Dependencies should be only on files");
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

    //return true if to_test is in parents of symbol or equal to it.
    pub fn is_symbol_in_parents(symbol: &Rc<RefCell<Symbol>>, to_test: &Rc<RefCell<Symbol>>) -> bool {
        if Rc::ptr_eq(symbol, to_test) {
            return true;
        }
        if symbol.borrow().parent.is_none() {
            return false;
        }
        let parent = symbol.borrow().parent.as_ref().unwrap().upgrade().unwrap();
        return Symbol::is_symbol_in_parents(&parent, to_test);
    }

    pub fn invalidate(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>, step: &BuildSteps) {
        //signals that a change occured to this symbol. "step" indicates which level of change occured.
        //It will trigger rebuild on all dependencies
        let mut vec_to_invalidate: VecDeque<Rc<RefCell<Symbol>>> = VecDeque::from([symbol.clone()]);
        while let Some(ref_to_inv) = vec_to_invalidate.pop_front() {
            let sym_to_inv = ref_to_inv.borrow();
            if [SymType::FILE, SymType::PACKAGE].contains(&sym_to_inv.sym_type) {
                if *step == BuildSteps::ARCH {
                    for (index, hashset) in sym_to_inv.dependents[BuildSteps::ARCH as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
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
                    for (index, hashset) in sym_to_inv.dependents[BuildSteps::ARCH_EVAL as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
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
                    for (index, hashset) in sym_to_inv.dependents[BuildSteps::ODOO as usize].iter().enumerate() {
                        for sym in hashset {
                            if !Symbol::is_symbol_in_parents(&sym, &ref_to_inv) {
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
            for sym in sym_to_inv.module_symbols.values() {
                vec_to_invalidate.push_back(sym.clone());
            }
        }
    }

    pub fn unload(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>) {
        /* Unload the symbol and its children. Mark all dependents symbols as 'to_revalidate' */
        if symbol.borrow().sym_type == SymType::DIRTY {
            panic!("Can't unload dirty symbol");
        }
        if symbol.borrow().sym_type == SymType::CONTENT {
            panic!("Only unload file, package, namespace, but never file content. The all_symbols function is not localized, and would mess everything");
        }
        let mut vec_to_unload: VecDeque<Rc<RefCell<Symbol>>> = VecDeque::from([symbol.clone()]);
        while vec_to_unload.len() > 0 {
            let ref_to_unload = vec_to_unload.front().unwrap().clone();
            let mut mut_symbol = ref_to_unload.borrow_mut();
            // Unload children first
            let mut found_one = false;
            for sym in mut_symbol.all_symbols() {
                found_one = true;
                vec_to_unload.push_front(sym.clone());
            }
            if found_one {
                continue;
            } else {
                vec_to_unload.pop_front();
            }
            if DEBUG_MEMORY && (mut_symbol.sym_type == SymType::FILE || mut_symbol.sym_type == SymType::PACKAGE) {
                info!("Unloading symbol {:?} at {:?}", mut_symbol.name, mut_symbol.paths);
            }
            //unload symbol
            let parent = mut_symbol.parent.as_ref().unwrap().upgrade().unwrap().clone();
            let mut parent = parent.borrow_mut();
            drop(mut_symbol);
            parent.remove_symbol(ref_to_unload.clone());
            drop(parent);
            if vec![SymType::FILE, SymType::PACKAGE].contains(&ref_to_unload.borrow().sym_type) {
                Symbol::invalidate(session, ref_to_unload.clone(), &BuildSteps::ARCH);
            }
            let mut mut_symbol = ref_to_unload.borrow_mut();
            if mut_symbol._module.is_some() {
                session.sync_odoo.modules.remove(mut_symbol._module.as_ref().unwrap().dir_name.as_str());
            }
            mut_symbol.sym_type = SymType::DIRTY;
        }
    }

    pub fn remove_symbol(&mut self, symbol: Rc<RefCell<Symbol>>) {
        if symbol.borrow().is_file_content() {
            self.symbols.as_mut().unwrap().remove(&symbol.borrow().name);
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

    pub fn get_file(&self) -> Option<Weak<RefCell<Symbol>>> {
        if self.sym_type == SymType::FILE || self.sym_type == SymType::PACKAGE {
            return self.weak_self.clone();
        }
        if self.parent.is_some() {
            return self.parent.as_ref().unwrap().upgrade().unwrap().borrow_mut().get_file();
        }
        return None;
    }

    /*given a SymbolRef, give all the SymbolRef that are evaluated as valid evaluation for it.
    example:
    ====
    a = 5
    if X:
        a = Test()
    else:
        a = Object()
    print(a)
    ====
    next_refs on the 'a' in the print will return a SymbolRef to Test and one to Object
    */
    pub fn next_refs(session: &mut SessionInfo, sym_ref: &SymbolRef, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> VecDeque<(SymbolRef, bool)> {
        let mut res = VecDeque::new();
        if !sym_ref.is_expired() {
            let _lsym = sym_ref.get_localized_symbol();
            if let Some(lsym) = _lsym {
                let lsym = lsym.borrow();
                if lsym.loc_sym_type == LocSymType::VARIABLE {
                    for eval in lsym.evaluations.iter() {
                        //TODO context is modified in each for loop, which is wrong !
                        res.push_back(eval.symbol.get_symbol(session, context, diagnostics));
                    }
                }
            }
        }
        res
    }

    pub fn follow_ref(symbol: &SymbolRef, session: &mut SessionInfo, context: &mut Option<Context>, stop_on_type: bool, stop_on_value: bool, diagnostics: &mut Vec<Diagnostic>) -> Vec<(SymbolRef, bool)> {
        //return a list of all possible evaluation: a weak ptr to the final symbol, and a bool indicating if this is an instance or not
        let mut sym = symbol.get_symbol();
        let mut sym_loc = symbol.get_localized_symbol();
        if sym_loc.is_none() {
            return vec![];
        }
        let mut results = Symbol::next_refs(session, &symbol, &mut None, &mut vec![]);
        let can_eval_external = !sym.borrow().is_external;

        let mut index = 0;
        while index < results.len() {
            let (sym_ref, instance) = &results[index];
            sym = sym_ref.get_symbol();
            sym_loc = sym_ref.get_localized_symbol();
            if sym_loc.is_none() {
                continue;
            }
            let sym_loc_uw = sym_loc.unwrap();
            if stop_on_type && !instance && !sym_loc_uw.borrow().is_import_variable {
                continue;
            }
            if stop_on_value && sym_loc_uw.borrow().evaluations.len() == 1 && sym_loc_uw.borrow().evaluations[0].value.is_some() {
                continue;
            }
            if sym_loc_uw.borrow().evaluations.len() == 0 {
                //no evaluation? let's check that the file has been evaluated
                let file_symbol = sym.borrow().get_file();
                match file_symbol {
                    Some(file_symbol) => {
                        if file_symbol.upgrade().expect("invalid weak value").borrow().arch_eval_status == BuildStatus::PENDING &&
                        session.sync_odoo.is_in_rebuild(&file_symbol.upgrade().unwrap(), BuildSteps::ARCH_EVAL) { //TODO check ARCH ?
                            let mut builder = PythonArchEval::new(file_symbol.upgrade().unwrap());
                            builder.eval_arch(session);
                        }
                    },
                    None => {}
                }
            }
            let mut next_sym_refs = Symbol::next_refs(session, sym_ref, &mut None, &mut vec![]);
            if next_sym_refs.len() >= 1 {
                results.pop_front();
                index -= 1;
                for next_results in next_sym_refs {
                    results.push_back(next_results);
                }
            }
            index += 1;
        }
        return Vec::from(results) // :'( a whole copy?
    }

    pub fn create_or_get_symbol(&mut self, session: &mut SessionInfo, name: &str, sym_type: SymType) -> Rc<RefCell<Symbol>> {
        if self.symbols.is_some() {
        if let Some(existing) = self.symbols().get(name) {
            return existing;
            }
        }
        let mut new_sym = Symbol::new(S!(name), sym_type);
        if self.is_external {
            new_sym.is_external = true;
        }
        let rc = Rc::new(RefCell::new(new_sym));
        let mut locked_symbol = rc.borrow_mut();
        locked_symbol.weak_self = Some(Rc::downgrade(&rc));
        locked_symbol.parent = match &self.weak_self {
            Some(s) => Some(s.clone()),
            None => panic!("A parent must be set to create a new symbol")
        };
        if locked_symbol.is_file_content() {
            self.symbols.as_mut().unwrap().add_symbol(name, rc.clone());
        } else {
            self.module_symbols.insert(S!(name), rc.clone());
        }
        if self._root.is_some() {
            self._root.as_ref().unwrap().add_symbol(session, &self, &mut locked_symbol);
        }
        rc.clone()
    }

    pub fn set_module(&mut self, session: &mut SessionInfo, _module: ModuleSymbol ) {
        self._module = Some(_module);
        session.sync_odoo.modules.insert(self._module.as_ref().unwrap().dir_name.clone(), self.weak_self.as_ref().unwrap().clone());
    }

    pub fn get_module(&self) -> &ModuleSymbol {
        self._module.as_ref().expect("Module should be set before call 'get_module'")
    }

    pub fn get_module_mut(&mut self) -> &mut ModuleSymbol {
        self._module.as_mut().expect("Module should be set before call 'get_module'")
    }

    pub fn create_from_path(session: &mut SessionInfo, path: &PathBuf, parent: Rc<RefCell<Symbol>>, require_module: bool) -> Option<Rc<RefCell<Symbol>>> {
        let name: String = path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap().to_string();
        let path_str = path.sanitize();
        if path_str.ends_with(".py") || path_str.ends_with(".pyi") {
            let ref_sym = (*parent).borrow_mut().create_or_get_symbol(session, name.as_str(), SymType::FILE);
            ref_sym.borrow_mut().paths = vec![path_str.clone()];
            return Some(ref_sym);
        } else {
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                let ref_sym = (*parent).borrow_mut().create_or_get_symbol(session, name.as_str(), SymType::PACKAGE);
                ref_sym.borrow_mut().paths = vec![path_str.clone()];
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
                let ref_sym = (*parent).borrow_mut().create_or_get_symbol(session, name.as_str(), SymType::NAMESPACE);
                ref_sym.borrow_mut().paths = vec![path_str.clone()];
                return Some(ref_sym);
            } else {
                return None
            }
        }
    }

    /// get a LocalizedSymbol that has the same given range and name
    pub fn get_positioned_symbol(&self, name: &String, range: &TextRange) -> Option<Rc<RefCell<LocalizedSymbol>>> {
        if let Some(symbol) = self.symbols.as_ref().unwrap().get(name) {
            for section in symbol.borrow().localized_sym.iter() {
                for loc in section.iter() {
                    if loc.borrow().range.start() == range.start() {
                        return Some(loc.clone());
                    }
                }
            }
        }
        None
    }

    fn _debug_print_graph_node(&self, acc: &mut String, level: u32) {
        for _ in 0..level {
            acc.push_str(" ");
        }
        acc.push_str(format!("{:?} {:?}\n", self.sym_type, self.name).as_str());
        for section in self.localized_sym.iter() {
            for local in section.iter() {
                for _ in 0..level {
                    acc.push_str(" ");
                }
                acc.push_str(format!("at {}", local.borrow().range.start().to_u32()).as_str());
            }
        }
        if let Some(symbol_location) = &self.symbols {
            if symbol_location.symbols().len() > 0 {
                for _ in 0..level {
                    acc.push_str(" ");
                }
                acc.push_str("SYMBOLS:\n");
                for sym in symbol_location.symbols().values() {
                    sym.borrow()._debug_print_graph_node(acc, level + 1);
                }
            }
        }
        if self.module_symbols.len() > 0 {
            for _ in 0..level {
                acc.push_str(" ");
            }
            acc.push_str("MODULES:\n");
            for (_, module) in self.module_symbols.iter() {
                module.borrow_mut()._debug_print_graph_node(acc, level + 1);
            }
        }
    }

    pub fn debug_to_json(&self) -> Value {
        let mut modules = vec![];
        let mut symbols = vec![];
        let mut offsets = vec![];
        for section in self.localized_sym.iter() {
            for local in section.iter() {
                offsets.push(local.borrow().range.start().to_u32());
            }
        }
        for s in self.module_symbols.values() {
            modules.push(s.borrow_mut().debug_to_json());
        }
        for s in self.symbols.as_ref().unwrap().symbols().values() {
            symbols.push(s.borrow_mut().debug_to_json());
        }
        json!({
            "name": self.name,
            "type": self.sym_type.to_string(),
            "offsets": offsets,
            "module_symbols": modules,
            "symbols": symbols,
        })
    }

    pub fn debug_print_graph(&self) -> String {
        info!("starting log");
        let mut res: String = String::new();
        self._debug_print_graph_node(&mut res, 0);
        res
    }

    pub fn all_symbols<'a>(&'a self) -> impl Iterator<Item= &'a Rc<RefCell<Symbol>>> + 'a {
        //return an iterator on all symbols of self. only symbols in symbols and module_symbols will
        //be returned. If include_inherits is set, symbols from parent will be included.
        let mut iter: Vec<Box<dyn Iterator<Item = &Rc<RefCell<Symbol>>>>> = Vec::new();
        iter.push(Box::new(self.symbols.as_ref().unwrap().symbols().values()));
        iter.push(Box::new(self.module_symbols.values()));
        iter.into_iter().flatten()
    }

    //create a new localized symbol on the last section for the given range
    pub fn new_localized_symbol(&mut self, loc_sym_type: LocSymType, range: TextRange) -> Rc<RefCell<LocalizedSymbol>> {
        let sym = Rc::new(RefCell::new(LocalizedSymbol::new(self.weak_self.as_ref().unwrap().clone(), loc_sym_type, range)));
        self.localized_sym.last_mut().unwrap().push(sym.clone());
        sym
    }

    //create a new localized symbol with a range that can be in custom section
    pub fn new_localized_symbol_with_range(&mut self, loc_sym_type: LocSymType, range: TextRange) -> Rc<RefCell<LocalizedSymbol>> {
        let sym = Rc::new(RefCell::new(LocalizedSymbol::new(self.weak_self.as_ref().unwrap().clone(), loc_sym_type, range)));
        let section_id = self.parent.as_ref().unwrap().upgrade().unwrap().borrow().symbols().get_section_for(range.start().to_u32()).index;
        let index_to_insert = self.localized_sym[section_id as usize].binary_search_by(|x| x.borrow().range.start().to_u32().cmp(&range.start().to_u32())).unwrap_or_else(|x| x);
        self.localized_sym[section_id as usize].insert(index_to_insert, sym.clone());
        sym
    }

    //infer a name, given a position
    pub fn infer_name(odoo: &mut SyncOdoo, on_symbol: &Rc<RefCell<Symbol>>, name: &String, position: Option<TextSize>) -> Vec<Rc<RefCell<LocalizedSymbol>>> {
        let mut results: Vec<Rc<RefCell<LocalizedSymbol>>> = vec![];
        //TODO implement 'super' behaviour in hooks
        let on_symbol = on_symbol.borrow();
        let symbol_location = on_symbol.symbols.as_ref().unwrap();
        if let Some(symbol) = symbol_location.get(name) {
            results = symbol.borrow().get_loc_sym(position.unwrap_or(TextSize::MAX).to_u32());
        }
        if results.len() == 0 && !vec![SymType::FILE, SymType::PACKAGE, SymType::ROOT].contains(&on_symbol.sym_type) {
            let parent = on_symbol.parent.as_ref().unwrap().upgrade().unwrap();
            return Symbol::infer_name(odoo, &parent, name, position);
        }
        if results.len() == 0 && (on_symbol.name != "builtins" || on_symbol.sym_type != SymType::FILE) {
            let builtins = odoo.get_symbol(&(vec![S!("builtins")], vec![])).as_ref().unwrap().clone();
            return Symbol::infer_name(odoo, &builtins, name, None);
        }
        results
    }

    pub fn get_sorted_symbols(&self) -> impl Iterator<Item = Rc<RefCell<LocalizedSymbol>>> {
        let mut symbols: Vec<Rc<RefCell<LocalizedSymbol>>> = Vec::new();
        let syms = self.symbols.as_ref().unwrap().symbols().values();
        for sym in syms {
            for section in sym.borrow().localized_sym.iter() {
                for loc in section.iter() {
                    symbols.push(loc.clone());
                }
            }
        }
        symbols.sort_by_key(|s| s.borrow().range.start().to_u32());
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

    /* return the LocalizedSymbol (class or function) the closest to the given offset */
    pub fn get_scope_symbol(sym: SymbolRef, offset: u32) -> SymbolRef {
        //TODO search in localSymbols too
        let mut result = sym.clone();
        let sym = sym.get_symbol();
        for s in sym.borrow().symbols.as_ref().unwrap().symbols().values() {
            for section in sym.borrow().localized_sym.iter() {
                for loc in section.iter() {
                    if loc.borrow().range.start().to_u32() < offset &&
                    loc.borrow().range.end().to_u32() >= offset &&
                    vec![LocSymType::CLASS, LocSymType::FUNCTION].contains(&loc.borrow().loc_sym_type) {
                        result = Symbol::get_scope_symbol(loc.borrow().to_symbol_ref(), offset);
                        break
                    }
                }
            }
        }
        return result
    }

    //panic if no localized symbol is available
    pub fn last_loc_sym(&self) -> Rc<RefCell<LocalizedSymbol>> {
        self.localized_sym.last().unwrap().last().unwrap().clone()
    }

    ///Return a SymbolRef for this symbol. If LocalizedSymbol is present, return a SymbolRef with position = u32::MAX
    pub fn to_sym_ref(&self) -> SymbolRef {
        SymbolRef::new(self.weak_self.as_ref().unwrap().upgrade().unwrap(), u32::MAX)
    }
}