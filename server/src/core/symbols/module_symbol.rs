use lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range};
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};
use tracing::info;
use weak_table::PtrWeakHashSet;
use std::collections::{HashMap, HashSet};

use crate::{constants::*, oyarn, Sy};
use crate::core::file_mgr::{add_diagnostic, FileInfo, FileMgr, NoqaInfo};
use crate::core::import_resolver::find_module;
use crate::core::model::Model;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol::Symbol;
use crate::constants::EXTENSION_NAME;
use crate::core::symbols::symbol_mgr::SymbolMgr;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;
use crate::S;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::cell::RefCell;

use super::symbol_mgr::SectionRange;


#[derive(Debug)]
pub struct ModuleSymbol {
    pub name: OYarn,
    pub path: String,
    pub i_ext: String,
    pub is_external: bool,
    root_path: String,
    loaded: bool,
    module_name: OYarn,
    pub dir_name: OYarn,
    depends: Vec<(OYarn, TextRange)>,
    all_depends: HashSet<OYarn>, //computed all depends to avoid too many recomputations
    data: Vec<String>, // TODO
    pub module_symbols: HashMap<OYarn, Rc<RefCell<Symbol>>>,
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub odoo_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub in_workspace: bool,
    pub model_dependencies: PtrWeakHashSet<Weak<RefCell<Model>>>, //always on validation level, as odoo step is always required
    pub dependencies: Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>,
    pub dependents: Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>,
    pub processed_text_hash: u64,
    pub noqas: NoqaInfo,

    //Trait SymbolMgr
    pub sections: Vec<SectionRange>,
    pub symbols: HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>,
    //--- dynamics variables
    pub ext_symbols: HashMap<OYarn, Vec<Rc<RefCell<Symbol>>>>,
}

impl ModuleSymbol {

    pub fn new(session: &mut SessionInfo, name: String, dir_path: &PathBuf, is_external: bool) -> Option<Self> {
        let mut module = ModuleSymbol {
            name: oyarn!("{}", name),
            path: dir_path.sanitize(),
            i_ext: S!(""),
            is_external,
            not_found_paths: vec![],
            in_workspace: false,
            root_path: dir_path.sanitize(),
            loaded: false,
            module_name: OYarn::from(""),
            dir_name: OYarn::from(""),
            depends: vec!((OYarn::from("base"), TextRange::default())),
            all_depends: HashSet::new(),
            data: Vec::new(),
            weak_self: None,
            parent: None,
            module_symbols: HashMap::new(),
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            odoo_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: vec![],
            dependents: vec![],
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
        };
        module._init_symbol_mgr();
        info!("building new module: {:?}", dir_path.sanitize());
        if dir_path.components().last().unwrap().as_os_str().to_str().unwrap() == "base" {
            module.depends.clear();
        }
        module.dir_name = oyarn!("{}", dir_path.with_extension("").components().last().unwrap().as_os_str().to_str().unwrap());
        let manifest_path = dir_path.join("__manifest__.py");
        if !manifest_path.exists() {
            return None
        }
        let (updated, manifest_file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, manifest_path.sanitize().as_str(), None, None, false);
        let mut manifest_file_info = (*manifest_file_info).borrow_mut();
        if manifest_file_info.file_info_ast.borrow().ast.is_none() {
            manifest_file_info.prepare_ast(session);
        }
        if manifest_file_info.file_info_ast.borrow().ast.is_none() {
            return None;
        }
        let diags = module._load_manifest(session, &manifest_file_info);
        if session.sync_odoo.modules.contains_key(&module.dir_name) {
            //TODO: handle multiple modules with the same name
        }
        manifest_file_info.replace_diagnostics(crate::constants::BuildSteps::SYNTAX, diags);
        manifest_file_info.publish_diagnostics(session);
        drop(manifest_file_info);
        info!("End building new module: {:?}", dir_path.sanitize());
        Some(module)
    }

