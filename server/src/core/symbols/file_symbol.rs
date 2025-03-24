use byteyarn::{yarn, Yarn};
use serde_json::json;
use weak_table::PtrWeakHashSet;

use crate::{constants::{BuildStatus, BuildSteps, SymType}, core::model::Model, tool_api::to_json::{dependencies_to_json, dependents_to_json}};
use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}};

use super::{symbol::Symbol, symbol_mgr::{SectionRange, SymbolMgr}};

#[derive(Debug)]
pub struct FileSymbol {
    pub name: Yarn,
    pub path: String,
    pub is_external: bool,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub not_found_paths: Vec<(BuildSteps, Vec<Yarn>)>,
    in_workspace: bool,
    pub self_import: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>,
    pub dependents: Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>,
    pub processed_text_hash: u64,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<Yarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<Yarn, Vec<Rc<RefCell<Symbol>>>>,
}

impl FileSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        let mut res = Self {
            name: yarn!("{}", name),
            path,
            is_external,
            weak_self: None,
            parent: None,
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            odoo_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            not_found_paths: vec![],
            in_workspace: false,
            self_import: false,
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: vec![],
            dependents: vec![],
            processed_text_hash: 0,
        };
        res._init_symbol_mgr();
        res
    }

    pub fn add_symbol(&mut self, content: &Rc<RefCell<Symbol>>, section: u32) {
        let sections = self.symbols.entry(content.borrow().name().clone()).or_insert_with(|| HashMap::new());
        let section_vec = sections.entry(section).or_insert_with(|| vec![]);
        section_vec.push(content.clone());
    }

    pub fn get_dependencies(&self, step: usize, level: usize) -> Option<&PtrWeakHashSet<Weak<RefCell<Symbol>>>>
    {
        self.dependencies.get(step)?.get(level)?.as_ref()
    }

    pub fn get_all_dependencies(&self, step: usize) -> Option<&Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>
    {
        self.dependencies.get(step)
    }

    pub fn get_dependents(&self, level: usize, step: usize) -> Option<&PtrWeakHashSet<Weak<RefCell<Symbol>>>>
    {
        self.dependents.get(level)?.get(step)?.as_ref()
    }

    pub fn get_all_dependents(&self, level: usize) -> Option<&Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>
    {
        self.dependents.get(level)
    }

    pub fn dependencies(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &self.dependencies
    }

    pub fn dependencies_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &mut self.dependencies
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

    pub fn dependents(&self) -> &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &self.dependents
    }

    pub fn dependents_mut(&mut self) -> &mut Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>> {
        &mut self.dependents
    }

    pub fn is_in_workspace(&self) -> bool {
        self.in_workspace
    }

    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "type": SymType::FILE.to_string(),
            "path": self.path,
            "is_external": self.is_external,
            "in_workspace": self.in_workspace,
            "arch_status": self.arch_status.to_string(),
            "arch_eval_status": self.arch_eval_status.to_string(),
            "odoo_status": self.odoo_status.to_string(),
            "validation_status": self.validation_status.to_string(),
            "not_found_paths": self.not_found_paths.iter().map(|(step, paths)| {
                json!({
                    "step": step.to_string(),
                    "paths": paths.into_iter().map(|x| x.to_string()).collect::<Vec<String>>(),
                })
            }).collect::<Vec<serde_json::Value>>(),
            "self_import": self.self_import,
            "model_dependencies": self.model_dependencies.iter().map(|x| json!(x.borrow().get_name())).collect::<Vec<serde_json::Value>>(),
            "dependencies": dependencies_to_json(&self.dependencies),
            "dependents": dependents_to_json(&self.dependents),
            "processed_text_hash": self.processed_text_hash,

            "sections": self.sections.iter().map(|x| {
                json!({
                    "start": x.start,
                    "index": x.index,
                })
            }).collect::<Vec<serde_json::Value>>(),
            "symbols": self.symbols.iter().map(|(name, sections)| {
                json!({
                    "name": name.to_string(),
                    "sections": sections.iter().map(|(section, symbols)| {
                        json!({
                            "section": section,
                            "symbols": symbols.iter().map(|sym| json!(sym.borrow().name().to_string())).collect::<Vec<serde_json::Value>>(),
                        })
                    }).collect::<Vec<serde_json::Value>>(),
                })
            }).collect::<Vec<serde_json::Value>>(),
            "ext_symbols": self.ext_symbols.iter().map(|(name, symbols)| {
                json!({
                    "name": name.to_string(),
                    "symbols": symbols.iter().map(|sym| json!(sym.borrow().name().to_string())).collect::<Vec<serde_json::Value>>(),
                })
            }).collect::<Vec<serde_json::Value>>(),
        })
    }

}