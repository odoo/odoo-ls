use std::{
    cell::RefCell,
    collections::{VecDeque, hash_map},
    path::PathBuf,
    rc::Rc,
};
use crate::{core::evaluation_context::ContextKey, utils::{HashMap, HashSet}};

use lsp_types::{Diagnostic, DiagnosticTag, Range, SymbolKind};
use ruff_text_size::TextRange;
use tracing::warn;

use crate::{
    constants::{BuildStatus, BuildSteps, OYarn, PackageType, SymType}, core::{
        diagnostics::{DiagnosticCode, create_diagnostic},
        entry_point::EntryPoint,
        evaluation::{Evaluation, EvaluationSymbolPtr},
        file_mgr::{FileInfo, FileMgr, NoqaInfo},
        model::Model,
        odoo::SyncOdoo,
        symbols::{
            storage::{
                dependency_mgr::{DependenciesTable, DependentsTable},
                SymbolTable,
            },
            symbol_keys::{
                ClassKey, FunctionKey, KeyValidator, ModuleKey, NamespaceKey, RootKey,
                SourceFileKey, SymbolKey, VariableKey, XmlDataKey,
            },
            symbol_mgr::{iter_symbol_keys, ContentSymbols, SectionIndex, SectionRange, SymbolMgr},
            Buildable, Dependencies,
        },
    },
    threads::SessionInfo,
    tree::{OYarnExt, Tree},
    utils::PathSanitizer,
    weak_collections::WeakSet,
};
use crate::core::evaluation_context::{Context, ContextValue};

impl SymbolTable {

    pub fn as_symbol_mgr(&self, target: SymbolKey) -> &dyn SymbolMgr {
        self.try_as_symbol_mgr(target).expect("Not a symbol Mgr")
    }

    fn try_as_symbol_mgr(&self, target: SymbolKey) -> Option<&dyn SymbolMgr> {
        match target {
            SymbolKey::File(f) => Some(&self[f]),
            SymbolKey::Class(c) => Some(&self[c]),
            SymbolKey::Function(f) => Some(&self[f]),
            SymbolKey::Module(m) => Some(&self[m]),
            SymbolKey::PythonPackage(p) => Some(&self[p]),
            _ => None,
        }
    }

    pub fn as_mut_symbol_mgr(&mut self, target: SymbolKey) -> &mut dyn SymbolMgr {
        match target {
            SymbolKey::File(f) => &mut self[f],
            SymbolKey::Class(c) => &mut self[c],
            SymbolKey::Function(f) => &mut self[f],
            SymbolKey::Module(m) => &mut self[m],
            SymbolKey::PythonPackage(p) => &mut self[p],
            _ => {panic!("Not a symbol Mgr");}
        }
    }

    pub fn name(&self, target: impl Into<SymbolKey>) -> &OYarn {
        match target.into() {
            SymbolKey::Root(k) => &self[k].name,
            SymbolKey::DiskDir(k) => &self[k].name,
            SymbolKey::Namespace(k) => &self[k].name,
            SymbolKey::PythonPackage(p) => &self[p].name,
            SymbolKey::Module(m) => &self[m].name,
            SymbolKey::File(k) => &self[k].name,
            SymbolKey::Compiled(k) => &self[k].name,
            SymbolKey::Class(k) => &self[k].name,
            SymbolKey::Function(k) => &self[k].name,
            SymbolKey::Variable(k) => &self[k].name,
            SymbolKey::XmlFile(k) => &self[k].name,
            SymbolKey::XmlRecord(_) => panic!("XmlRecord doesn't have a name"), //TODO do not have a panic?
            SymbolKey::XmlField(_) => panic!("XmlField doesn't have a name"),
            SymbolKey::XmlMenuItem(_) => panic!("XmlMenuItem doesn't have a name"),
            SymbolKey::XmlTemplate(_) => panic!("XmlTemplate doesn't have a name"),
            SymbolKey::XmlAsset(_) => panic!("XmlAsset doesn't have a name"),
            SymbolKey::XmlDelete(_) => panic!("XmlDelete doesn't have a name"),
            SymbolKey::CsvFile(k) => &self[k].name,
        }
    }

    pub fn is_external(&self, target: SymbolKey) -> bool {
        match target {
            SymbolKey::Root(_) => true,
            SymbolKey::DiskDir(d) => self[d].is_external,
            SymbolKey::Namespace(n) => self[n].is_external,
            SymbolKey::PythonPackage(p) => self[p].is_external,
            SymbolKey::Module(m) => self[m].is_external,
            SymbolKey::File(f) => self[f].is_external,
            SymbolKey::Compiled(c) => self[c].is_external,
            SymbolKey::Class(c) => self[c].is_external,
            SymbolKey::Function(f) => self[f].is_external,
            SymbolKey::Variable(v) => self[v].is_external,
            SymbolKey::XmlFile(x) => self[x].is_external,
            SymbolKey::XmlRecord(x) => self[x].is_external,
            SymbolKey::XmlField(x) => self[x].is_external,
            SymbolKey::XmlMenuItem(x) => self[x].is_external,
            SymbolKey::XmlTemplate(x) => self[x].is_external,
            SymbolKey::XmlAsset(x) => self[x].is_external,
            SymbolKey::XmlDelete(x) => self[x].is_external,
            SymbolKey::CsvFile(c) => self[c].is_external,
        }
    }

    pub fn set_is_external(&mut self, target: SymbolKey, external: bool) {
        match target {
            SymbolKey::Root(_) => {},
            SymbolKey::DiskDir(d) => self[d].is_external = external,
            SymbolKey::Namespace(n) => self[n].is_external = external,
            SymbolKey::Module(m) => self[m].is_external = external,
            SymbolKey::PythonPackage(p) => self[p].is_external = external,
            SymbolKey::File(f) => self[f].is_external = external,
            SymbolKey::Compiled(c) => self[c].is_external = external,
            SymbolKey::Class(c) => self[c].is_external = external,
            SymbolKey::Function(f) => self[f].is_external = external,
            SymbolKey::Variable(v) => self[v].is_external = external,
            SymbolKey::XmlFile(x) => self[x].is_external = external,
            SymbolKey::XmlRecord(x) => self[x].is_external = external,
            SymbolKey::XmlField(x) => self[x].is_external = external,
            SymbolKey::XmlMenuItem(x) => self[x].is_external = external,
            SymbolKey::XmlTemplate(x) => self[x].is_external = external,
            SymbolKey::XmlAsset(x) => self[x].is_external = external,
            SymbolKey::XmlDelete(x) => self[x].is_external = external,
            SymbolKey::CsvFile(c) => self[c].is_external = external,
        }
    }

    pub fn has_range(&self, target: SymbolKey) -> bool {
        match target {
            SymbolKey::Root(_) => false,
            SymbolKey::DiskDir(_) => false,
            SymbolKey::Namespace(_) => false,
            SymbolKey::PythonPackage(_) => false,
            SymbolKey::Module(_) => false,
            SymbolKey::File(_) => false,
            SymbolKey::Compiled(_) => false,
            SymbolKey::Class(_) => true,
            SymbolKey::Function(_) => true,
            SymbolKey::Variable(_) => true,
            SymbolKey::XmlFile(_) => false,
            SymbolKey::XmlRecord(_) => true,
            SymbolKey::XmlField(_) => true,
            SymbolKey::XmlMenuItem(_) => true,
            SymbolKey::XmlTemplate(_) => true,
            SymbolKey::XmlAsset(_) => true,
            SymbolKey::XmlDelete(_) => true,
            SymbolKey::CsvFile(_) => false,
        }
    }

    pub fn range(&self, target: SymbolKey) -> &TextRange {
        match target {
            SymbolKey::Root(_) => panic!(),
            SymbolKey::DiskDir(_) => panic!(),
            SymbolKey::Namespace(_) => panic!(),
            SymbolKey::PythonPackage(_) => panic!(),
            SymbolKey::Module(_) => panic!(),
            SymbolKey::File(_) => panic!(),
            SymbolKey::Compiled(_) => panic!(),
            SymbolKey::Class(c) => &self[c].range,
            SymbolKey::Function(f) => &self[f].range,
            SymbolKey::Variable(v) => &self[v].range,
            SymbolKey::XmlFile(_) => panic!(),
            SymbolKey::XmlRecord(x) => &self[x].range,
            SymbolKey::XmlField(x) => &self[x].range,
            SymbolKey::XmlMenuItem(x) => &self[x].range,
            SymbolKey::XmlTemplate(x) => &self[x].range,
            SymbolKey::XmlAsset(x) => &self[x].range,
            SymbolKey::XmlDelete(x) => &self[x].range,
            SymbolKey::CsvFile(_) => panic!(),
        }
    }

    pub fn parent(&self, target: impl Into<SymbolKey>) -> Option<SymbolKey> {
        match target.into() {
            SymbolKey::Root(_) => None,
            SymbolKey::DiskDir(k) => Some(self[k].parent()),
            SymbolKey::Namespace(k) => Some(self[k].parent()),
            SymbolKey::PythonPackage(k) => Some(self[k].parent()),
            SymbolKey::Module(k) => Some(self[k].parent().into()),
            SymbolKey::File(k) => Some(self[k].parent()),
            SymbolKey::Compiled(k) => Some(self[k].parent()),
            SymbolKey::Class(k) => Some(self[k].parent()),
            SymbolKey::Function(k) => Some(self[k].parent()),
            SymbolKey::Variable(k) => Some(self[k].parent()),
            SymbolKey::XmlFile(x) => Some(self[x].parent().into()),
            SymbolKey::XmlRecord(x) => Some(self[x].parent()),
            SymbolKey::XmlField(x) => Some(self[x].parent()),
            SymbolKey::XmlMenuItem(x) => Some(self[x].parent()),
            SymbolKey::XmlTemplate(x) => Some(self[x].parent()),
            SymbolKey::XmlAsset(x) => Some(self[x].parent()),
            SymbolKey::XmlDelete(x) => Some(self[x].parent()),
            SymbolKey::CsvFile(c) => Some(self[c].parent().into()),
        }
    }

    pub fn paths(&self, target: SymbolKey) -> Vec<String> {
        match target {
            SymbolKey::Root(_) => vec![],
            SymbolKey::Namespace(n) => self[n].paths(),
            SymbolKey::DiskDir(d) => vec![self[d].path.clone()],
            SymbolKey::PythonPackage(p) => vec![self[p].path.clone()],
            SymbolKey::Module(m) => vec![self[m].path.clone()],
            SymbolKey::File(f) => vec![self[f].path.clone()],
            SymbolKey::Compiled(c) => vec![self[c].path.clone()],
            SymbolKey::Class(_) => vec![],
            SymbolKey::Function(_) => vec![],
            SymbolKey::Variable(_) => vec![],
            SymbolKey::XmlFile(x) => vec![self[x].path.clone()],
            SymbolKey::XmlRecord(_) => vec![],
            SymbolKey::XmlField(_) => vec![],
            SymbolKey::XmlMenuItem(_) => vec![],
            SymbolKey::XmlTemplate(_) => vec![],
            SymbolKey::XmlAsset(_) => vec![],
            SymbolKey::XmlDelete(_) => vec![],
            SymbolKey::CsvFile(c) => vec![self[c].path.clone()],
        }
    }

    pub fn path(&self, target: SourceFileKey) -> &str {
        match target {
            SourceFileKey::PythonPackage(p) => &self[p].path,
            SourceFileKey::Module(m) => &self[m].path,
            SourceFileKey::File(f) => &self[f].path,
            SourceFileKey::XmlFile(x) => &self[x].path,
            SourceFileKey::CsvFile(c) => &self[c].path,
        }
    }