    pub fn add_symbol(&mut self, content: &Rc<RefCell<Symbol>>, section: u32) {
        let sections = self.symbols.entry(content.borrow().name().clone()).or_insert_with(|| HashMap::new());
        let section_vec = sections.entry(section).or_insert_with(|| vec![]);
        section_vec.push(content.clone());
    }

    pub fn load_module_info(symbol: Rc<RefCell<Symbol>>, session: &mut SessionInfo, odoo_addons: Rc<RefCell<Symbol>>) -> Vec<OYarn> {
        {
            let _symbol = symbol.borrow();
            if _symbol.as_module_package().loaded {
                return vec![];
            }
        }
        let (mut diagnostics, mut loaded) = ModuleSymbol::_load_depends(&mut (*symbol).borrow_mut(), session, odoo_addons);
        diagnostics.append(&mut ModuleSymbol::_load_data(symbol.clone(), session.sync_odoo));
        diagnostics.append(&mut ModuleSymbol::_load_arch(symbol.clone(), session));
        {
            let mut _symbol = symbol.borrow_mut();
            let module = _symbol.as_module_package_mut();
            module.loaded = true;
            loaded.push(module.dir_name.clone());
            let manifest_path = PathBuf::from(module.root_path.clone()).join("__manifest__.py");
            let manifest_file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest_path.sanitize()).expect("file not found in cache").clone();
            let mut manifest_file_info = (*manifest_file_info).borrow_mut();
            manifest_file_info.replace_diagnostics(crate::constants::BuildSteps::ARCH, diagnostics);
            manifest_file_info.publish_diagnostics(session);
        }
        loaded
    }

    /* Load manifest to identify the module characteristics.
    Returns list of od diagnostics to publish in manifest file. */
    fn _load_manifest(&mut self, session: &mut SessionInfo, file_info: &FileInfo) -> Vec<Diagnostic> {
        let mut res = vec![];
        let file_info_ast = file_info.file_info_ast.borrow();
        let ast = file_info_ast.ast.as_ref().unwrap();
        if ast.len() != 1 || !matches!(ast.first(), Some(Stmt::Expr(expr)) if expr.value.is_dict_expr()) {
            add_diagnostic(&mut res, Diagnostic::new(
                Range::new(Position::new(0, 0), Position::new(0, 1)),
                Some(DiagnosticSeverity::ERROR),
                Some(NumberOrString::String(S!("OLS30201"))),
                Some(EXTENSION_NAME.to_string()),
                "A manifest should contain exactly one dictionary".to_string(),
                None,
                None,
            ), &session.current_noqa);
            return res;
        }
        let mut visited_keys = HashSet::new();
        let dict = &ast[0].as_expr_stmt().unwrap().value.clone().dict_expr().unwrap();
        for (index, key) in dict.iter_keys().enumerate() {
            match key {
                Some(key) => {
                    let value = &dict.items.get(index).unwrap().value;
                    match key {
                        Expr::StringLiteral(key_literal) => {
                            let key_str = key_literal.value.to_string();
                            if visited_keys.contains(&key_str){
                                add_diagnostic(&mut res, self._create_diagnostic_for_manifest_key("A manifest should not have duplicate keys", S!("OLS30202"), &key_literal.range, Some(DiagnosticSeverity::WARNING)), &session.current_noqa);
                            }
                            visited_keys.insert(key_str.clone());
                            if key_str == "name" {
                                if !value.is_string_literal_expr() {
                                    add_diagnostic(&mut res, self._create_diagnostic_for_manifest_key("The name of the module should be a string", S!("OLS30203"), &key_literal.range, Some(DiagnosticSeverity::ERROR)), &session.current_noqa);
                                } else {
                                    self.module_name = oyarn!("{}", value.as_string_literal_expr().unwrap().value);
                                }
                            } else if key_str == "depends" {
                                if !value.is_list_expr() {
                                    add_diagnostic(&mut res, self._create_diagnostic_for_manifest_key("The depends value should be a list", S!("OLS30204"), &key_literal.range, Some(DiagnosticSeverity::ERROR)), &session.current_noqa);
                                } else {
                                    for depend in value.as_list_expr().unwrap().elts.iter() {
                                        if !depend.is_string_literal_expr() {
                                            add_diagnostic(&mut res, self._create_diagnostic_for_manifest_key("The depends key should be a list of strings", S!("OLS30205"), &depend.range(), Some(DiagnosticSeverity::ERROR)), &session.current_noqa);
                                        } else {
                                            let depend_value = oyarn!("{}", depend.as_string_literal_expr().unwrap().value);
                                            if depend_value == self.dir_name {
                                                add_diagnostic(&mut res, self._create_diagnostic_for_manifest_key("A module cannot depends on itself", S!("OLS30206"), &depend.range(), Some(DiagnosticSeverity::ERROR)), &session.current_noqa);
                                            } else {
                                                self.depends.push((depend_value, depend.range().clone()));
                                            }
                                        }
                                    }
                                }
                            } else if key_str == "data" {
                                if !value.is_list_expr() {
                                    add_diagnostic(&mut res, self._create_diagnostic_for_manifest_key("The data value should be a list", S!("OLS30207"), &key_literal.range, Some(DiagnosticSeverity::ERROR)), &session.current_noqa);
                                } else {
                                    for data in value.as_list_expr().unwrap().elts.iter() {
                                        if !data.is_literal_expr() {
                                            add_diagnostic(&mut res, self._create_diagnostic_for_manifest_key("The data key should be a list of strings", S!("OLS30208"), &data.range(), Some(DiagnosticSeverity::ERROR)), &session.current_noqa);
                                        } else {
                                            self.data.push(data.as_string_literal_expr().unwrap().value.to_string());
                                        }
                                    }
                                }
                            } else if key_str == "active" {
                                add_diagnostic(&mut res, Diagnostic::new(
                                    Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                                    Some(DiagnosticSeverity::WARNING),
                                    Some(NumberOrString::String(S!("OLS20201"))),
                                    Some(EXTENSION_NAME.to_string()),
                                    "The active key is deprecated".to_string(),
                                    None,
                                    Some(vec![DiagnosticTag::DEPRECATED]),
                                ), &session.current_noqa)
                            }
                        }
                        _ => {
                            add_diagnostic(&mut res, self._create_diagnostic_for_manifest_key("Manifest keys should be strings", S!("OLS30209"), &key.range(), Some(DiagnosticSeverity::ERROR)), &session.current_noqa);
                        }
                    }
                },
                None => {
                    add_diagnostic(&mut res, Diagnostic::new(
                        Range::new(Position::new(0, 0), Position::new(0, 1)),
                        Some(DiagnosticSeverity::ERROR),
                        Some(NumberOrString::String(S!("OLS30302"))),
                        Some(EXTENSION_NAME.to_string()),
                        "Do not use dict unpacking to build your manifest".to_string(),
                        None,
                        None,
                    ), &session.current_noqa);
                    return res;
                }
            }
        }
        res
    }

    fn _create_diagnostic_for_manifest_key(&self, text: &str, code: String, range: &TextRange, severity: Option<DiagnosticSeverity>) -> Diagnostic {
        Diagnostic::new(
            Range::new(Position::new(range.start().to_u32(), 0), Position::new(range.end().to_u32(), 0)),
            severity,
            Some(NumberOrString::String(code)),
            Some(EXTENSION_NAME.to_string()),
            text.to_string(),
            None,
            None,
        )
    }

    /* ensure that all modules indicates in the module dependencies are well loaded.
    Returns list of diagnostics to publish in manifest file */
    fn _load_depends(symbol: &mut Symbol, session: &mut SessionInfo, odoo_addons: Rc<RefCell<Symbol>>) -> (Vec<Diagnostic>, Vec<OYarn>) {
        symbol.as_module_package_mut().all_depends.clear();
        let all_depends = symbol.as_module_package().depends.iter().map(|(depend, _)| depend.clone()).collect::<Vec<_>>();
        symbol.as_module_package_mut().all_depends.extend(all_depends);
        let mut diagnostics: Vec<Diagnostic> = vec![];
        let mut loaded: Vec<OYarn> = vec![];
        for (depend, range) in symbol.as_module_package().depends.clone().iter() {
            //TODO: raise an error on dependency cycle
            if !session.sync_odoo.modules.contains_key(depend) {
                let module = find_module(session, odoo_addons.clone(), depend);
                if module.is_none() {
                    symbol.get_entry().unwrap().borrow_mut().not_found_symbols.insert(symbol.weak_self().as_ref().unwrap().upgrade().expect("The symbol must be in the tree"));
                    symbol.not_found_paths_mut().push((BuildSteps::ARCH, vec![Sy!("odoo"), Sy!("addons"), depend.clone()]));
                    add_diagnostic(&mut diagnostics, Diagnostic::new(
                        FileMgr::textRange_to_temporary_Range(range),
                        Some(DiagnosticSeverity::ERROR),
                        Some(NumberOrString::String(S!("OLS30210"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("Module {} depends on {} which is not found. Please review your addons paths", symbol.name(), depend),
                        None,
                        None,
                    ), &session.current_noqa)
                } else {
                    loaded.push(depend.clone());
                    let module = module.unwrap();
                    let mut module = (*module).borrow_mut();
                    symbol.as_module_package_mut().all_depends.extend(module.as_module_package().all_depends.clone());
                    symbol.add_dependency(&mut module, BuildSteps::ARCH, BuildSteps::ARCH);
                }
            } else {
                let module = session.sync_odoo.modules.get(depend).unwrap().upgrade().unwrap();
                let mut module = (*module).borrow_mut();
                symbol.as_module_package_mut().all_depends.extend(module.as_module_package().all_depends.clone());
                symbol.add_dependency(&mut module, BuildSteps::ARCH, BuildSteps::ARCH)
            }
        }
        (diagnostics, loaded)
    }

    fn _load_data(_symbol: Rc<RefCell<Symbol>>, _odoo: &mut SyncOdoo) -> Vec<Diagnostic> {
        vec![]
    }

    fn _load_arch(symbol: Rc<RefCell<Symbol>>, session: &mut SessionInfo) -> Vec<Diagnostic> {
        let root_path = (*symbol).borrow().as_module_package().root_path.clone();
        let tests_path = PathBuf::from(root_path).join("tests");
        if tests_path.exists() {
            let rc_symbol = Symbol::create_from_path(session, &tests_path, symbol, false);
            if rc_symbol.is_some() && rc_symbol.as_ref().unwrap().borrow().typ() != SymType::NAMESPACE {
                let rc_symbol = rc_symbol.unwrap();
                session.sync_odoo.add_to_rebuild_arch(rc_symbol);
            }
        }
        vec![]
    }

    pub fn is_in_deps(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>, dir_name: &OYarn) -> bool {
        symbol.borrow().as_module_package().dir_name == *dir_name || symbol.borrow().as_module_package().all_depends.contains(dir_name)
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

    pub fn set_in_workspace(&mut self, in_workspace: bool) {
        self.in_workspace = in_workspace;
        if in_workspace {
            self.dependencies= vec![
                vec![ //ARCH
                    None //ARCH
                ],
                vec![ //ARCH_EVAL
                    None, //ARCH,
                    None, //ARCH_EVAL
                ],
                vec![
                    None, // ARCH
                    None, //ARCH_EVAL
                    None, //VALIDATIOn
                ]
            ];
            self.dependents = vec![
                vec![ //ARCH
                    None, //ARCH
                    None, //ARCH_EVAL
                    None, //VALIDATION
                ],
                vec![ //ARCH_EVAL
                    None, //ARCH_EVAL
                    None //VALIDATION
                ],
                vec![ //VALIDATION
                    None //VALIDATION
                ]
            ];
        }
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

    pub fn is_in_workspace(&self) -> bool {
        self.in_workspace
    }

}
