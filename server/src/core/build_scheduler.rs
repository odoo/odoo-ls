use std::{cell::RefCell, path::Path, rc::Rc, sync::atomic::Ordering};

use lsp_types::MessageType;
use tracing::{info, trace};

use crate::{S, constants::{BuildStatus, BuildSteps, DEBUG_STEPS, DEBUG_THREADS, MAX_WATCHED_FILES_UPDATES_BEFORE_RESTART}, core::{csv_validation::CsvValidator, entry_point::EntryPoint, import_resolver::ImportCache, js_validator::JsValidator, odoo::InitState, python_arch_builder::PythonArchBuilder, python_arch_eval::PythonArchEval, python_validator::PythonValidator, symbols::{SymbolTable, symbol_keys::SymbolKey}, xml_validation::XmlValidator}, fifo_ptr_weak_hash_set::FifoWeakHashSet, progress_reporter::ProgressReporterRemaining, threads::SessionInfo, tree::Tree, utils::HashSet};

#[derive(Debug)]
pub struct BuildScheduler {
    rebuild_arch: FifoWeakHashSet<SymbolKey>,
    rebuild_arch_eval: FifoWeakHashSet<SymbolKey>,
    rebuild_validation: FifoWeakHashSet<SymbolKey>,
}

macro_rules! bs {
    ($session:expr) => {
        $session.sync_odoo.build_scheduler
    };
}

impl Default for BuildScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildScheduler {
    pub fn new() -> Self {
        BuildScheduler {
            rebuild_arch: FifoWeakHashSet::new(),
            rebuild_arch_eval: FifoWeakHashSet::new(),
            rebuild_validation: FifoWeakHashSet::new(),
        }
    }
    
    /// Build one item from the build queues, preferably ARCH, then ARCH_EVAL, then VALIDATION if `validation` is `true`.
    /// Returns true if an item was built, false if all queues are empty.
    pub fn build_one(session: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, validation: bool) -> bool {
        while let Some(symbol) = bs!(session).rebuild_arch.pop_front_valid(&session.sync_odoo.symbol_table) {
            if let Some(mut builder) = PythonArchBuilder::new(session.st(), entry.clone(), symbol) {
                builder.load_arch(session);
                return true;
            }
        }
        while let Some(symbol) = bs!(session).rebuild_arch_eval.pop_front_valid(&session.sync_odoo.symbol_table) {
            if let Some(mut builder) = PythonArchEval::new(session.st(), entry.clone(), symbol) {
                builder.eval_arch(session);
                return true;
            }
        }
        if validation && let Some(symbol) = bs!(session).rebuild_validation.pop_front_valid(&session.sync_odoo.symbol_table) {
            Self::validate(session, symbol, entry.clone());
            return true;
        }
        false
    }

    /**
     * Queue the rebuild of a symbol for the given build step
     * Warning! 
     * 
     */
    pub fn queue(session: &mut SessionInfo, symbol: impl Into<SymbolKey>, step: BuildSteps) {
        let symbol = symbol.into();
        match step {
            BuildSteps::ARCH => Self::add_to_rebuild_arch(session, symbol),
            BuildSteps::ARCH_EVAL => Self::add_to_rebuild_arch_eval(session, symbol),
            BuildSteps::VALIDATION => Self::add_to_validations(session, symbol),
            _ => {}
        }
    }