    /// like `path`, but with `__init__.py` for packages and modules.
    pub fn file_path(&self, target: SourceFileKey) -> &str {
        match target {
            SourceFileKey::PythonPackage(p) => &self[p].init_path,
            SourceFileKey::Module(m) => &self[m].init_path,
            SourceFileKey::File(f) => &self[f].path,
            SourceFileKey::XmlFile(x) => &self[x].path,
            SourceFileKey::CsvFile(c) => &self[c].path,
        }
    }

    fn dependencies_mut(&mut self, target: SourceFileKey) -> &mut DependenciesTable {
        match target {
            SourceFileKey::File(k) => &mut self[k].dependencies,
            SourceFileKey::XmlFile(k) => &mut self[k].dependencies,
            SourceFileKey::CsvFile(k) => &mut self[k].dependencies,
            SourceFileKey::PythonPackage(p) => &mut self[p].dependencies,
            SourceFileKey::Module(k) => &mut self[k].dependencies,
        }
    }

    pub fn dependents(&self, target: SourceFileKey) -> &DependentsTable {
        match target {
            SourceFileKey::PythonPackage(p) => self[p].dependents(),
            SourceFileKey::Module(m) => self[m].dependents(),
            SourceFileKey::File(f) => self[f].dependents(),
            SourceFileKey::XmlFile(x) => self[x].dependents(),
            SourceFileKey::CsvFile(c) => self[c].dependents(),
        }
    }

    fn dependents_as_mut(&mut self, target: SourceFileKey) -> &mut DependentsTable {
        match target {
            SourceFileKey::File(f) => &mut self[f].dependents,
            SourceFileKey::XmlFile(x) => &mut self[x].dependents,
            SourceFileKey::CsvFile(c) => &mut self[c].dependents,
            SourceFileKey::PythonPackage(p) => &mut self[p].dependents,
            SourceFileKey::Module(k) => &mut self[k].dependents,
        }
    }

    pub fn has_modules(&self, target: SymbolKey) -> bool {
        match target {
            SymbolKey::Root(_) | SymbolKey::Namespace(_) | SymbolKey::PythonPackage(_) | SymbolKey::Module(_) | SymbolKey::DiskDir(_) => true,
            _ => false
        }
    }


    pub fn all_module_symbol(&self, target: SymbolKey) -> Vec<SymbolKey> {
        match target {
            SymbolKey::Root(r) => self[r].module_symbols().values().copied().collect(),
            SymbolKey::Namespace(n) => {
                self[n].directories().iter()
                    .flat_map(|x| x.module_symbols().values())
                    .copied()
                    .collect()
            },
            SymbolKey::DiskDir(d) => self[d].module_symbols().values().copied().collect(),
            SymbolKey::Module(m) => self[m].module_symbols().values().copied().collect(),
            SymbolKey::PythonPackage(p) => self[p].module_symbols().values().copied().collect(),
            SymbolKey::File(_) => panic!("No module symbol on File"),
            SymbolKey::Compiled(_) => panic!("No module symbol on Compiled"),
            SymbolKey::Class(_c) => panic!("No module symbol on Class"),
            SymbolKey::Function(_) => panic!("No module symbol on Function"),
            SymbolKey::Variable(_) => panic!("No module symbol on Variable"),
            SymbolKey::XmlFile(_) => panic!("No module symbol on XmlFileSymbol"),
            SymbolKey::XmlRecord(_) => panic!("No module symbol on XmlRecordSymbol"),
            SymbolKey::XmlField(_) => panic!("No module symbol on XmlFieldSymbol"),
            SymbolKey::XmlMenuItem(_) => panic!("No module symbol on XmlMenuItemSymbol"),
            SymbolKey::XmlTemplate(_) => panic!("No module symbol on XmlTemplateSymbol"),
            SymbolKey::XmlAsset(_) => panic!("No module symbol on XmlAssetSymbol"),
            SymbolKey::XmlDelete(_) => panic!("No module symbol on XmlDeleteSymbol"),
            SymbolKey::CsvFile(_) => panic!("No module symbol on CsvFileSymbol"),
        }
    }


    pub fn in_workspace(&self, target: SymbolKey) -> bool {
        match target {
            SymbolKey::Root(_) => false,
            SymbolKey::Namespace(n) => self[n].in_workspace,
            SymbolKey::DiskDir(d) => self[d].in_workspace,
            SymbolKey::Module(m) => self[m].is_in_workspace(),
            SymbolKey::PythonPackage(p) => self[p].is_in_workspace(),
            SymbolKey::File(f) => self[f].is_in_workspace(),
            SymbolKey::Compiled(_) => panic!(),
            SymbolKey::Class(_) => panic!(),
            SymbolKey::Function(_) => panic!(),
            SymbolKey::Variable(_) => panic!(),
            SymbolKey::XmlFile(x) => self[x].is_in_workspace(),
            SymbolKey::XmlRecord(_) => panic!(),
            SymbolKey::XmlField(_) => panic!(),
            SymbolKey::XmlMenuItem(_) => panic!(),
            SymbolKey::XmlTemplate(_) => panic!(),
            SymbolKey::XmlAsset(_) => panic!(),
            SymbolKey::XmlDelete(_) => panic!(),
            SymbolKey::CsvFile(c) => self[c].is_in_workspace(),
        }
    }

    pub fn set_in_workspace(&mut self, target: SymbolKey, in_workspace: bool) {
        match target {
            SymbolKey::Root(_) => panic!(),
            SymbolKey::Namespace(n) => { self[n].in_workspace = in_workspace },
            SymbolKey::DiskDir(d) => { self[d].in_workspace = in_workspace; },
            SymbolKey::Module(m) => self[m].set_in_workspace(in_workspace),
            SymbolKey::PythonPackage(p) => self[p].set_in_workspace(in_workspace),
            SymbolKey::File(f) => self[f].set_in_workspace(in_workspace),
            SymbolKey::Compiled(_) => panic!(),
            SymbolKey::Class(_) => panic!(),
            SymbolKey::Function(_) => panic!(),
            SymbolKey::Variable(_) => panic!(),
            SymbolKey::XmlFile(x) => self[x].set_in_workspace(in_workspace),
            SymbolKey::XmlRecord(_) => panic!(),
            SymbolKey::XmlField(_) => panic!(),
            SymbolKey::XmlMenuItem(_) => panic!(),
            SymbolKey::XmlTemplate(_) => panic!(),
            SymbolKey::XmlAsset(_) => panic!(),
            SymbolKey::XmlDelete(_) => panic!(),
            SymbolKey::CsvFile(c) => self[c].set_in_workspace(in_workspace),
        }
    }


    pub fn build_status(&self, target: SymbolKey, step: BuildSteps) -> BuildStatus {
        debug_assert!(self.is_key_valid(target)); // expect valid key (self in Symbol method)
        match target {
            SymbolKey::Root(_) => panic!(),
            SymbolKey::Namespace(_) => panic!(),
            SymbolKey::DiskDir(_) => panic!(),
            SymbolKey::PythonPackage(k) => self[k].build_status(step),
            SymbolKey::Module(k) => self[k].build_status(step),
            SymbolKey::File(k) => self[k].build_status(step),
            SymbolKey::Compiled(_) => panic!(),
            SymbolKey::Class(_) => panic!(),
            SymbolKey::Function(k) => self[k].build_status(step),
            SymbolKey::Variable(_) => panic!(),
            SymbolKey::XmlFile(k) => self[k].build_status(step),
            SymbolKey::XmlRecord(_) => panic!(),
            SymbolKey::XmlField(_) => panic!(),
            SymbolKey::XmlMenuItem(_) => panic!(),
            SymbolKey::XmlTemplate(_) => panic!(),
            SymbolKey::XmlAsset(_) => panic!(),
            SymbolKey::XmlDelete(_) => panic!(),
            SymbolKey::CsvFile(k) => self[k].build_status(step),
        }
    }

    pub fn set_build_status(&mut self, target: SymbolKey, step: BuildSteps, status: BuildStatus) {
        debug_assert!(self.is_key_valid(target)); // expect valid key (self in Symbol method)
        match target {
            SymbolKey::Root(_) => panic!(),
            SymbolKey::Namespace(_) => panic!(),
            SymbolKey::DiskDir(_) => panic!(),
            SymbolKey::PythonPackage(k) => self[k].set_build_status(step, status),
            SymbolKey::Module(k) => self[k].set_build_status(step, status),
            SymbolKey::File(k) => self[k].set_build_status(step, status),
            SymbolKey::Compiled(_) => panic!(),
            SymbolKey::Class(_) => panic!(),
            SymbolKey::Function(k) => self[k].set_build_status(step, status),
            SymbolKey::Variable(_) => panic!(),
            SymbolKey::XmlFile(k) => self[k].set_build_status(step, status),
            SymbolKey::XmlRecord(_) => panic!(),
            SymbolKey::XmlField(_) => panic!(),
            SymbolKey::XmlMenuItem(_) => panic!(),
            SymbolKey::XmlTemplate(_) => panic!(),
            SymbolKey::XmlAsset(_) => panic!(),
            SymbolKey::XmlDelete(_) => panic!(),
            SymbolKey::CsvFile(k) => self[k].set_build_status(step, status),
        }
    }

