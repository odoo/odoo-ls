use lsp_types::{Diagnostic, DiagnosticTag, Position, Range};
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};
use tracing::{error, info};
use weak_table::{PtrWeakHashSet, PtrWeakKeyHashMap};
use std::collections::{HashMap, HashSet};

use crate::core::csv_arch_builder::CsvArchBuilder;
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::xml_arch_builder::XmlArchBuilder;
use crate::core::xml_data::OdooData;
use crate::{constants::*, oyarn, Sy};
use crate::core::file_mgr::{FileInfo, FileMgr, NoqaInfo};
use crate::core::import_resolver::find_module;
use crate::core::model::Model;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol::Symbol;
use crate::core::symbols::symbol_mgr::SymbolMgr;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;
use crate::S;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::cell::RefCell;

use super::symbol_mgr::SectionRange;
use super::xml_file_symbol::XmlFileSymbol;


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
    data: Vec<(String, TextRange)>, // TODO
    pub module_symbols: HashMap<OYarn, Rc<RefCell<Symbol>>>,
    pub xml_id_locations: HashMap<OYarn, PtrWeakHashSet<Weak<RefCell<Symbol>>>>, //contains all xml_file_symbols that contains the xml_id. Needed because it can be in another module.
    pub xml_ids: HashMap<OYarn, Vec<OdooData>>, //used for dynamic XML_ID records, like ir.models. normal ids are in their XmlFile
    pub arch_status: BuildStatus,
    pub arch_eval_status: BuildStatus,
    pub validation_status: BuildStatus,
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub not_found_paths: Vec<(BuildSteps, Vec<OYarn>)>,
    pub not_found_data: HashMap<String, BuildSteps>,
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
    pub ext_symbols: HashMap<OYarn, PtrWeakHashSet<Weak<RefCell<Symbol>>>>,
    pub decl_ext_symbols: PtrWeakKeyHashMap<Weak<RefCell<Symbol>>, HashMap<OYarn, HashMap<u32, Vec<Rc<RefCell<Symbol>>>>>>,
    pub data_symbols: HashMap<String, Rc<RefCell<Symbol>>>,
}

impl ModuleSymbol {