    fn add_to_rebuild_arch(session: &mut SessionInfo, symbol: impl Into<SymbolKey>) {
        let symbol = symbol.into();
        if DEBUG_THREADS {
            trace!("ADDED TO ARCH - {}", session.st().debug_path(symbol));
        }
        if session.st().build_status(symbol, BuildSteps::ARCH) != BuildStatus::IN_PROGRESS {
            session.st_mut().set_build_status(symbol, BuildSteps::ARCH, BuildStatus::PENDING);
            session.st_mut().set_build_status(symbol, BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
            session.st_mut().set_build_status(symbol, BuildSteps::VALIDATION, BuildStatus::PENDING);
            bs!(session).rebuild_arch.insert(symbol);
        }
    }

    fn add_to_rebuild_arch_eval(session: &mut SessionInfo, symbol: impl Into<SymbolKey>) {
        let symbol = symbol.into();
        if DEBUG_THREADS {
            trace!("ADDED TO EVAL - {}", session.st().debug_path(symbol));
        }
        if session.st().build_status(symbol, BuildSteps::ARCH_EVAL) != BuildStatus::IN_PROGRESS {
            session.st_mut().set_build_status(symbol, BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
            session.st_mut().set_build_status(symbol, BuildSteps::VALIDATION, BuildStatus::PENDING);
            bs!(session).rebuild_arch_eval.insert(symbol);
        }
    }

    fn add_to_validations(session: &mut SessionInfo, symbol: impl Into<SymbolKey>) {
        let symbol = symbol.into();
        if DEBUG_THREADS {
            trace!("ADDED TO VALIDATION - {}", session.st().debug_path(symbol));
        }
        if session.st().build_status(symbol, BuildSteps::VALIDATION) != BuildStatus::IN_PROGRESS {
            session.st_mut().set_build_status(symbol, BuildSteps::VALIDATION, BuildStatus::PENDING);
            bs!(session).rebuild_validation.insert(symbol);
        }
    }

    pub fn get_rebuild_queue_size(session: &mut SessionInfo) -> usize {
        bs!(session).rebuild_arch.len() +
        bs!(session).rebuild_arch_eval.len() +
        bs!(session).rebuild_validation.len()
    }

    pub fn remove_from_rebuild_arch(session: &mut SessionInfo, symbol: SymbolKey) {
        bs!(session).rebuild_arch.remove(&symbol);
    }

    pub fn remove_from_rebuild_arch_eval(session: &mut SessionInfo, symbol: SymbolKey) {
        bs!(session).rebuild_arch_eval.remove(&symbol);
    }

    pub fn remove_from_rebuild_validation(session: &mut SessionInfo, symbol: SymbolKey) {
        bs!(session).rebuild_validation.remove(&symbol);
    }

    pub fn remove_from_rebuild(session: &mut SessionInfo, symbol: SymbolKey, step: BuildSteps) {
        if DEBUG_STEPS {
            trace!("REMOVED FROM {step:?} - {}", session.sync_odoo.symbol_table.debug_path(symbol));
        }
        if step == BuildSteps::ARCH {
            bs!(session).rebuild_arch.remove(&symbol);
        } else if step == BuildSteps::ARCH_EVAL {
            bs!(session).rebuild_arch_eval.remove(&symbol);
        } else if step == BuildSteps::VALIDATION {
            bs!(session).rebuild_validation.remove(&symbol);
        }
    }

    pub fn is_in_rebuild(session: &mut SessionInfo, symbol: SymbolKey, step: BuildSteps) -> bool {
        if step == BuildSteps::ARCH {
            return bs!(session).rebuild_arch.contains(&symbol);
        }
        if step == BuildSteps::ARCH_EVAL {
            return bs!(session).rebuild_arch_eval.contains(&symbol);
        }
        if step == BuildSteps::VALIDATION {
            return bs!(session).rebuild_validation.contains(&symbol);
        }
        false
    }

    pub fn validation_queue_len(session: &mut SessionInfo) -> usize {
        bs!(session).rebuild_validation.len()
    }

    fn pop_item(session: &mut SessionInfo, step: BuildSteps) -> Option<SymbolKey> {
        //Part 1: Find the symbol with a unmutable set
        let set =  match step {
            BuildSteps::ARCH_EVAL => &bs!(session).rebuild_arch_eval,
            BuildSteps::VALIDATION => &bs!(session).rebuild_validation,
            _ => &bs!(session).rebuild_arch
        };
        let mut selected_sym: Option<SymbolKey> = None;
        let mut selected_count: u32 = 999999999;
        let mut current_count: u32;
        for sym in set.iter_valid(&session.sync_odoo.symbol_table) {
            current_count = 0;
            let file = session.sync_odoo.symbol_table.get_file(sym).unwrap();
            let all_dep = session.sync_odoo.symbol_table.get_all_dependencies(file, step);
            for (index, dep_set) in all_dep.iter().enumerate() {
                let index_set =  match index {
                    x if x == BuildSteps::ARCH as usize => &bs!(session).rebuild_arch,
                    x if x == BuildSteps::ARCH_EVAL as usize => &bs!(session).rebuild_arch_eval,
                    x if x == BuildSteps::VALIDATION as usize => &bs!(session).rebuild_validation,
                    _ => continue,
                };
                current_count += dep_set.iter_valid(&session.sync_odoo.symbol_table)
                    .filter(|&dep| index_set.contains(&dep.into()))
                    .count() as u32;
            }
            if current_count < selected_count {
                selected_sym = Some(sym);
                selected_count = current_count;
                if current_count == 0 {
                    break;
                }
            }
        }
        let set =  match step {
            BuildSteps::ARCH_EVAL => &mut bs!(session).rebuild_arch_eval,
            BuildSteps::VALIDATION => &mut bs!(session).rebuild_validation,
            _ => &mut bs!(session).rebuild_arch,
        };
        if selected_sym.is_none() {
            set.clear(); //remove any potential dead weak ref
            return None;
        }
        let selected_sym_unwrapped = selected_sym.unwrap();
        if !set.remove(&selected_sym_unwrapped) {
            panic!("Unable to remove selected symbol from rebuild set")
        }
        Some(selected_sym_unwrapped)
    }

    fn add_from_self_reload(session: &mut SessionInfo) {
        for (weak_sym, path) in session.sync_odoo.must_reload_paths.clone().iter() {
            let Some(parent) = weak_sym.upgrade(session.st()) else {
                continue;
            };
            let in_addons = session.sync_odoo.get_main_entry_tree(parent) == (&["odoo", "addons"], &[]);
            let new_symbol = SymbolTable::create_from_path(session, Path::new(path), parent, in_addons);
            let Some(new_symbol) = new_symbol else {
                continue;
            };
            session.sync_odoo.must_reload_paths.retain(|(_, p)| p != path);
            session.st_mut().set_is_external(new_symbol, false);
            match new_symbol {
                SymbolKey::PythonPackage(p) => {
                    session.st_mut()[p].self_import = true;
                }
                SymbolKey::File(f) => {
                    session.st_mut()[f].self_import = true;
                },
                SymbolKey::JsFile(f) => {
                    session.st_mut()[f].self_import = true;
                    BuildScheduler::queue(session, new_symbol, BuildSteps::VALIDATION);
                    continue;
                }
                SymbolKey::Module(_) => {}
                SymbolKey::Namespace(_) => continue, // A module became a namespace, due to __init__ deletion/renaming
                _ => {panic!("Unexpected symbol type: {:?}", new_symbol);}
            }
            if let SymbolKey::Module(m) = new_symbol {
                let name = session.st()[m].name.clone();
                session.sync_odoo.modules.insert(name, m.into());
            }
            BuildScheduler::queue(session, new_symbol, BuildSteps::ARCH);
        }
        session.sync_odoo.must_reload_paths.retain(|x| x.0.upgrade(&session.sync_odoo.symbol_table).is_some());
    }

    pub fn process_rebuilds(session: &mut SessionInfo, no_validation: bool) -> bool {
        session.sync_odoo.interrupt_rebuild.store(false, Ordering::SeqCst);
        if session.sync_odoo.watched_file_updates > MAX_WATCHED_FILES_UPDATES_BEFORE_RESTART {
            return false;
        }
        BuildScheduler::add_from_self_reload(session);
        session.sync_odoo.import_cache = Some(ImportCache::default());
        let mut already_arch_rebuilt: HashSet<Tree> = HashSet::default();
        let mut already_arch_eval_rebuilt: HashSet<Tree> = HashSet::default();

        //workdone progress
        let mut reporter = (!bs!(session).rebuild_arch.is_empty() || !bs!(session).rebuild_arch_eval.is_empty() || !bs!(session).rebuild_validation.is_empty())
            .then(|| ProgressReporterRemaining::start(session, "Odoo: Indexing"));
        trace!("Starting rebuild: {:?} - {:?} - {:?}", bs!(session).rebuild_arch.len(), bs!(session).rebuild_arch_eval.len(), bs!(session).rebuild_validation.len());
        while !session.sync_odoo.need_rebuild && (!bs!(session).rebuild_arch.is_empty() || !bs!(session).rebuild_arch_eval.is_empty() || !bs!(session).rebuild_validation.is_empty()) {
            if DEBUG_THREADS {
                trace!("remains: {:?} - {:?} - {:?}", bs!(session).rebuild_arch.len(), bs!(session).rebuild_arch_eval.len(), bs!(session).rebuild_validation.len());
            }
            let queue_size = bs!(session).rebuild_arch.len() * 3 + bs!(session).rebuild_arch_eval.len() * 2 + bs!(session).rebuild_validation.len();
            if let Some(reporter) = &mut reporter {
                reporter.report_progress(queue_size);
            }
            if session.sync_odoo.terminate_rebuild.load(Ordering::SeqCst){
                info!("Terminating rebuilds due to server shutdown");
                if let Some(reporter) = &mut reporter {
                    reporter.end();
                }
                return false;
            }
            let sym = BuildScheduler::pop_item(session, BuildSteps::ARCH);
            if let Some(sym_key) = sym {
                if DEBUG_STEPS {
                    trace!("PROCESSING FROM ARCH - {}", session.st().debug_path(sym_key));
                }
                let (tree, entry) = session.st().get_tree_and_entry(sym_key);
                if already_arch_rebuilt.contains(&tree) {
                    info!("Already arch rebuilt, skipping");
                    continue;
                }
                already_arch_rebuilt.insert(tree);
                if let Some(mut builder) = PythonArchBuilder::new(session.st(), entry, sym_key) {
                    builder.load_arch(session);
                };
                continue;
            }
            let sym = BuildScheduler::pop_item(session, BuildSteps::ARCH_EVAL);
            if let Some(sym_key) = sym {
                if DEBUG_STEPS {
                    trace!("PROCESSING FROM ARCH_EVAL - {}", session.st().debug_path(sym_key));
                }
                let (tree, entry) = session.st().get_tree_and_entry(sym_key);
                if already_arch_eval_rebuilt.contains(&tree) {
                    info!("Already arch eval rebuilt, skipping");
                    continue;
                }
                already_arch_eval_rebuilt.insert(tree);
                if let Some(mut builder) = PythonArchEval::new(session.st(), entry, sym_key) {
                    builder.eval_arch(session);
                };
                continue;
            }
            let sym = BuildScheduler::pop_item(session, BuildSteps::VALIDATION);
            if let Some(sym_key) = sym {
                if DEBUG_STEPS {
                    trace!("PROCESSING FROM VALIDATION - {}", session.st().debug_path(sym_key));
                }
                let (_, entry) = session.st().get_tree_and_entry(sym_key);
                if session.sync_odoo.state_init == InitState::ODOO_READY {
                    let mut no_validation = no_validation;
                    if session.sync_odoo.interrupt_rebuild.load(Ordering::SeqCst) {
                        session.sync_odoo.interrupt_rebuild.store(false, Ordering::SeqCst);
                        session.log_message(MessageType::INFO, S!("Rebuild interrupted"));
                        no_validation = true;
                    }
                    if no_validation {
                        session.request_delayed_rebuild();
                        BuildScheduler::queue(session, sym_key, BuildSteps::VALIDATION);
                        if let Some(reporter) = &mut reporter {
                            reporter.end();
                        }
                        return true;
                    }
                }
                Self::validate(session, sym_key, entry);
                continue;
            }
        }
        if session.sync_odoo.need_rebuild {
            session.log_message(MessageType::INFO, S!("Rebuild required. Resetting database on breaktime..."));
            info!("Odoo version change detected. OdooLS is restarting");
            session.send_notification("$Odoo/restartNeeded", ());
        }
        session.sync_odoo.import_cache = None;
        session.sync_odoo.watched_file_updates = 0;
        if let Some(reporter) = &mut reporter {
            reporter.end();
        }
        trace!("Leaving rebuild with remaining tasks: {:?} - {:?} - {:?}", bs!(session).rebuild_arch.len(), bs!(session).rebuild_arch_eval.len(), bs!(session).rebuild_validation.len());
        true
    }

    fn validate(session: &mut SessionInfo, sym_key: SymbolKey, entry: Rc<RefCell<EntryPoint>>) {
        match sym_key {
            SymbolKey::XmlFile(xml) => {
                let mut validator = XmlValidator::new(&entry, xml, session.st());
                validator.validate(session);
            },
            SymbolKey::CsvFile(csv) => {
                let mut validator = CsvValidator::new();
                validator.validate(session, csv);
            },
            SymbolKey::JsFile(js) => {
                let mut validator = JsValidator::new(js);
                validator.validate(session);
            },
            _ => {
                let mut validator = PythonValidator::new(session.st(), entry, sym_key);
                validator.validate(session);
            }
        }
    }
}