    pub fn iter_symbols(&self, target: SymbolKey) -> hash_map::Iter<'_, OYarn, HashMap<u32, Vec<SymbolKey>>> {
        match target {
            SymbolKey::File(f) => {
                self[f].symbols().iter()
            }
            SymbolKey::Root(_) => panic!(),
            SymbolKey::Namespace(_) => panic!(),
            SymbolKey::DiskDir(_) => panic!(),
            SymbolKey::Module(m) => self[m].symbols().iter(),
            SymbolKey::PythonPackage(p) => self[p].symbols().iter(),
            SymbolKey::Compiled(_) => panic!(),
            SymbolKey::Class(c) => {
                self[c].symbols().iter()
            },
            SymbolKey::Function(f) => {
                self[f].symbols().iter()
            },
            SymbolKey::Variable(_) => panic!(),
            SymbolKey::XmlFile(_) => panic!("despite having symbols, XmlFileSymbol doesn't support sections"),
            SymbolKey::XmlRecord(_) => panic!(),
            SymbolKey::XmlField(_) => panic!(),
            SymbolKey::XmlMenuItem(_) => panic!(),
            SymbolKey::XmlTemplate(_) => panic!(),
            SymbolKey::XmlAsset(_) => panic!(),
            SymbolKey::XmlDelete(_) => panic!(),
            SymbolKey::CsvFile(_) => panic!(),
        }
    }

    pub fn evaluations(&self, target: SymbolKey) -> Option<&Vec<Evaluation>> {
        match target {
            SymbolKey::File(_) => { None },
            SymbolKey::Root(_) => { None },
            SymbolKey::Namespace(_) => { None },
            SymbolKey::DiskDir(_) => { None },
            SymbolKey::PythonPackage(_) => { None },
            SymbolKey::Module(_) => { None },
            SymbolKey::Compiled(_) => { None },
            SymbolKey::Class(_) => { None },
            SymbolKey::Function(f) => Some(&self[f].evaluations),
            SymbolKey::Variable(v) => Some(&self[v].evaluations),
            SymbolKey::XmlFile(_) => None,
            SymbolKey::XmlRecord(_) => None,
            SymbolKey::XmlField(_) => None,
            SymbolKey::XmlMenuItem(_) => None,
            SymbolKey::XmlTemplate(_) => None,
            SymbolKey::XmlAsset(_) => None,
            SymbolKey::XmlDelete(_) => None,
            SymbolKey::CsvFile(_) => None,
        }
    }

    pub fn set_evaluations(&mut self, target: SymbolKey, data: Vec<Evaluation>) {
        match target {
            SymbolKey::File(_) => { panic!() },
            SymbolKey::Root(_) => { panic!() },
            SymbolKey::Namespace(_) => { panic!() },
            SymbolKey::DiskDir(_) => { panic!() },
            SymbolKey::PythonPackage(_) => { panic!() },
            SymbolKey::Module(_) => { panic!() },
            SymbolKey::Compiled(_) => { panic!() },
            SymbolKey::Class(_) => { panic!() },
            SymbolKey::Function(f) => { self[f].evaluations = data; },
            SymbolKey::Variable(v) => { self[v].evaluations = data; },
            SymbolKey::XmlFile(_) => { panic!() },
            SymbolKey::XmlRecord(_) => { panic!() },
            SymbolKey::XmlField(_) => { panic!() },
            SymbolKey::XmlMenuItem(_) => { panic!() },
            SymbolKey::XmlTemplate(_) => { panic!() },
            SymbolKey::XmlAsset(_) => { panic!() },
            SymbolKey::XmlDelete(_) => { panic!() },
            SymbolKey::CsvFile(_) => { panic!() },
        }
    }

    pub fn not_found_paths(&self, target: SourceFileKey) -> &Vec<(BuildSteps, Vec<OYarn>)> {
        match target {
            SourceFileKey::File(f) => { &self[f].not_found_paths },
            SourceFileKey::Module(m) => &self[m].not_found_paths,
            SourceFileKey::PythonPackage(p) => &self[p].not_found_paths,
            SourceFileKey::XmlFile(x) => &self[x].not_found_paths,
            SourceFileKey::CsvFile(c) => &self[c].not_found_paths,
        }
    }

    pub fn not_found_paths_mut(&mut self, target: SourceFileKey) -> &mut Vec<(BuildSteps, Vec<OYarn>)> {
        match target {
            SourceFileKey::File(f) => &mut self[f].not_found_paths,
            SourceFileKey::Module(m) => &mut self[m].not_found_paths,
            SourceFileKey::PythonPackage(p) => &mut self[p].not_found_paths,
            SourceFileKey::XmlFile(x) => &mut self[x].not_found_paths,
            SourceFileKey::CsvFile(x) => &mut self[x].not_found_paths,
        }
    }

    pub fn not_found_models(&self, target: SourceFileKey) -> Option<&HashMap<OYarn, BuildSteps>> {
        match target {
            SourceFileKey::File(f) => Some(&self[f].not_found_models),
            SourceFileKey::XmlFile(f) => Some(&self[f].not_found_models),
            SourceFileKey::Module(m) => Some(&self[m].not_found_models),
            SourceFileKey::PythonPackage(_) => None,
            SourceFileKey::CsvFile(_) => None,
        }
    }

    pub fn not_found_models_mut(&mut self, target: SourceFileKey) -> Option<&mut HashMap<OYarn, BuildSteps>> {
        match target {
            SourceFileKey::File(f) => Some(&mut self[f].not_found_models),
            SourceFileKey::XmlFile(f) => Some(&mut self[f].not_found_models),
            SourceFileKey::Module(m) => Some(&mut self[m].not_found_models),
            SourceFileKey::PythonPackage(_) => None,
            SourceFileKey::CsvFile(_) => None,
        }
    }

    /* Helper to merge dependencies eval_from_ast will fill when called. To be called on a file/package... */
    pub fn insert_dependencies(&mut self, target: SourceFileKey, deps: &[Vec<SourceFileKey>], current_step: BuildSteps) {
        for (step, dependencies) in deps.iter().enumerate() {
            let dep_level = BuildSteps::from(step as i32);
            for &dependency in dependencies {
                if target != dependency {
                    self.add_dependency(target, dependency, current_step, dep_level);
                }
            }
        }
    }

    ///Given a path, create the appropriated symbol and attach it to the given parent
    pub fn create_from_path(session: &mut SessionInfo, path: &PathBuf, parent: SymbolKey, require_module: bool) -> Option<SymbolKey> {
        if require_module {
            let SymbolKey::Namespace(addons) = parent else {
                return None;
            };
            return Self::create_module_from_path(session, path, addons).map(SymbolKey::from)
        }
        let name: String = if path.is_dir() {
            path.components().next_back().unwrap().as_os_str().to_str().unwrap().to_string()
        } else {
            path.with_extension("").components().next_back().unwrap().as_os_str().to_str().unwrap().to_string()
        };
        let path_str = path.sanitize_cow();
        if path_str.ends_with(".py") || path_str.ends_with(".pyi") || FileMgr::is_untitled(&path_str) {
            return Some(session.st_mut().add_new_file(parent, &name, &path_str).into());
        }
        let main_entry_tree = session.sync_odoo.get_main_entry_tree(parent);
        if main_entry_tree == (&["odoo", "addons"], &[]) && path.join("__manifest__.py").exists() {
            if let SymbolKey::Namespace(addons) = parent {
                let module = Self::add_new_module_package(session, addons, &name, path);
                let dir_name = session.st()[module].dir_name.clone();
                session.sync_odoo.modules.insert(dir_name, module.into());
                return Some(module.into());
            } else {
                if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                    let i_ext = if path.join("__init__.py").exists() { "" } else { "i" };
                    let package_key = session.st_mut().add_new_python_package(parent, &name, &path_str, i_ext);
                    return Some(package_key.into());
                } else {
                    return None;
                }
            }
        } else {
            let symbol_table = session.st_mut();
            if path.join("__init__.py").exists() || path.join("__init__.pyi").exists() {
                if main_entry_tree == (&["odoo"], &[]) && path_str.ends_with("addons") {
                    //Force namespace for odoo/addons
                    let namespace_key = symbol_table.add_new_namespace(parent, &name, &path_str);
                    return Some(namespace_key.into());
                } else {
                    let i_ext = if path.join("__init__.py").exists() { "" } else { "i" };
                    let package_key = symbol_table.add_new_python_package(parent, &name, &path_str, i_ext);
                    return Some(package_key.into());
                }
            } else if path.is_dir() {
                let namespace_key = symbol_table.add_new_namespace(parent, &name, &path_str);
                return Some(namespace_key.into());
            }
        }
        None
    }

    pub fn create_module_from_path(session: &mut SessionInfo, path: &PathBuf, addons: NamespaceKey) -> Option<ModuleKey> {
        let main_entry_tree = session.sync_odoo.get_main_entry_tree(addons);
        if !(main_entry_tree == (&["odoo", "addons"], &[]) && path.join("__manifest__.py").exists()) {
            return None;
        }
        let name = path.components().last().unwrap().as_os_str().to_str().unwrap();
        let module = Self::add_new_module_package(session, addons, name, path);
        let dir_name = session.sync_odoo.symbol_table[module].dir_name.clone();
        session.sync_odoo.modules.insert(dir_name, module.into());
        Some(module)
    }

    fn get_tree_helper(&self, symbol_key: SymbolKey) -> (Tree, RootKey) {
        let mut tree = Tree::default();
        let mut current_key = symbol_key;
        while !matches!(current_key, SymbolKey::Root(_)) {
            if self.is_file_content(current_key) {
                tree.1.push(self.name(current_key).clone());
            } else {
                tree.0.push(self.name(current_key).clone());
            }
            current_key = self.parent(current_key).unwrap();
        }
        let root = current_key.unwrap_root_key();
        tree.0.reverse();
        tree.1.reverse();
        (tree, root)
    }


    pub fn get_tree(&self, symbol_key: impl Into<SymbolKey>) -> Tree {
        self.get_tree_helper(symbol_key.into()).0
    }

    pub fn get_tree_and_entry(&self, symbol_key: SymbolKey) -> (Tree, Rc<RefCell<EntryPoint>>) {
        let (tree, root_key) = self.get_tree_helper(symbol_key);
        let entry = self[root_key].entry_point().clone();
        (tree, entry)
    }

    /**
     * Return the tree without the entrypoint tree.
     * As long as the tree starts with the entrypoint tree,
     * otherwise return the full tree, even if it is related to an entrypoint.
     * Which is possible due to relative imports e.g. `from ..module import X`.
     */
    pub fn get_local_tree(&self, symbol_key: SymbolKey) -> Tree {
        let (mut tree, entry) = self.get_tree_and_entry(symbol_key);
        let entry_tree = &entry.borrow().tree;
        if tree.0.starts_with(entry_tree) {
            tree.0.drain(0..entry_tree.len());
        }
        tree
    }

    /// Searches for a symbol at the given tree, starting from the target symbol.
    ///
    /// Note:
    /// The generic type `S` allows for flexibility: `tree` can be a tuple of `&[OYarn]` or `&[&str]`
    /// To call this function with a param of type `Tree`, transform it into its slice form: `tree.as_slice()`
    pub fn get_symbol<S: AsRef<str>>(
        &self,
        target: SymbolKey,
        tree: (&[S], &[S]),
        position: u32,
    ) -> Vec<SymbolKey> {
        let (symbol_tree_files, symbol_tree_content) = tree;
        if symbol_tree_files.is_empty() && symbol_tree_content.is_empty() {
            return vec![];
        }
        let mut iter_sym: Vec<SymbolKey> = vec![target];
        // Walk the file paths
        for section_name in symbol_tree_files {
            iter_sym = iter_sym
                .iter()
                .filter_map(|&sym| self.get_module_symbol(sym, section_name.as_ref()))
                .collect();
            if iter_sym.is_empty() {
                return vec![];
            }
        }
        // Walk the content paths, capturing every symbol
        for content_name in symbol_tree_content {
            iter_sym = iter_sym
                .iter()
                .flat_map(|&sym| {
                    self.get_sub_symbol(sym, content_name.as_ref(), position)
                        .symbols
                })
                .collect();
            if iter_sym.is_empty() {
                return vec![];
            }
        }
        iter_sym
    }

    /*
    Return a symbol that is in module symbols (symbol that represent something on disk - file, package, namespace)
     */
    pub fn get_module_symbol(&self, target: SymbolKey, name: &str) -> Option<SymbolKey> {
        match target {
            SymbolKey::Namespace(n) => {
                for dir in self[n].directories().iter() {
                    let result = dir.module_symbols().get(name);
                    if result.is_some() {
                        return result.copied();
                    }
                }
                None
            },
            SymbolKey::Module(m) => {
                self[m].module_symbols().get(name).copied()
            },
            SymbolKey::PythonPackage(p) => {
                self[p].module_symbols().get(name).copied()
            }
            SymbolKey::Root(r) => {
                self[r].module_symbols().get(name).copied()
            },
            SymbolKey::DiskDir(d) => {
                self[d].module_symbols().get(name).copied()
            }
            _ => {None}
        }
    }

    /**
     * Return all symbol before the given position that match the name in the body of the symbol
     * Return all the symbols that are valid as last declaration for the given position
     */
    pub fn get_content_symbol(&self, target: SymbolKey, name: &str, position: u32) -> ContentSymbols {
        let Some(target_sym_mgr) = self.try_as_symbol_mgr(target) else {
            return ContentSymbols::default();
        };
        let sections = target_sym_mgr.symbols().get(name);
        let mut content = if let Some(sections) = sections {
            let section: SectionRange = target_sym_mgr.get_section_for(position);
            self.get_loc_symbol(target_sym_mgr, sections, position, &SectionIndex::INDEX(section.index), &mut HashSet::default())
        } else {
            ContentSymbols::default()
        };
        let ext_sym = self.get_ext_symbol(target, name);
        if ext_sym.len() > 1 {
            content.symbols.extend(ext_sym.into_iter().map(SymbolKey::from));
            content.always_defined = true;
        }
        content
    }

    /// Return all symbols before the given position that are visible in the body of this symbol.
    pub fn get_all_visible_symbols(&self, target: SymbolKey, name_prefix: &String, position: u32) -> HashMap<OYarn, Vec<SymbolKey>> {
        let Some(target_sym_mgr) = self.try_as_symbol_mgr(target) else {
            return HashMap::default();
        };
        let mut result = HashMap::default();
        let current_section = target_sym_mgr.get_section_for(position);
        let current_index = SectionIndex::INDEX(current_section.index);

        for (name, section_map) in target_sym_mgr.symbols().iter() {
            if !name.starts_with(name_prefix) {
                continue;
            }
            let mut seen = HashSet::default();
            let content = self.get_loc_symbol(target_sym_mgr, section_map, position, &current_index, &mut seen);

            if !content.symbols.is_empty() {
                result.insert(name.clone(), content.symbols);
            }
        }
        result
    }


    /**
     * Return a symbol that can be called from outside of the body of the symbol
     */
    pub fn get_sub_symbol(&self, target: SymbolKey, name: &str, position: u32) -> ContentSymbols {
        match target {
            SymbolKey::Class(_) | SymbolKey::File(_) | SymbolKey::PythonPackage(_) | SymbolKey::Module(_) => {
                self.get_content_symbol(target, name, position)
            },
            SymbolKey::Function(_) | SymbolKey::Namespace(_) => ContentSymbols {
                symbols: self.get_ext_symbol(target, name).into_iter().map(SymbolKey::from).collect(),
                always_defined: true,
            },
            _ => ContentSymbols::default(),
        }
    }

    ///given all the sections of a symbol and a position, return all the Symbols that can represent the symbol
    pub fn get_loc_symbol(&self, target: &dyn SymbolMgr, map: &HashMap<u32, Vec<SymbolKey>>, position: u32, index: &SectionIndex, acc: &mut HashSet<u32>) -> ContentSymbols {
        let mut res = ContentSymbols::default();
        match index {
                SectionIndex::NONE => { return res; },
                SectionIndex::INDEX(index) => {
                if acc.contains(index){
                    res.always_defined = true;
                    return res;
                }
                let section = target.get_sections().get(*index as usize).unwrap();
                //take index and try to find an evaluation. if no evaluation is found, search in previous index, and mix evaluation if there is multiple precedences
                if let Some(symbols) = map.get(index) {
                    for &sym_key in symbols.iter().rev() {
                        if self.range(sym_key).start().to_u32() < position {
                            res.symbols.push(sym_key);
                            break;
                        }
                    }
                }
                acc.insert(*index);
                if !res.symbols.is_empty() {
                    res.always_defined = true;
                    return res;
                }
                res = self.get_loc_symbol(target, map, position, &section.previous_indexes, acc);
            },
            SectionIndex::OR(indexes) => {
                if indexes.is_empty() {
                    unreachable!("Or indexes should not be empty")
                }
                res.always_defined = true;
                for index in indexes.iter() {
                    let sub_result = self.get_loc_symbol(target, map, position, index, acc);
                    res.symbols.extend(sub_result.symbols);
                    res.always_defined = res.always_defined && sub_result.always_defined;
                }
            }
        }
        res
   }

    pub fn get_all_dependencies(&self, target: SourceFileKey, step: BuildSteps) -> &[WeakSet<SourceFileKey>] {
        if step == BuildSteps::SYNTAX {
            panic!("Can't get dependencies for syntax step")
        }
        match target {
            SourceFileKey::Module(m) => self[m].get_all_dependencies(step as usize),
            SourceFileKey::PythonPackage(p) => self[p].get_all_dependencies(step as usize),
            SourceFileKey::File(f) => self[f].get_all_dependencies(step as usize),
            SourceFileKey::XmlFile(x) => self[x].get_all_dependencies(step as usize),
            SourceFileKey::CsvFile(c) => self[c].get_all_dependencies(step as usize),
        }
    }

    /**Add a symbol as dependency on the step of the other symbol for the build level.
    * -> The build of the 'step' of 'target' requires the build of 'dep_level' of 'dependency' to be done */
    pub fn add_dependency(&mut self, target: SourceFileKey, dependency: SourceFileKey, step:BuildSteps, dep_level:BuildSteps) {
        if step == BuildSteps::SYNTAX || dep_level == BuildSteps::SYNTAX {
            panic!("Can't add dependency for syntax step")
        }
        if dep_level > step {
            panic!("Can't add dependency for step {:?} and level {:?}", step, dep_level)
        }
        if target == dependency {
            return;
        }
        if !self.in_workspace(target.into()) || !self.in_workspace(dependency.into()) {
            return;
        }
        let step_i = step as usize;
        let level_i = dep_level as usize;

        // register `depends_on` as a dependency of `target`
        self.dependencies_mut(target)[step_i][level_i].insert(dependency);

        // register `target` as a dependent of `depends_on`
        self.dependents_as_mut(dependency)[level_i][step_i - level_i].insert(target);
    }

    pub fn add_model_dependencies(&mut self, target: SourceFileKey, model: &Rc<RefCell<Model>>) {
        let model_dependencies = match target {
            SourceFileKey::Module(m) => &mut self[m].model_dependencies,
            SourceFileKey::PythonPackage(p) => &mut self[p].model_dependencies,
            SourceFileKey::File(f) => &mut self[f].model_dependencies,
            SourceFileKey::XmlFile(x) => &mut self[x].model_dependencies,
            SourceFileKey::CsvFile(c) => &mut self[c].model_dependencies,
        };
        model_dependencies.insert(model.clone());
        model.borrow_mut().add_dependent(target);
    }


    pub fn invalidate(session: &mut SessionInfo, symbol: SourceFileKey, step: BuildSteps) {
        //signals that a change occurred to this symbol. "step" indicates which level of change occurred.
        //It will trigger rebuild on all dependencies
        let mut vec_to_invalidate = VecDeque::from([symbol]);
        while let Some(ref_to_inv) = vec_to_invalidate.pop_front() {
            let in_workspace = session.st().in_workspace(ref_to_inv.into());
            if step == BuildSteps::ARCH && in_workspace {
                let arch_dependents = &session.st().dependents(ref_to_inv)[BuildSteps::ARCH as usize];
                let mut build_queue = vec![];
                for (index, hashset) in arch_dependents.iter().enumerate() {
                    for sym in hashset.iter_valid(session.st()) {
                        if !session.st().is_symbol_in_parents(sym.into(), ref_to_inv.into()) {
                            build_queue.push((index, sym));
                        }
                    }
                }
                for (index, sym) in build_queue {
                    if index == BuildSteps::ARCH as usize {
                        session.sync_odoo.add_to_rebuild_arch(sym);
                    } else if index == BuildSteps::ARCH_EVAL as usize {
                        session.sync_odoo.add_to_rebuild_arch_eval(sym);
                    } else if index == BuildSteps::VALIDATION as usize {
                        session.st_mut().invalidate_sub_functions(sym);
                        session.sync_odoo.add_to_validations(sym);
                    }
                }
            }
            if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL].contains(&step) && in_workspace {
                let arch_eval_dependents = &session.st().dependents(ref_to_inv)[BuildSteps::ARCH_EVAL as usize];
                let mut build_queue = vec![];
                for (index, hashset) in arch_eval_dependents.iter().enumerate() {
                    for sym in hashset.iter_valid(session.st()) {
                        if !session.st().is_symbol_in_parents(sym.into(), ref_to_inv.into()) {
                            build_queue.push((index, sym));
                        }
                    }
                }
                for (index, sym) in build_queue {
                    if index + 1 == BuildSteps::ARCH_EVAL as usize {
                        session.sync_odoo.add_to_rebuild_arch_eval(sym);
                    } else if index + 1 == BuildSteps::VALIDATION as usize {
                        session.st_mut().invalidate_sub_functions(sym);
                        session.sync_odoo.add_to_validations(sym);
                    }
                }
                for class in session.st().iter_classes(ref_to_inv.into()) {
                    if let Some(model_data) = &session.st()[class]._model {
                        let model = session.sync_odoo.models.get(&model_data.name).cloned();
                        if let Some(model) = model {
                            let from_module = session.st().find_module(class);
                            model.borrow().add_dependents_to_validation(session, from_module);
                        }
                    }
                }
            }
            if [BuildSteps::ARCH, BuildSteps::ARCH_EVAL, BuildSteps::VALIDATION].contains(&step) && in_workspace {
                let validation_dependents = &session.st().dependents(ref_to_inv)[BuildSteps::VALIDATION as usize];
                for sym in validation_dependents.iter()
                        .flat_map(|s| s.iter_valid(session.st()))
                        .collect::<Vec<_>>() {
                    if !session.st_mut().is_symbol_in_parents(sym.into(), ref_to_inv.into()) {
                        session.st_mut().invalidate_sub_functions(sym);
                        session.sync_odoo.add_to_validations(sym);
                    }
                }
            }
            if session.st().has_modules(ref_to_inv.into()) {
                for sym in session.st().all_module_symbol(ref_to_inv.into()).iter()
                        .filter_map(|&s| s.as_source_file_key()) {
                    vec_to_invalidate.push_back(sym);
                }
            }
        }
    }

    pub fn invalidate_sub_functions(&mut self, target: SourceFileKey) {
        if matches!(target, SourceFileKey::File(_) | SourceFileKey::PythonPackage(_) | SourceFileKey::Module(_)) {
            for func_key in self.iter_inner_functions(target.into()) {
                let func = &mut self[func_key];
                func.evaluations.clear();
                func.set_build_status(BuildSteps::ARCH_EVAL, BuildStatus::PENDING);
                func.set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
            }
        }
    }

    pub fn previous_step_done(&self, target: SymbolKey, step: BuildSteps) -> bool {
        if step == BuildSteps::SYNTAX {
            panic!("Can't check previous step for syntax step")
        }
        for i in 0 .. step as usize {
            if self.build_status(target, BuildSteps::from(i as i32)) != BuildStatus::DONE {
                return false;
            }
        }
        true
    }

    pub fn is_file_content(&self, target: SymbolKey) -> bool {
        match target {
            SymbolKey::Root(_)
            | SymbolKey::Namespace(_)
            | SymbolKey::DiskDir(_)
            | SymbolKey::PythonPackage(_)
            | SymbolKey::Module(_)
            | SymbolKey::File(_)
            | SymbolKey::Compiled(_)
            | SymbolKey::XmlFile(_)
            | SymbolKey::CsvFile(_) => false,
            SymbolKey::Class(_)
            | SymbolKey::Function(_)
            | SymbolKey::Variable(_)
            | SymbolKey::XmlRecord(_)
            | SymbolKey::XmlField(_)
            | SymbolKey::XmlMenuItem(_)
            | SymbolKey::XmlTemplate(_)
            | SymbolKey::XmlAsset(_)
            | SymbolKey::XmlDelete(_) => true,
        }
    }


    ///return true if to_test is in parents of symbol or equal to it.
    pub fn is_symbol_in_parents(&self, symbol: SymbolKey, to_test: SymbolKey) -> bool {
        self.has_in_parents(symbol, to_test, false)
    }

    pub fn set_processed_text_hash(&mut self, target: SourceFileKey, hash: u64) {
        match target {
            SourceFileKey::File(f) => self[f].processed_text_hash = hash,
            SourceFileKey::Module(m) => self[m].processed_text_hash = hash,
            SourceFileKey::PythonPackage(p) => self[p].processed_text_hash = hash,
            SourceFileKey::XmlFile(x) => self[x].processed_text_hash = hash,
            SourceFileKey::CsvFile(c) => self[c].processed_text_hash = hash,
        }
    }

    pub fn get_processed_text_hash(&self, target: SourceFileKey) -> u64 {
        match target {
            SourceFileKey::File(f) => self[f].processed_text_hash,
            SourceFileKey::Module(m) => self[m].processed_text_hash,
            SourceFileKey::PythonPackage(p) => self[p].processed_text_hash,
            SourceFileKey::XmlFile(x) => self[x].processed_text_hash,
            SourceFileKey::CsvFile(c) => self[c].processed_text_hash,
        }
    }

    pub fn set_noqas(&mut self, target: SymbolKey, noqa: NoqaInfo) {
        match target {
            SymbolKey::File(f) => self[f].noqas = noqa,
            SymbolKey::DiskDir(_) => panic!("set_noqas called on DiskDir"),
            SymbolKey::Module(m) => self[m].noqas = noqa,
            SymbolKey::PythonPackage(p) => self[p].noqas = noqa,

            SymbolKey::Function(f) => self[f].noqas = noqa,
            SymbolKey::Root(_) => panic!("set_noqas called on Root"),
            SymbolKey::Namespace(_) => panic!("set_noqas called on Namespace"),
            SymbolKey::Compiled(_) => panic!("set_noqas called on Compiled"),
            SymbolKey::Class(c) => self[c].noqas = noqa,
            SymbolKey::Variable(_) => panic!("set_noqas called on Variable"),
            SymbolKey::XmlFile(x) => self[x].noqas = noqa,
            SymbolKey::XmlRecord(_) => panic!("set_noqas called on XmlRecord"),
            SymbolKey::XmlField(_) => panic!("set_noqas called on XmlField"),
            SymbolKey::XmlMenuItem(_) => panic!("set_noqas called on XmlMenuItem"),
            SymbolKey::XmlTemplate(_) => panic!("set_noqas called on XmlTemplate"),
            SymbolKey::XmlAsset(_) => panic!("set_noqas called on XmlAsset"),
            SymbolKey::XmlDelete(_) => panic!("set_noqas called on XmlDelete"),
            SymbolKey::CsvFile(c) => self[c].noqas = noqa,
        }
    }

    pub fn get_noqas(&self, target: SymbolKey) -> NoqaInfo {
        match target {
            SymbolKey::File(f) => self[f].noqas.clone(),
            SymbolKey::Module(m) => self[m].noqas.clone(),
            SymbolKey::PythonPackage(p) => self[p].noqas.clone(),
            SymbolKey::DiskDir(_) => panic!("get_noqas called on DiskDir"),
            SymbolKey::Function(f) => self[f].noqas.clone(),
            SymbolKey::Root(_) => panic!("get_noqas called on Root"),
            SymbolKey::Namespace(_) => panic!("get_noqas called on Namespace"),
            SymbolKey::Compiled(_) => panic!("get_noqas called on Compiled"),
            SymbolKey::Class(c) => self[c].noqas.clone(),
            SymbolKey::Variable(_) => panic!("get_noqas called on Variable"),
            SymbolKey::XmlFile(x) => self[x].noqas.clone(),
            SymbolKey::XmlRecord(_) => panic!("get_noqas called on XmlRecord"),
            SymbolKey::XmlField(_) => panic!("set_noqas called on XmlField"),
            SymbolKey::XmlMenuItem(_) => panic!("get_noqas called on XmlMenuItem"),
            SymbolKey::XmlTemplate(_) => panic!("get_noqas called on XmlTemplate"),
            SymbolKey::XmlAsset(_) => panic!("get_noqas called on XmlAsset"),
            SymbolKey::XmlDelete(_) => panic!("get_noqas called on XmlDelete"),
            SymbolKey::CsvFile(c) => self[c].noqas.clone(),
        }
    }

    pub fn get_in_parents(&self, target: SymbolKey, sym_types: &[SymType], stop_same_file: bool) -> Option<SymbolKey> {
        let mut current = target;
        loop {
            let current_type = current.typ();
            if sym_types.contains(&current_type) {
                return Some(current);
            }
            if stop_same_file && matches!(current_type, SymType::FILE | SymType::PACKAGE(_)) {
                return None;
            }
            current = self.parent(current)?;
        }
    }

    pub fn get_root(&self, target: SymbolKey) -> RootKey {
        let mut current = target;
        while let Some(parent) = self.parent(current) {
            current = parent;
        }
        current.unwrap_root_key()
    }

    pub fn get_entry(&self, target: impl Into<SymbolKey>) -> Rc<RefCell<EntryPoint>> {
        let root = self.get_root(target.into());
        self[root].entry_point().clone()
    }

    pub fn has_in_parents(&self, symbol: SymbolKey, to_test: SymbolKey, stop_same_file: bool) -> bool {
        let mut current = symbol;
        loop {
            if current == to_test {
                return true;
            }
            if stop_same_file && matches!(current.typ(), SymType::FILE | SymType::PACKAGE(_)) {
                return false;
            }
            let Some(parent) = self.parent(current) else {
                return false;
            };
            current = parent;
        }
    }

    /// get a Symbol that has the same given range and name
    pub fn get_positioned_symbol(&self, target: SymbolKey, name: &str, range: &TextRange) -> Option<SymbolKey> {
        if let Some(symbols) = match target {
            SymbolKey::Class(c) => { self[c].symbols().get(name) },
            SymbolKey::File(f) => {self[f].symbols().get(name)},
            SymbolKey::Function(f) => {self[f].symbols().get(name)},
            SymbolKey::Module(m) => {self[m].symbols().get(name)},
            SymbolKey::PythonPackage(p) => {self[p].symbols().get(name)},
            _ => {None}
        } {
            for sym_list in symbols.values() {
                for &key in sym_list.iter() {
                    if self.range(key).start() == range.start() {
                        return Some(key);
                    }
                }
            }
        }
        None
    }

    pub fn get_file(&self, target: SymbolKey) -> Option<SourceFileKey> {
        let mut key = target;
        loop {
            match key {
                SymbolKey::File(f) => return Some(f.into()),
                SymbolKey::Module(m) => return Some(m.into()),
                SymbolKey::PythonPackage(p) => return Some(p.into()),
                SymbolKey::XmlFile(x) => return Some(x.into()),
                SymbolKey::CsvFile(c) => return Some(c.into()),
                _ => {}
            }
            key = self.parent(key)?;
        }
    }

    pub fn parent_file_or_function(&self, target: SymbolKey) -> Option<SymbolKey> {
        self.get_in_parents(
            target,
            &[
                SymType::FILE,
                SymType::PACKAGE(PackageType::PYTHON_PACKAGE),
                SymType::PACKAGE(PackageType::MODULE),
                SymType::FUNCTION,
            ],
            false,
        )
    }

    pub fn find_module(&self, key: impl Into<SymbolKey>) -> Option<ModuleKey> {
        let mut key = key.into();
        loop {
            if let SymbolKey::Module(module) = key {
                return Some(module);
            }
            key = self.parent(key)?;
        }
    }

    fn next_refs_class(session: &mut SessionInfo, class_key: ClassKey, context: Option<&Context>, symbol_context: &Context) -> Vec<EvaluationSymbolPtr> {
        //if current symbol is a descriptor, we have to resolve __get__ method before going further
        let mut res = Vec::new();
        let mut base_attr = symbol_context.get(ContextKey::BaseAttr);
        if base_attr.is_none() {
            //search in context (used in decorators to indicate on which base the field is searched)
            if let Some(context) = context {
                base_attr = context.get(ContextKey::BaseAttr);
            }
        }
        let Some(base_attr) = base_attr else {
            return res;
        };
        let Some(base_attr) = base_attr.as_symbol().upgrade(session.st()) else {
            return res;
        };
        if !matches!(base_attr, SymbolKey::Class(_)) {
            return res;
        }
        //TODO shouldn't we set the from_module in the call to get_member_symbol?
        let get_method = Self::get_member_symbol(session, class_key.into(), "__get__", None, true, false, false, true, false).0.first().copied();
        let Some(SymbolKey::Function(get_method)) = get_method else {
            return res;
        };
        SyncOdoo::ensure_func_evaluations(session, get_method);
        let evaluations = session.st()[get_method].evaluations.clone();
        let merged_context = if let Some(context) = context {
            &Context::merge(context, symbol_context)
        } else {
            symbol_context
        };
        for get_method_eval in evaluations.iter() {
            let get_result = get_method_eval.symbol.get_symbol_as_weak(session, Some(merged_context), &mut vec![], None);
            if !get_result.weak.is_expired(session.st()) {
                let mut eval = Evaluation::eval_from_symbol(session.st(), get_result.weak, get_result.instance);
                match eval.symbol.get_mut_symbol_ptr() {
                    EvaluationSymbolPtr::WEAK(weak) | EvaluationSymbolPtr::SELF(weak) => {
                        if let Some(eval_sym) = weak.weak.upgrade(session.st()) {
                            if eval_sym == class_key {
                                continue;
                            }
                        }
                        weak.context.insert(ContextKey::BaseAttr, ContextValue::SYMBOL(base_attr.into()));
                        res.push(eval.symbol.get_symbol_ptr().clone());
                    },
                    _ => {}
                }
            }
        }
        res
    }

    fn next_refs_variable(session: &mut SessionInfo, key: VariableKey, context: Option<&Context>, symbol_context: &Context) -> Vec<EvaluationSymbolPtr> {
        let mut res = Vec::new();
        let var_symbol = &session.sync_odoo.symbol_table[key];
        let evaluations = var_symbol.evaluations.clone();
        let ctx = if let Some(context) = context {
            &Context::merge(symbol_context, context)
        } else {
            symbol_context
        };
        for eval in evaluations.iter() {
            let mut sym = eval.symbol.get_symbol(session, Some(ctx), &mut vec![], None);
            if let EvaluationSymbolPtr::WEAK(w) | EvaluationSymbolPtr::SELF(w) = &mut sym {
                if let Some(base_attr) = symbol_context.get(ContextKey::BaseAttr) {
                    if !w.context.get(ContextKey::IsAttrOfInstance).map(|x| x.as_bool()).unwrap_or(false) {
                        w.context.insert(ContextKey::BaseAttr, base_attr.clone());
                    }
                }
                if let Some(base_attr) = symbol_context.get(ContextKey::IsAttrOfInstance) {
                    if !w.context.get(ContextKey::IsAttrOfInstance).map(|x| x.as_bool()).unwrap_or(false) {
                        w.context.insert(ContextKey::IsAttrOfInstance, base_attr.clone());
                    }
                }
            }
            if !sym.is_expired_if_weak(&session.sync_odoo.symbol_table) {
                res.push(sym);
            }
        }
        res
    }

    /*given a Symbol, give all the Symbol that are evaluated as valid evaluation for it.
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
    fn next_refs(session: &mut SessionInfo, symbol_key: SymbolKey, context: Option<&Context>, symbol_context: &Context, stop_on_type: bool) -> Vec<EvaluationSymbolPtr> {
        match symbol_key {
            SymbolKey::Class(c) if !stop_on_type => Self::next_refs_class(session, c, context, symbol_context),
            SymbolKey::Variable(v) => Self::next_refs_variable(session, v, context, symbol_context),
            _ => vec![],
        }
    }

    /*
    Follow evaluation of current symbol until type, value or end of the chain, depending or the parameters.
    If a symbol in the chain is a descriptor, return the __get__ return evaluation.
    If filter_on_tree is set, stop following when one of the symbols in the chain is in the tree, and only return those symbols.
        */
    pub fn follow_ref(evaluation: &EvaluationSymbolPtr, session: &mut SessionInfo, context: Option<&Context>, stop_on_type: bool, stop_on_value: bool, filter_on_tree: Option<(&[&str], &[&str])>, max_scope: Option<SymbolKey>) -> Vec<EvaluationSymbolPtr> {
        let default_result = match filter_on_tree.as_ref() {
            Some(_) => vec![],
            None => vec![evaluation.clone()],
        };
        let stop_on_tree_syms = filter_on_tree.map(|tree| session.sync_odoo.get_symbol("", tree, u32::MAX));
        if matches!(stop_on_tree_syms.as_ref(), Some(syms) if syms.is_empty()) {
            // can't find the tree symbol, stop here
            return default_result;
        }
        let EvaluationSymbolPtr::WEAK(w) = evaluation else {
            // Non-weak evaluations are final
            return default_result
        };
        let Some(symbol_key) = w.weak.upgrade(session.st()) else {
            return default_result;
        };
        if stop_on_value {
            if let Some(evals) = session.st().evaluations(symbol_key) {
                for eval in evals.iter() {
                    if eval.value.is_some() {
                        return default_result;
                    }
                }
            }
        }
        let can_eval_external = !session.st().is_external(symbol_key);
        //return a list of all possible evaluation: a weak ptr to the final symbol, and a bool indicating if this is an instance or not
        let mut work_queue: VecDeque<_> = Self::next_refs(session, symbol_key, context, &w.context, stop_on_type).into_iter().collect();
        if work_queue.is_empty() {
            return default_result;
        }
        if w.instance.is_some_and(|v| v) {
            //if the previous evaluation was set to True, we want to keep it
            work_queue = work_queue.into_iter().map(|mut r| {
                if let EvaluationSymbolPtr::WEAK(ref mut weak) = r {
                    weak.instance = Some(true);
                }
                r
            }).collect();
        }
        let mut results = Vec::new();
        let mut visited = HashSet::default();
        while let Some(current_eval) = work_queue.pop_front() {
            let next_ref_weak = match &current_eval {
                EvaluationSymbolPtr::WEAK(weak)
                | EvaluationSymbolPtr::SELF(weak) => weak,
                _ => {
                    // Non-weak references are final
                    results.push(current_eval);
                    continue;
                }
            };
            let Some(sym_key) = next_ref_weak.weak.upgrade(session.st()) else {
                // Discard evaluation to expired reference
                continue;
            };
            // Avoid cycles
            if visited.contains(&sym_key) {
                continue;
            }
            visited.insert(sym_key);
            let next_ref_weak_instance = next_ref_weak.instance.clone();
            match sym_key {
                SymbolKey::Variable(v) => {
                    // let sym = sym_key.borrow();
                    // let var = sym.as_variable();
                    let var = &session.st()[v];
                    if (stop_on_type && matches!(next_ref_weak.is_instance(), Some(false)) && !var.is_import_variable) ||
                        (stop_on_value && var.evaluations.len() == 1 && var.evaluations[0].value.is_some()) ||
                        (max_scope.is_some() && !session.st().has_in_parents(sym_key, max_scope.unwrap(), true)) {
                        // current evaluation is final
                        results.push(current_eval);
                        continue;
                    }
                    if let Some(stop_on_tree_syms) = stop_on_tree_syms.as_ref() {
                        if stop_on_tree_syms.iter().any(|s| *s == sym_key) {
                            results.push(current_eval);
                            continue;
                        }
                    }
                    if var.evaluations.is_empty() && var.name != "__all__" && can_eval_external {
                        //no evaluation? let's check that the file has been evaluated
                        if let Some(file_symbol) = session.st().get_file(sym_key) {
                            SyncOdoo::build_now(session, file_symbol, BuildSteps::ARCH_EVAL);
                        }
                    }
                    let mut next_sym_refs = Self::next_refs_variable(session, v, context, &next_ref_weak.context);
                    if next_sym_refs.is_empty() {
                        // keep current evaluation
                        results.push(current_eval);
                        continue;
                    }
                    // /!\ we want to keep instance = True if previous evaluation was set to True!
                    if next_ref_weak_instance.is_some_and(|v| v) {
                        next_sym_refs = next_sym_refs.into_iter().map(|mut next_results| {
                           match next_results {
                                EvaluationSymbolPtr::WEAK(ref mut weak)
                                | EvaluationSymbolPtr::SELF(ref mut weak) =>  {
                                    weak.instance = Some(true);
                                },
                                _ => {}
                            }
                            next_results
                        }).collect();
                    }
                    // enqueue evaluations to follow, replacing current evaluation
                    work_queue.extend(next_sym_refs);
                },
                SymbolKey::Class(c) if !stop_on_type => {
                    //On class, follow descriptor declarations
                    let next_sym_refs = Self::next_refs_class(session, c, context, &next_ref_weak.context);
                    if next_sym_refs.is_empty() {
                        // keep current evaluation
                        results.push(current_eval);
                    } else {
                        // enqueue evaluations to follow, replacing current evaluation
                        work_queue.extend(next_sym_refs);
                    }
                },
                _ => {
                    results.push(current_eval);
                }
            }
        }
        if let Some(stop_on_tree_syms) = stop_on_tree_syms.as_ref() {
            results.retain(|r| {
                match r {
                    EvaluationSymbolPtr::WEAK(weak) | EvaluationSymbolPtr::SELF(weak) => {
                        if let Some(key) = weak.weak.upgrade(session.st()) {
                            stop_on_tree_syms.iter().any(|&s| s == key)
                        } else {
                            false
                        }
                    },
                    _ => false
                }
            });
        }
        results
    }

    pub fn follow_imported_ref(evaluation: &EvaluationSymbolPtr, session: &mut SessionInfo, context: Option<&Context>) -> Vec<EvaluationSymbolPtr> {
        let mut res = vec![];
        let mut symbols = VecDeque::new();
        symbols.push_back(evaluation.clone());
        while let Some(current_sym) = symbols.pop_front() {
            let EvaluationSymbolPtr::WEAK(w) = &current_sym else {
                res.push(current_sym.clone());
                continue;
            };
            let Some(symbol) = w.weak.upgrade(session.st()) else {
                res.push(current_sym.clone());
                continue;
            };
            if let SymbolKey::Variable(variable_key) = symbol && session.st()[variable_key].is_import_variable {
                let evaluations = session.st()[variable_key].evaluations.clone();
                for eval in evaluations {
                    symbols.push_back(eval.symbol.get_symbol(session, context, &mut vec![], None));
                }
            } else {
                res.push(current_sym);
            }
        }
        res
    }

    pub fn all_symbols(&self, target: SymbolKey) -> Vec<SymbolKey> {
        //return an iterator on all symbols of self. only symbols in symbols and module_symbols will
        //be returned.
        let mut iter: Vec<SymbolKey> = Vec::new();
        match target {
            SymbolKey::File(f) => iter.extend(iter_symbol_keys(&self[f])),
            SymbolKey::Class(c) => iter.extend(iter_symbol_keys(&self[c])),
            SymbolKey::Function(f) => iter.extend(iter_symbol_keys(&self[f])),
            SymbolKey::Module(m) => {
                let module = &self[m];
                iter.extend(iter_symbol_keys(module));
                iter.extend(module.module_symbols().values());
            },
            SymbolKey::PythonPackage(p) => {
                let package = &self[p];
                iter.extend(iter_symbol_keys(package));
                iter.extend(package.module_symbols().values());
            },
            SymbolKey::Namespace(n) => {
                let symbols = self[n].directories().iter().flat_map(|d| d.module_symbols().values());
                iter.extend(symbols);
            },
            SymbolKey::Root(r) => iter.extend(self[r].module_symbols().values()),
            SymbolKey::DiskDir(d) => iter.extend(self[d].module_symbols().values()),
            _ => {}
        }
        iter
    }

    //store in result all available members for symbol: sub symbols, base class elements and models symbols
    //TODO is order right of Vec in HashMap? if we take first or last in it, do we have the last effective value?
    pub fn all_members(
        symbol: SymbolKey,
        session: &mut SessionInfo,
        with_co_models: bool,
        only_fields: bool,
        only_methods: bool,
        from_module: Option<ModuleKey>,
        is_super: bool
    ) -> HashMap<OYarn, Vec<(SymbolKey, Option<OYarn>)>> {
        let mut result: HashMap<OYarn, Vec<(SymbolKey, Option<OYarn>)>> = HashMap::default();
        let mut acc = HashSet::default();
        Self::_all_members(symbol, session, &mut result, with_co_models, only_fields, only_methods, from_module, &mut acc, is_super);
        return  result;
    }

    fn _all_members(symbol_key: SymbolKey, session: &mut SessionInfo, result: &mut HashMap<OYarn, Vec<(SymbolKey, Option<OYarn>)>>, with_co_models: bool, only_fields: bool, only_methods: bool, from_module: Option<ModuleKey>, acc: &mut HashSet<SymbolKey>, is_super: bool) {
        if acc.contains(&symbol_key) {
            return;
        }
        acc.insert(symbol_key);
        let mut append_result = |name: OYarn, symbol: SymbolKey, dep: Option<OYarn>| {
            if let Some(vec) = result.get_mut(&name) {
                vec.push((symbol, dep));
            } else {
                result.insert(name, vec![(symbol, dep)]);
            }
        };
        match symbol_key {
            SymbolKey::Class(class_key) => {
                // Skip current class symbols for super
                if !is_super{
                    for symbol in session.st().all_symbols(symbol_key) {
                        if (only_fields && !Self::is_field(session, symbol)) || (only_methods && !matches!(symbol, SymbolKey::Function(_))) {
                            continue;
                        }
                        let name = session.st().name(symbol).clone();
                        append_result(name, symbol, None);
                    }
                }
                let model_option = session.st()[class_key]._model.as_ref().and_then(|model_data|
                    session.sync_odoo.models.get(&model_data.name).cloned()
                );
                if let Some(model) = model_option && with_co_models {
                    // no recursion because it is handled in all_symbols_inherits
                    let (model_symbols, model_inherits_symbols) = model.borrow().all_symbols_inherits(session, from_module);
                    for (model_key, dependency) in model_symbols {
                        if dependency.is_some() || class_key == model_key {
                            continue;
                        }
                        let model_sym = &session.st()[model_key];
                        let all_symbols = model_sym.children();
                        let model_name = model_sym.name.clone();
                        for s in all_symbols {
                            if (only_fields && !Self::is_field(session, s)) || (only_methods && !matches!(s, SymbolKey::Function(_))) {
                                continue;
                            }
                            let name = session.st().name(s).clone();
                            append_result(name, s, Some(model_name.clone()));
                        }
                    }
                    for (model_key, dependency) in model_inherits_symbols {
                        if dependency.is_some() || class_key == model_key {
                            continue;
                        }
                        let model_sym = &session.st()[model_key];
                        // for inherits symbols, we only add fields
                        let all_symbols = model_sym.children();
                        let model_name = model_sym.name.clone();
                        let fields = all_symbols.into_iter().filter(|&s| Self::is_field(session, s)).collect::<Vec<_>>();
                        for s in fields {
                            let name = session.st().name(s).clone();
                            append_result(name, s, Some(model_name.clone()));
                        }
                    }
                }
                let bases = session.st()[class_key].bases.iter()
                    .filter_map(|base| base.upgrade(session.st()))
                    .collect::<Vec<_>>();
                for base in bases {
                    //no comodel as we will search for co-model from original class (what about overrided _name?)
                    //TODO what about base of co-models classes?
                    Self::_all_members(base.into(), session, result, false, only_fields, only_methods, from_module, acc, false);
                }
            },
            SymbolKey::Function(_) => {
                // A function does not expose its symbols
            },
            // if not class just add it to result
            _ => {
                session.st().all_symbols(symbol_key).into_iter().for_each(|s|
                    if !(only_fields && !Self::is_field(session, s)) {
                        let name = session.st().name(s).clone();
                        append_result(name, s, None);
                    }
                )
            }
        }
    }

    /* return the Symbol (class, function or file) the closest to the given offset */
    pub fn get_scope_symbol(&self, file: impl Into<SymbolKey>, offset: u32, is_param: bool) -> SymbolKey {
        let mut result = file.into();
        let file_sym_mgr = self.as_symbol_mgr(result); // formely Rc (strong)
        let section_id = file_sym_mgr.get_section_for(offset);
        for (_, sym_map) in file_sym_mgr.symbols() {
            let Some(symbols) = sym_map.get(&section_id.index) else { continue };
            for &key in symbols {
                match key {
                    SymbolKey::Class(c) => {
                        let class = &self[c];
                        let range = match is_param {
                            true => class.range.start().to_u32(),
                            false => class.body_range.start().to_u32(),
                        };
                        if range <= offset && class.body_range.end().to_u32() > offset {
                            result = self.get_scope_symbol(key, offset, is_param);
                        }
                    },
                    SymbolKey::Function(f) => {
                        let function = &self[f];
                        let range = match is_param {
                            true => function.range.start().to_u32(),
                            false => function.body_range.start().to_u32(),
                        };
                        if range <= offset && function.body_range.end().to_u32() > offset {
                            result = self.get_scope_symbol(key, offset, is_param);
                        }
                    }
                    _ => {}
                }
            }
        }
        result
    }

    pub fn get_all_inferred_names(&self, on_symbol: SymbolKey, name: &String, position: u32) -> HashMap<OYarn, Vec<SymbolKey>> {
        fn helper(
            symbol_table: &SymbolTable, on_symbol: SymbolKey, name: &String, position: u32, acc: &mut HashMap<OYarn, Vec<SymbolKey>>
        ) {
            // Add symbols from files and functions
            if matches!(on_symbol.typ(), SymType::FILE | SymType::FUNCTION) {
                let symbols_map = symbol_table.get_all_visible_symbols(on_symbol, name, position);
                for (sym_name, sym_vec) in symbols_map {
                    acc.entry(sym_name)
                        .or_default()
                        .extend(sym_vec);
                }
            }
            // Traverse upwards if we are under a class or a function
            if matches!(on_symbol.typ(), SymType::CLASS | SymType::FUNCTION) {
                if let Some(parent) = symbol_table.parent(on_symbol) {
                    helper(symbol_table, parent, name, position, acc);
                }
            }
        }
        let mut results = HashMap::default();
        helper(self, on_symbol, name, position, &mut results);
        results
    }

    //infer a name, given a position
    pub fn infer_name(odoo: &SyncOdoo, on_symbol: SymbolKey, name: &String, position: Option<u32>) -> ContentSymbols {
        let symbol_table = &odoo.symbol_table;
        let results = symbol_table.get_content_symbol(on_symbol, name, position.unwrap_or(u32::MAX));
        if !results.symbols.is_empty() {
            return results;
        }
        let on_symbol_type = on_symbol.typ();
        if !matches!(on_symbol_type, SymType::FILE | SymType::PACKAGE(_) | SymType::ROOT) {
            let mut parent = symbol_table.parent(on_symbol).unwrap();
            while let SymbolKey::Class(c) = parent {
                parent = symbol_table[c].parent();
            }
            // A function can reference another name from the full outer scope so no position is needed
            Self::infer_name(odoo, parent, name, None)
        } else if symbol_table.name(on_symbol) != "builtins" || on_symbol_type != SymType::FILE {
            let builtins = odoo.get_symbol("", (&["builtins"], &[]), u32::MAX)[0];
            Self::infer_name(odoo, builtins, name, None)
        } else {
            ContentSymbols::default()
        }
    }

    /* Hook for get_member_symbol
    Position is set to [0,0], because inside the method there is no concept of the current position.
    The setting of the position is then delegated to the calling function.
    TODO Consider refactoring.
        */
    fn member_symbol_hook(session: &SessionInfo, target: SymbolKey, name: &str, diagnostics: &mut Vec<Diagnostic>){
        if session.sync_odoo.version.major >= 17 && name == "Form"{
            let tree = session.sync_odoo.symbol_table.get_tree(target);
            if tree.0.ends_with_strs(&["odoo", "tests", "common"]) && tree.1.is_empty() {
                if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03301, &[]) {
                    diagnostics.push(
                        Diagnostic {
                            range: Range::default(),
                            tags: Some(vec![DiagnosticTag::DEPRECATED]),
                            ..diagnostic_base.clone()
                        }
                    );
                }
            }
        }
    }


    pub fn is_field(session: &mut SessionInfo, target: SymbolKey) -> bool {
        let SymbolKey::Variable(v) = target else {
            return false;
        };
        let var_symbol = &session.sync_odoo.symbol_table[v];
        let evaluations = var_symbol.evaluations.clone();
        for eval in evaluations {
            let symbol = eval.symbol.get_symbol(session, None,  &mut vec![], None);
            let eval_weaks = Self::follow_ref(&symbol, session, None, true, false, None, None);
            for eval_weak in eval_weaks.iter() {
                if let Some(key) = eval_weak.upgrade_weak(&session.sync_odoo.symbol_table) {
                    if Self::is_field_class(session, key) {
                        return true;
                    }
                }
            }
        }
        false
    }


    fn is_method(session: &mut SessionInfo, target: SymbolKey) -> bool {
        if matches!(target, SymbolKey::Function(_)) {
            return true;
        }
        let SymbolKey::Variable(v) = target else {
            return false;
        };
        let var_symbol = &session.sync_odoo.symbol_table[v];
        let evals = var_symbol.evaluations.clone();
        for eval in evals.iter() {
            let symbol = eval.symbol.get_symbol(session, None,  &mut vec![], None);
            let eval_weaks = Self::follow_ref(&symbol, session, None, true, false, None, None);
            for eval_weak in eval_weaks.iter() {
                if let Some(key) = eval_weak.upgrade_weak(&session.sync_odoo.symbol_table) {
                    if matches!(key, SymbolKey::Function(_)) {
                        return true;
                    }
                }
            }
        }
        false
    }


    pub fn is_inheriting_from_field(session: &SessionInfo, class_key: ClassKey) -> bool {
        let tree = session.sync_odoo.get_main_entry_tree(class_key).flatten();
        if session.sync_odoo.version <= (18, 0) {
            if tree == ["odoo", "fields", "Field"] {
                return true;
            }

        } else {
            if tree == ["odoo", "orm", "fields", "Field"] {
                return true;
            }
        }
        // Follow class inheritance
        let symbol_table = &session.sync_odoo.symbol_table;
        let class_symbol = &symbol_table[class_key];
        for base_key in class_symbol.bases.iter().filter_map(|w| w.upgrade(symbol_table)) {
            if Self::is_inheriting_from_field(session, base_key) {
                return true;
            }
        }
        false
    }

    pub fn is_field_class(session: &SessionInfo, symbol_key: SymbolKey) -> bool {
        // if not class return false
        let SymbolKey::Class(class_key) = symbol_key else {
            return false;
        };
        let symbol_table = &session.sync_odoo.symbol_table;
        let class_symbol = &symbol_table[class_key];
        let cache = &class_symbol._is_field_class;
        if let Some(is_field_class) = *cache.borrow() {
            return is_field_class;
        }
        let result = Self::is_field_class_uncached(session, class_key);
        cache.borrow_mut().replace(result);
        result
    }

    fn is_field_class_uncached(session: &SessionInfo, class_key: ClassKey) -> bool {
        let tree = &session.sync_odoo.get_main_entry_tree(class_key);
        if session.sync_odoo.version >= (18, 1) {
            if tree.0.len() == 3 && tree.1.len() == 1 && tree.0[0] == "odoo" && tree.0[1] == "orm" && (
                    tree.0[2] == "fields_misc" && tree.1[0] == "Boolean" ||
                    tree.0[2] == "fields_numeric" && tree.1[0] == "Integer" ||
                    tree.0[2] == "fields_numeric" && tree.1[0] == "Float" ||
                    tree.0[2] == "fields_numeric" && tree.1[0] == "Monetary" ||
                    tree.0[2] == "fields_textual" && tree.1[0] == "Char" ||
                    tree.0[2] == "fields_textual" && tree.1[0] == "Text" ||
                    tree.0[2] == "fields_textual" && tree.1[0] == "Html" ||
                    tree.0[2] == "fields_temporal" && tree.1[0] == "Date" ||
                    tree.0[2] == "fields_temporal" && tree.1[0] == "Datetime" ||
                    tree.0[2] == "fields_binary" && tree.1[0] == "Binary" ||
                    tree.0[2] == "fields_binary" && tree.1[0] == "Image" ||
                    tree.0[2] == "fields_selection" && tree.1[0] == "Selection" ||
                    tree.0[2] == "fields_reference" && tree.1[0] == "Reference" ||
                    tree.0[2] == "fields_relational" && tree.1[0] == "Many2one" ||
                    tree.0[2] == "fields_reference" && tree.1[0] == "Many2oneReference" ||
                    tree.0[2] == "fields_misc" && tree.1[0] == "Json" ||
                    tree.0[2] == "fields_properties" && tree.1[0] == "Properties" ||
                    tree.0[2] == "fields_properties" && tree.1[0] == "PropertiesDefinition" ||
                    tree.0[2] == "fields_relational" && tree.1[0] == "One2many" ||
                    tree.0[2] == "fields_relational" && tree.1[0] == "Many2many" ||
                    tree.0[2] == "fields_misc" && tree.1[0] == "Id"
            ){
                return true;
            }
        } else {
            if tree.0.len() == 2 && tree.1.len() == 1 && tree.0[0] == "odoo" && tree.0[1] == "fields" {
                if matches!(tree.1[0].as_str(), "Boolean" | "Integer" | "Float" | "Monetary" | "Char" | "Text" | "Html" | "Date" | "Datetime" |
            "Binary" | "Image" | "Selection" | "Reference" | "Json" | "Properties" | "PropertiesDefinition" | "Id" | "Many2one" | "One2many" | "Many2many" | "Many2oneReference") {
                    return true;
                }
            }
        }
        if Self::is_inheriting_from_field(session, class_key) {
            return true;
        }
        false
    }

    pub fn is_specific_field_class(session: &SessionInfo, target: SymbolKey, field_names: &[&str]) -> bool {
        let tree = session.sync_odoo.get_main_entry_tree(target).flatten();
        let Some(tree_last) = tree.last() else {
            return false;
        };
        return Self::is_field_class(session, target)
            && field_names.iter().any(|&name| { tree_last == name })
    }

    pub fn is_specific_field(session: &mut SessionInfo, target: SymbolKey, field_names: &[&str]) -> bool {
        let SymbolKey::Variable(v) = target else {
            return false;
        };
        let evaluations = session.st()[v].evaluations.clone();
        for eval in evaluations.iter() {
            let symbol = eval.symbol.get_symbol(session, None, &mut vec![], None);
            let eval_weaks = Self::follow_ref(&symbol, session, None, true, false, None, None);
            for eval_weak in eval_weaks.iter() {
                if let Some(symbol) = eval_weak.upgrade_weak(session.st()) {
                    if Self::is_specific_field_class(session, symbol, field_names){
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn all_fields(symbol: SymbolKey, session: &mut SessionInfo, from_module: Option<ModuleKey>) -> HashMap<OYarn, Vec<(SymbolKey, Option<OYarn>)>> {
        Self::all_members(symbol, session, true, true, false, from_module, false)
    }


    /* similar to get_symbol: will return the symbol that is under this one with the specified name.
    However, if the symbol is a class or a model, it will search in the base class or in comodel classes
    if not all, it will return the first found. If all, the all found symbols are returned, but the first one
    is the one that is overriding others.
    :param: from_module: optional, can change the from_module of the given class */
    pub fn get_member_symbol(
        session: &mut SessionInfo,
        target: SymbolKey,
        name: &str,
        from_module: Option<ModuleKey>,
        prevent_comodel: bool,
        only_fields: bool,
        only_methods: bool,
        all: bool,
        is_super: bool
    ) -> (Vec<SymbolKey>, Vec<Diagnostic>) {
        let mut visited_classes: HashSet<ClassKey> = HashSet::default();
        return Self::_get_member_symbol_helper(session, target, name, from_module, prevent_comodel, only_fields, only_methods, all, is_super, &mut visited_classes);
    }

    fn _get_member_symbol_helper(
        session: &mut SessionInfo,
        target: SymbolKey,
        name: &str,
        from_module: Option<ModuleKey>,
        prevent_comodel: bool,
        only_fields: bool,
        only_methods: bool,
        all: bool,
        is_super: bool,
        visited_classes: &mut HashSet<ClassKey>
    ) -> (Vec<SymbolKey>, Vec<Diagnostic>) {
        let mut result: Vec<SymbolKey> = vec![];
        let mut visited_symbols: HashSet<SymbolKey> = HashSet::default();
        let extend_result = |syms: Vec<SymbolKey>, result: &mut Vec<SymbolKey>, visited_symbols: &mut HashSet<SymbolKey>| {
            syms.iter().for_each(|&sym|{
                if !visited_symbols.contains(&sym){
                    visited_symbols.insert(sym);
                    result.push(sym);
                }
            });
        };
        let mut diagnostics: Vec<Diagnostic> = vec![];
        Self::member_symbol_hook(session, target, name, &mut diagnostics);
        let mod_sym = session.st().get_module_symbol(target, name);
        if let Some(mod_sym) = mod_sym {
            if !only_fields {
                if all {
                    extend_result(vec![mod_sym], &mut result, &mut visited_symbols);
                } else {
                    return (vec![mod_sym], diagnostics);
                }
            }
        }
        if !is_super {
            let mut content_syms = session.st().get_sub_symbol(target, name, u32::MAX).symbols;
            if only_fields {
                content_syms = content_syms.iter().filter(|&&x| Self::is_field(session, x)).copied().collect();
            }
            if only_methods {
                content_syms = content_syms.iter().filter(|&&x| Self::is_method(session, x)).copied().collect();
            }
            if !content_syms.is_empty() {
                if all {
                    extend_result(content_syms, &mut result, &mut visited_symbols);
                } else {
                    return (content_syms, diagnostics);
                }
            }
        }
        let SymbolKey::Class(c) = target else {
            return (result, diagnostics);
        };
        let model_data = &session.st()[c]._model;
        if model_data.is_some() && !prevent_comodel {
            let model = session.sync_odoo.models.get(&model_data.as_ref().unwrap().name).cloned();
            if let Some(model) = model {
                let mut from_module = from_module;
                if from_module.is_none() {
                    from_module = session.st().find_module(target);
                }
                if let Some(from_module) = from_module {
                    let model_symbols = Model::get_full_model_symbols(model.clone(), session, from_module);
                    for model_symbol in model_symbols {
                        if target == model_symbol || visited_classes.contains(&model_symbol) {
                            continue;
                        }
                        visited_classes.insert(model_symbol);
                        let (attributs, att_diagnostic) = Self::_get_member_symbol_helper(session, model_symbol.into(), name, None, true, only_fields, only_methods, all, false, visited_classes);
                        diagnostics.extend(att_diagnostic);
                        if all {
                            extend_result(attributs, &mut result, &mut visited_symbols);
                        } else {
                            if !attributs.is_empty() {
                                return (attributs, diagnostics);
                            }
                        }
                    }
                    for model_inherits_symbol in model.clone().borrow().get_inherits_models(session, from_module) {
                        //only fields are visible on inherits, not methods
                        let model_symbols = Model::get_full_model_symbols(model_inherits_symbol, session, from_module);
                        for model_symbol in model_symbols {
                            if target == model_symbol || visited_classes.contains(&model_symbol) {
                                continue;
                            }
                            visited_classes.insert(model_symbol);
                            let (attributs, att_diagnostic) = Self::_get_member_symbol_helper(session, model_symbol.into(), name, None, true, true, only_methods, all, false, visited_classes);
                            diagnostics.extend(att_diagnostic);
                            if all {
                                extend_result(attributs, &mut result, &mut visited_symbols);
                            } else {
                                if !attributs.is_empty() {
                                    return (attributs, diagnostics);
                                }
                            }
                        }
                    }
                }
            }
        }
        if result.is_empty() { // if we already have something, do not go up in bases
            let class_sym = &session.st()[c];
            let bases = class_sym.bases.iter().filter_map(|w| w.upgrade(session.st())).collect::<Vec<_>>();
            for base in bases {
                if visited_classes.contains(&base){
                    continue;
                }
                visited_classes.insert(base);
                let (s, s_diagnostic) = Self::get_member_symbol(session, base.into(), name, from_module, prevent_comodel, only_fields, only_methods, all, false);
                    diagnostics.extend(s_diagnostic);
                if !s.is_empty() {
                    if all {
                        extend_result(s, &mut result, &mut visited_symbols);
                    } else {
                        return (s, diagnostics);
                    }
                }
            }
        }
        (result, diagnostics)
    }


    /**
     * Only browse file content, do not use on namespace or packages to browse disk
     * return a list of functions under Class symbol
     */
    pub fn iter_inner_functions(&self, key: SymbolKey) -> Vec<FunctionKey> {
        let mut res = vec![];

        fn iter_recursive(table: &SymbolTable, key: SymbolKey, res: &mut Vec<FunctionKey>) {
            match key {
                SymbolKey::Class(c) => {
                    for child_key in iter_symbol_keys(&table[c]) {
                        if let SymbolKey::Function(fk) = child_key {
                            res.push(*fk);
                        }
                    }
                },
                SymbolKey::File(f) => {
                    for child_key in iter_symbol_keys(&table[f]) {
                        iter_recursive(table, *child_key, res);
                    }
                },
                SymbolKey::Function(f) => {
                    for child_key in iter_symbol_keys(&table[f]) {
                        iter_recursive(table, *child_key, res);
                    }
                },
                SymbolKey::DiskDir(_)
                | SymbolKey::Root(_)
                | SymbolKey::Namespace(_)
                | SymbolKey::PythonPackage(_)
                | SymbolKey::Module(_)
                | SymbolKey::Compiled(_)
                | SymbolKey::Variable(_)
                | SymbolKey::XmlFile(_)
                | SymbolKey::XmlRecord(_)
                | SymbolKey::XmlField(_)
                | SymbolKey::XmlMenuItem(_)
                | SymbolKey::XmlTemplate(_)
                | SymbolKey::XmlAsset(_)
                | SymbolKey::XmlDelete(_)
                | SymbolKey::CsvFile(_) => {},
            }
        }

        iter_recursive(self, key, &mut res);
        res
    }

    pub fn iter_classes(&self, key: SymbolKey) -> Vec<ClassKey> {
        let mut res = vec![];

        fn iter_recursive(table: &SymbolTable, key: SymbolKey, res: &mut Vec<ClassKey>) {
            match key {
                SymbolKey::Class(c) => {
                    res.push(c);
                    let class_sym = &table[c];
                    for child_key in iter_symbol_keys(class_sym) {
                        iter_recursive(table, *child_key, res);
                    }
                },
                SymbolKey::File(f) => {
                    let file_sym = &table[f];
                    for child_key in iter_symbol_keys(file_sym) {
                        iter_recursive(table, *child_key, res);
                    }
                },
                SymbolKey::Function(f) => {
                    let func_sym = &table[f];
                    for child_key in iter_symbol_keys(func_sym) {
                        iter_recursive(table, *child_key, res);
                    }
                },
                SymbolKey::DiskDir(_)
                | SymbolKey::Root(_)
                | SymbolKey::Namespace(_)
                | SymbolKey::PythonPackage(_)
                | SymbolKey::Module(_)
                | SymbolKey::Compiled(_)
                | SymbolKey::Variable(_)
                | SymbolKey::XmlFile(_)
                | SymbolKey::XmlRecord(_)
                | SymbolKey::XmlField(_)
                | SymbolKey::XmlMenuItem(_)
                | SymbolKey::XmlTemplate(_)
                | SymbolKey::XmlAsset(_)
                | SymbolKey::XmlDelete(_)
                | SymbolKey::CsvFile(_) => {},
            }
        }

        iter_recursive(self, key, &mut res);

        res
    }

    pub fn get_lsp_symbol_kind(target: SymbolKey) -> SymbolKind {
        match target.typ() {
            SymType::CLASS => SymbolKind::CLASS,
            SymType::FUNCTION => SymbolKind::FUNCTION,
            SymType::VARIABLE => SymbolKind::VARIABLE,
            SymType::FILE | SymType::CSV_FILE | SymType::XML_FILE => SymbolKind::FILE,
            SymType::XML_RECORD => SymbolKind::CONSTANT,
            SymType::XML_FIELD => SymbolKind::CONSTANT,
            SymType::XML_MENUITEM => SymbolKind::CONSTANT,
            SymType::XML_TEMPLATE => SymbolKind::CONSTANT,
            SymType::XML_ASSET => SymbolKind::CONSTANT,
            SymType::XML_DELETE => SymbolKind::CONSTANT,
            SymType::PACKAGE(_) => SymbolKind::PACKAGE,
            SymType::NAMESPACE => SymbolKind::NAMESPACE,
            SymType::DISK_DIR | SymType::COMPILED => SymbolKind::FILE,
            SymType::ROOT => SymbolKind::NAMESPACE,
        }
    }

    pub fn get_xml_id(&self, xml_data_key: XmlDataKey) -> Option<OYarn> {
        match xml_data_key {
            XmlDataKey::RECORD(r) => self[r].xml_id.clone(),
            XmlDataKey::MENUITEM(m) => self[m].xml_id.clone(),
            XmlDataKey::TEMPLATE(t) => self[t].xml_id.clone(),
            XmlDataKey::ASSET(a) => self[a].xml_id.clone(),
            XmlDataKey::DELETE(d) => self[d].xml_id.clone(),
        }
    }

    /// Util for debug logs
    pub fn debug_path(&self, target: SymbolKey) -> String {
        self.paths(target).first().cloned().unwrap_or(self.name(target).to_string())
    }


    pub fn get_file_info_for_validation(
        session: &mut SessionInfo,
        symbol: SourceFileKey,
    ) -> Option<Rc<RefCell<FileInfo>>> {
        let tree_path = session.sync_odoo.symbol_table.path(symbol).to_owned();
        match session
            .sync_odoo
            .get_file_mgr()
            .borrow()
            .get_file_info(session.sync_odoo.symbol_table.file_path(symbol))
        {
            Some(file_info) => Some(file_info),
            None => {
                let (updated, result) = session
                    .sync_odoo
                    .get_file_mgr()
                    .borrow_mut()
                    .update_file_info(session, &tree_path, None, Some(-100), true);
                if updated {
                    Some(result)
                } else {
                    warn!(
                        "File info not found for validating symbol: {} at path {}",
                        session.sync_odoo.symbol_table.name(symbol),
                        tree_path
                    );
                    return None;
                }
            }
        }
    }

}