    pub fn new(session: &mut SessionInfo, name: String, dir_path: &PathBuf, is_external: bool) -> Option<Self> {
        let mut module = ModuleSymbol {
            name: oyarn!("{}", name),
            path: dir_path.sanitize(),
            i_ext: S!(""),
            is_external,
            not_found_paths: vec![],
            not_found_data: HashMap::new(),
            in_workspace: false,
            root_path: dir_path.sanitize(),
            loaded: false,
            module_name: OYarn::from(""),
            xml_id_locations: HashMap::new(),
            xml_ids: HashMap::new(),
            dir_name: OYarn::from(""),
            depends: vec!((OYarn::from("base"), TextRange::default())),
            all_depends: HashSet::new(),
            data: Vec::new(),
            weak_self: None,
            parent: None,
            module_symbols: HashMap::new(),
            arch_status: BuildStatus::PENDING,
            arch_eval_status: BuildStatus::PENDING,
            validation_status: BuildStatus::PENDING,
            sections: vec![],
            symbols: HashMap::new(),
            ext_symbols: HashMap::new(),
            decl_ext_symbols: PtrWeakKeyHashMap::new(),
            model_dependencies: PtrWeakHashSet::new(),
            dependencies: vec![],
            dependents: vec![],
            processed_text_hash: 0,
            noqas: NoqaInfo::None,
            data_symbols: HashMap::new(),
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
        let (_, manifest_file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, manifest_path.sanitize().as_str(), None, None, false);
        let mut manifest_file_info = (*manifest_file_info).borrow_mut();
        if manifest_file_info.file_info_ast.borrow().indexed_module.is_none() {
            manifest_file_info.prepare_ast(session);
        }
        if manifest_file_info.file_info_ast.borrow().indexed_module.is_none() {
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

    pub fn load_module_info(symbol: &Rc<RefCell<Symbol>>, session: &mut SessionInfo, odoo_addons: Rc<RefCell<Symbol>>) {
        let (mut diagnostics, _loaded) = ModuleSymbol::_load_depends(symbol.clone(), session, odoo_addons);
        diagnostics.extend(ModuleSymbol::check_data(&symbol, session));
        if !symbol.borrow().as_module_package().loaded {
            diagnostics.append(&mut ModuleSymbol::_load_arch(symbol.clone(), session));
        }
        let mut _symbol = symbol.borrow_mut();
        let module = _symbol.as_module_package_mut();
        module.loaded = true;
        let manifest_path = PathBuf::from(module.root_path.clone()).join("__manifest__.py");
        let manifest_file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest_path.sanitize()).expect("file not found in cache").clone();
        let mut manifest_file_info = (*manifest_file_info).borrow_mut();
        manifest_file_info.replace_diagnostics(crate::constants::BuildSteps::ARCH, diagnostics);
        manifest_file_info.publish_diagnostics(session);
    }

    /* Load manifest to identify the module characteristics.
    Returns list of od diagnostics to publish in manifest file. */
    fn _load_manifest(&mut self, session: &mut SessionInfo, file_info: &FileInfo) -> Vec<Diagnostic> {
        let mut res = vec![];
        let file_info_ast = file_info.file_info_ast.borrow();
        let ast = file_info_ast.get_stmts().unwrap();
        if ast.len() != 1 || !matches!(ast.first(), Some(Stmt::Expr(expr)) if expr.value.is_dict_expr()) {
            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04001, &[]) {
                res.push(Diagnostic {
                    range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                    ..diagnostic
                });
            }
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
                            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04002, &[]) {
                                res.push(Diagnostic {
                                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                                    ..diagnostic
                                });
                            }
                            }
                            visited_keys.insert(key_str.clone());
                            if key_str == "name" {
                                if !value.is_string_literal_expr() {
                                    if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04003, &[]) {
                                        res.push(Diagnostic {
                                            range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                                            ..diagnostic
                                        });
                                    }
                                } else {
                                    self.module_name = oyarn!("{}", value.as_string_literal_expr().unwrap().value);
                                }
                            } else if key_str == "depends" {
                                if !value.is_list_expr() {
                                    if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04004, &[]) {
                                        res.push(Diagnostic {
                                            range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                                            ..diagnostic
                                        });
                                    }
                                } else {
                                    for depend in value.as_list_expr().unwrap().elts.iter() {
                                        if !depend.is_string_literal_expr() {
                                            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04005, &[]) {
                                                res.push(Diagnostic {
                                                    range: Range::new(Position::new(depend.range().start().to_u32(), 0), Position::new(depend.range().end().to_u32(), 0)),
                                                    ..diagnostic
                                                });
                                            }
                                        } else {
                                            let depend_value = oyarn!("{}", depend.as_string_literal_expr().unwrap().value);
                                            if depend_value == self.dir_name {
                                                if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04006, &[]) {
                                                    res.push(Diagnostic {
                                                        range: Range::new(Position::new(depend.range().start().to_u32(), 0), Position::new(depend.range().end().to_u32(), 0)),
                                                        ..diagnostic
                                                    });
                                                }
                                            } else {
                                                self.depends.push((depend_value, depend.range().clone()));
                                            }
                                        }
                                    }
                                }
                            } else if key_str == "data" {
                                if !value.is_list_expr() {
                                    if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04007, &[]) {
                                        res.push(Diagnostic {
                                            range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                                            ..diagnostic
                                        });
                                    }
                                } else {
                                    for data in value.as_list_expr().unwrap().elts.iter() {
                                        if !data.is_literal_expr() {
                                            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04008, &[]) {
                                                res.push(Diagnostic {
                                                    range: Range::new(Position::new(data.range().start().to_u32(), 0), Position::new(data.range().end().to_u32(), 0)),
                                                    ..diagnostic
                                                });
                                            }
                                        } else {
                                            self.data.push((data.as_string_literal_expr().unwrap().value.to_string(), data.range().clone()));
                                        }
                                    }
                                }
                            } else if key_str == "active" {
                                if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS03302, &[]) {
                                    res.push(Diagnostic {
                                        range: Range::new(Position::new(key_literal.range().start().to_u32(), 0), Position::new(key_literal.range().end().to_u32(), 0)),
                                        tags: Some(vec![DiagnosticTag::DEPRECATED]),
                                        ..diagnostic
                                    });
                                }
                            }
                        }
                        _ => {
                            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04009, &[]) {
                                    res.push(Diagnostic {
                                        range: Range::new(Position::new(key.range().start().to_u32(), 0), Position::new(key.range().end().to_u32(), 0)),
                                        ..diagnostic
                                    });
                            }
                        }
                    }
                },
                None => {
                    if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS04011, &[]) {
                        res.push(Diagnostic {
                            range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                            ..diagnostic_base.clone()
                        });
                    }
                    return res;
                }
            }
        }
        res
    }

    /* ensure that all modules indicates in the module dependencies are well loaded.
    Returns list of diagnostics to publish in manifest file */
    fn _load_depends(symbol_rc: Rc<RefCell<Symbol>>, session: &mut SessionInfo, odoo_addons: Rc<RefCell<Symbol>>) -> (Vec<Diagnostic>, Vec<OYarn>) {
        symbol_rc.borrow_mut().as_module_package_mut().all_depends.clear();
        let all_depends = symbol_rc.borrow_mut().as_module_package().depends.iter().map(|(depend, _)| depend.clone()).collect::<Vec<_>>();
        symbol_rc.borrow_mut().as_module_package_mut().all_depends.extend(all_depends);
        let mut diagnostics: Vec<Diagnostic> = vec![];
        let mut loaded: Vec<OYarn> = vec![];
        let dependencies = symbol_rc.borrow().as_module_package().depends.clone();
        for (depend, range) in dependencies.iter() {
            //TODO: raise an error on dependency cycle
            if !session.sync_odoo.modules.contains_key(depend) {
                let mut symbol = symbol_rc.borrow_mut();
                let module = find_module(session, odoo_addons.clone(), depend);
                if module.is_none() {
                    symbol.get_entry().unwrap().borrow_mut().not_found_symbols.insert(symbol.weak_self().as_ref().unwrap().upgrade().expect("The symbol must be in the tree"));
                    symbol.not_found_paths_mut().push((BuildSteps::ARCH, vec![Sy!("odoo"), Sy!("addons"), depend.clone()]));
                    if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS04010, &[&symbol.name(), &depend]) {
                        diagnostics.push(Diagnostic {
                            range: FileMgr::textRange_to_temporary_Range(range),
                            ..diagnostic_base.clone()
                        });
                    }
                } else {
                    loaded.push(depend.clone());
                    let module = module.unwrap();
                    let mut module = (*module).borrow_mut();
                    symbol.as_module_package_mut().all_depends.extend(module.as_module_package().all_depends.clone());
                    symbol.add_dependency(&mut module, BuildSteps::ARCH, BuildSteps::ARCH);
                }
            } else {
                let module = session.sync_odoo.modules.get(depend).unwrap().upgrade().unwrap();
                SyncOdoo::build_now(session, &module, BuildSteps::ARCH);
                let name = symbol_rc.borrow().name().clone();
                if module.borrow().as_module_package().all_depends.contains(&name){
                    if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS04012, &[depend]) {
                        diagnostics.push(Diagnostic {
                            range: FileMgr::textRange_to_temporary_Range(range),
                            ..diagnostic_base.clone()
                        });
                    }
                }
                let mut module = (*module).borrow_mut();
                symbol_rc.borrow_mut().as_module_package_mut().all_depends.extend(module.as_module_package().all_depends.clone());
                symbol_rc.borrow_mut().add_dependency(&mut module, BuildSteps::ARCH, BuildSteps::ARCH)
            }
        }
        (diagnostics, loaded)
    }

    fn check_data(symbol: &Rc<RefCell<Symbol>>, session: &mut SessionInfo) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        let data_paths = symbol.borrow().as_module_package().data.clone();
        for (data_url, data_range) in data_paths.iter() {
            //check if the file exists
            let path = PathBuf::from(symbol.borrow().paths()[0].clone()).join(data_url);
            if !path.exists() {
                symbol.borrow_mut().as_module_package_mut().not_found_data.insert(path.sanitize(), BuildSteps::ARCH);
                symbol.borrow().get_entry().unwrap().borrow_mut().not_found_symbols.insert(symbol.clone());
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05049, &[&path.sanitize()]) {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(data_range.start().to_u32(), 0), Position::new(data_range.end().to_u32(), 0)),
                        ..diagnostic.clone()
                    });
                }
            } else if path.extension().map_or(true, |ext| !["xml", "csv"].contains(&ext.to_str().unwrap_or(""))) {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05050, &[&path.sanitize()]) {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(data_range.start().to_u32(), 0), Position::new(data_range.end().to_u32(), 0)),
                        ..diagnostic.clone()
                    });
                }
            }
        }
        diagnostics
    }

    pub fn load_data(symbol: &Rc<RefCell<Symbol>>, session: &mut SessionInfo) {
        let data_paths = symbol.borrow().as_module_package().data.clone();
        for (data_url, _data_range) in data_paths.iter() {
            //load data from file
            let path = PathBuf::from(symbol.borrow().paths()[0].clone()).join(data_url);
            let (_, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, &path.sanitize(), None, None, false); //create ast if not in cache
            let mut file_info = file_info.borrow_mut();
            let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
            if file_name.ends_with(".xml") {
                let xml_sym = symbol.borrow_mut().add_new_xml_file(session, &file_name, &path.sanitize());
                symbol.borrow_mut().add_dependency(&mut xml_sym.borrow_mut(), BuildSteps::ARCH, BuildSteps::ARCH);
                if file_info.file_info_ast.borrow().text_rope.as_ref().is_none() {
                    //TODO do we want to add a diagnostic here?
                    continue;
                }
                //That's a little bit crappy, but the SYNTAX step of XML files are done here, as lifetime of roXMLTree are not flexible enough to be separated from the Arch building
                let data = file_info.file_info_ast.borrow().text_rope.as_ref().unwrap().to_string();
                let document = roxmltree::Document::parse(&data);
                if let Ok(document) = document {
                    file_info.replace_diagnostics(BuildSteps::SYNTAX, vec![]);
                    let root = document.root_element();
                    let mut xml_builder = XmlArchBuilder::new(xml_sym);
                    xml_builder.load_arch(session, &mut file_info, &root);
                } else if data.len() > 0 {
                    let mut diagnostics = vec![];
                    XmlFileSymbol::build_syntax_diagnostics(&session, &mut diagnostics, &mut file_info, &document.unwrap_err());
                    file_info.replace_diagnostics(BuildSteps::SYNTAX, diagnostics);
                    file_info.publish_diagnostics(session);
                    continue
                }
            } else if file_name.ends_with(".csv") {
                let csv_sym = symbol.borrow_mut().add_new_csv_file(session, &file_name, &path.sanitize());
                symbol.borrow_mut().add_dependency(&mut csv_sym.borrow_mut(), BuildSteps::ARCH, BuildSteps::ARCH);
                if file_info.file_info_ast.borrow().text_rope.as_ref().is_none() {
                    //TODO do we want to add a diagnostic here?
                    continue;
                }
                let data = file_info.file_info_ast.borrow().text_rope.as_ref().unwrap().to_string();
                let mut csv_builder = CsvArchBuilder::new();
                csv_builder.load_csv(session, csv_sym, &data);
            } else {
                error!("Unsupported data file type: {}", file_name);
            }
        }
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

    pub fn is_in_deps(_session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>, dir_name: &OYarn) -> bool {
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

    pub fn get_ext_symbol(&self, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
        let mut result = vec![];
        if let Some(owners) = self.ext_symbols.get(name) {
            for owner in owners.iter() {
                let owner = owner.borrow();
                result.extend(owner.get_decl_ext_symbol(&self.weak_self.as_ref().unwrap().upgrade().unwrap(), name));
            }
        }
        result
    }

    pub fn get_decl_ext_symbol(&self, symbol: &Rc<RefCell<Symbol>>, name: &OYarn) -> Vec<Rc<RefCell<Symbol>>> {
        let mut result = vec![];
        if let Some(object_decl_symbols) = self.decl_ext_symbols.get(symbol) {
            if let Some(symbols) = object_decl_symbols.get(name) {
                for end_symbols in symbols.values() {
                    //TODO actually we don't take position into account, but can we really?
                    result.extend(end_symbols.iter().map(|s| s.clone()));
                }
            }
        }
        result
    }

    pub fn this_and_dependencies(&self, session: &mut SessionInfo) -> PtrWeakHashSet<Weak<RefCell<Symbol>>> {
        let mut result = PtrWeakHashSet::new();
        result.insert(self.weak_self.as_ref().unwrap().upgrade().unwrap());
        for dep in self.depends.iter() {
            if let Some(module) = session.sync_odoo.modules.get(&dep.0) {
                if let Some(module) = module.upgrade() {
                    result.insert(module);
                }
            }
        }
        result
    }

    //given an xml_id without "module." part, return all XmlData that declare it ("this_module.xml_id"), regardless of the module declaring it.
    //For example, stock could create an xml_id called "account.my_xml_id", and so be returned by this function called on "account" module with xml_id "my_xml_id"
    pub fn get_xml_id(&self, xml_id: &OYarn) -> Vec<OdooData> {
        let mut res = vec![];
        if let Some(xml_file_set) = self.xml_id_locations.get(xml_id) {
            for xml_file in xml_file_set.iter() {
                if let Some(xml_data) = xml_file.borrow().get_xml_id(xml_id) {
                    res.extend(xml_data.iter().cloned());
                }
            }
        }
        res
    }

}
