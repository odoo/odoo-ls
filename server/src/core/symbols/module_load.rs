use super::XmlFileSymbol;
use crate::core::csv_arch_builder::CsvArchBuilder;
use crate::core::data_hooks;
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::file_mgr::{FileInfo, FileMgr};
use crate::core::import_resolver::create_module_from_name;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol_keys::{ModuleKey, NamespaceKey, SourceFileKey, SymbolKey, XmlId};
use crate::core::symbols::ModuleSymbol;
use crate::core::{symbols::storage::SymbolTable, xml_arch_builder::XmlArchBuilder};
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer;
use crate::weak_collections::WeakSet;
use crate::{constants::*, oyarn, Sy};
use lsp_types::{Diagnostic, DiagnosticTag, Position, Range};
use ruff_python_ast::{Expr, ExprStringLiteral, Stmt};
use ruff_text_size::Ranged;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::PathBuf;
use tracing::{error, info};

impl ModuleSymbol {

    pub fn load(mut self, session: &mut SessionInfo, dir_path: &PathBuf) -> Option<Self> {
        info!("building new module: {:?}", self.path);
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
        let diags = self.load_manifest(session, &manifest_file_info);
        if session.sync_odoo.modules.contains_key(&self.dir_name) {
            //TODO: handle multiple modules with the same name
        }
        manifest_file_info.replace_diagnostics(crate::constants::BuildSteps::SYNTAX, diags);
        manifest_file_info.publish_diagnostics(session);
        drop(manifest_file_info);
        info!("End building new module: {:?}", self.path);
        Some(self)
    }

    pub fn load_module_info(module_key: ModuleKey, session: &mut SessionInfo, odoo_addons: NamespaceKey) {
        let (mut diagnostics, _loaded) = ModuleSymbol::load_depends(module_key, session, odoo_addons);
        diagnostics.extend(ModuleSymbol::check_data(module_key, session));
        let module = &session.st()[module_key];
        if !module.loaded {
            diagnostics.append(&mut ModuleSymbol::load_arch(module_key, session));
        }
        let module = &mut session.st_mut()[module_key];
        module.loaded = true;
        let manifest_path = PathBuf::from(&module.root_path).join("__manifest__.py");
        let manifest_file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest_path.sanitize()).expect("file not found in cache").clone();
        let mut manifest_file_info = (*manifest_file_info).borrow_mut();
        manifest_file_info.replace_diagnostics(crate::constants::BuildSteps::ARCH, diagnostics);
    }

    /* Load manifest to identify the module characteristics.
    Returns list of od diagnostics to publish in manifest file. */
    fn load_manifest(&mut self, session: &mut SessionInfo, file_info: &FileInfo) -> Vec<Diagnostic> {
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
                            let key_str = key_literal.value.to_str();
                            if visited_keys.contains(key_str){
                            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04002, &[]) {
                                res.push(Diagnostic {
                                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                                    ..diagnostic
                                });
                            }
                            }
                            visited_keys.insert(key_str);
                            if key_str == "name" {
                                self.load_manifest_name(session, &mut res, key_literal, value);
                            } else if key_str == "depends" {
                                self.load_manifest_depends(session, &mut res, key_literal, value);
                            } else if key_str == "data" {
                                self.load_manifest_data(session, &mut res, key_literal, value);
                            } else if key_str == "assets" {
                                self.load_manifest_assets(session, &mut res, key_literal, value);
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

    fn load_manifest_name(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_string_literal_expr() {
            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04003, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        } else {
            self.module_name = oyarn!("{}", value.as_string_literal_expr().unwrap().value);
        }
    }

    fn load_manifest_depends(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_list_expr() {
            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04004, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        } else {
            for depend in value.as_list_expr().unwrap().elts.iter() {
                if !depend.is_string_literal_expr() {
                    if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04005, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(depend.range().start().to_u32(), 0), Position::new(depend.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                } else {
                    let depend_value = oyarn!("{}", depend.as_string_literal_expr().unwrap().value);
                    if depend_value == self.dir_name {
                        if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04006, &[]) {
                            diagnostics.push(Diagnostic {
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
    }

    fn load_manifest_data(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_list_expr() {
            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04007, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        } else {
            for data in value.as_list_expr().unwrap().elts.iter() {
                if !data.is_string_literal_expr() {
                    if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04008, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(data.range().start().to_u32(), 0), Position::new(data.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                } else {
                    self.data.push((data.as_string_literal_expr().unwrap().value.to_string(), data.range().clone()));
                }
            }
        }
    }

    fn load_manifest_assets(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_dict_expr() {
            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04013, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        } else {
            for data in value.as_dict_expr().unwrap().items.iter() {
                if data.key.is_none() {
                    if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04014, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(data.range().start().to_u32(), 0), Position::new(data.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                    continue;
                }
                if !data.key.as_ref().unwrap().is_string_literal_expr() {
                    if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04015, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(data.range().start().to_u32(), 0), Position::new(data.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                }
                if !data.value.is_list_expr() {
                    if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04016, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(data.range().start().to_u32(), 0), Position::new(data.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                    continue;
                }
                for item in data.value.as_list_expr().unwrap().iter() {
                    if item.is_string_literal_expr() {
                        self.assets.push((item.as_string_literal_expr().unwrap().value.to_string(), item.range().clone()));
                    } else if item.is_tuple_expr() {
                        if item.as_tuple_expr().unwrap().elts.len() == 0 {
                            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04018, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(item.range().start().to_u32(), 0), Position::new(item.range().end().to_u32(), 0)),
                                    ..diagnostic
                                });
                            }
                            continue;
                        }
                        let first_element = item.as_tuple_expr().unwrap().elts.first().unwrap();
                        if !first_element.is_string_literal_expr() {
                            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04018, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(item.range().start().to_u32(), 0), Position::new(item.range().end().to_u32(), 0)),
                                    ..diagnostic
                                });
                            }
                            continue;
                        }
                        let first_element_str = first_element.as_string_literal_expr().unwrap().value.to_str();
                        match first_element_str {
                            "before" | "after" | "replace" => {
                                if item.as_tuple_expr().unwrap().elts.len() != 3 {
                                    if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04020, &["3"]) {
                                        diagnostics.push(Diagnostic {
                                            range: Range::new(Position::new(item.range().start().to_u32(), 0), Position::new(item.range().end().to_u32(), 0)),
                                            ..diagnostic
                                        });
                                    }
                                    continue;
                                }
                                for value in item.as_tuple_expr().unwrap().elts.iter().skip(1) {
                                    if !value.is_string_literal_expr() {
                                        if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04018, &[]) {
                                            diagnostics.push(Diagnostic {
                                                range: Range::new(Position::new(value.range().start().to_u32(), 0), Position::new(value.range().end().to_u32(), 0)),
                                                ..diagnostic
                                            });
                                        }
                                        continue;
                                    }
                                }
                            },
                            "append" | "include" | "remove" | "prepend" => {
                                if item.as_tuple_expr().unwrap().elts.len() != 2 {
                                    if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04020, &["2"]) {
                                        diagnostics.push(Diagnostic {
                                            range: Range::new(Position::new(item.range().start().to_u32(), 0), Position::new(item.range().end().to_u32(), 0)),
                                            ..diagnostic
                                        });
                                    }
                                    continue;
                                }
                                for value in item.as_tuple_expr().unwrap().elts.iter().skip(1) {
                                    if !value.is_string_literal_expr() {
                                        if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04018, &[]) {
                                            diagnostics.push(Diagnostic {
                                                range: Range::new(Position::new(value.range().start().to_u32(), 0), Position::new(value.range().end().to_u32(), 0)),
                                                ..diagnostic
                                            });
                                        }
                                        continue;
                                    }
                                }
                            }
                            _ => {
                                if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04019, &[]) {
                                    diagnostics.push(Diagnostic {
                                        range: Range::new(Position::new(first_element.range().start().to_u32(), 0), Position::new(first_element.range().end().to_u32(), 0)),
                                        ..diagnostic
                                    });
                                }
                                continue;
                            }
                        }
                    } else {
                        if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS04017, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range::new(Position::new(item.range().start().to_u32(), 0), Position::new(item.range().end().to_u32(), 0)),
                                ..diagnostic
                            });
                        }
                    }
                }
            }
        }
    }

    /* ensure that all modules indicates in the module dependencies are well loaded.
    Returns list of diagnostics to publish in manifest file */
    fn load_depends(symbol_key: ModuleKey, session: &mut SessionInfo, odoo_addons: NamespaceKey) -> (Vec<Diagnostic>, Vec<OYarn>) {
        let module = &mut session.st_mut()[symbol_key];
        let name = module.name.clone();
        let all_depends = module.depends.iter().map(|(depend, _)| depend.clone()).collect::<Vec<_>>();
        module.all_depends.clear();
        module.all_depends.extend(all_depends);
        let mut diagnostics: Vec<Diagnostic> = vec![];
        let mut loaded: Vec<OYarn> = vec![];
        let dependencies = module.depends.clone();
        for (depend, range) in dependencies.iter() {
            if let Some(dependency) = session.sync_odoo.modules.get(depend).and_then(|m| m.upgrade(session.st())) {
                // Dependency already in modules
                SyncOdoo::build_now(session, dependency, BuildSteps::ARCH);
                if session.st()[dependency].all_depends.contains(&name) {
                    if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS04012, &[depend]) {
                        diagnostics.push(Diagnostic {
                            range: FileMgr::textRange_to_temporary_Range(range),
                            ..diagnostic_base.clone()
                        });
                    }
                }
                ModuleSymbol::extend_dependencies(session.st_mut(), symbol_key, dependency);
            } else if let Some(dependency) = create_module_from_name(session, odoo_addons, depend) {
                // Dependency just added to modules
                loaded.push(depend.clone());
                ModuleSymbol::extend_dependencies(session.st_mut(), symbol_key, dependency);
            } else {
                // Dependency not found nor created
                let entry = session.st().get_entry(symbol_key);
                entry.borrow_mut().not_found_symbols.insert(symbol_key.into());
                session.st_mut()[symbol_key].not_found_paths.push((BuildSteps::ARCH, vec![Sy!("odoo"), Sy!("addons"), depend.clone()]));
                if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS04010, &[&name, &depend]) {
                    diagnostics.push(Diagnostic {
                        range: FileMgr::textRange_to_temporary_Range(range),
                        ..diagnostic_base.clone()
                    });
                }
            }

        }
        (diagnostics, loaded)
    }

    fn extend_dependencies(symbol_table: &mut SymbolTable, symbol_key: ModuleKey, dependency: ModuleKey) {
        let dep_module = &symbol_table[dependency];
        let dep_module_dependencies = dep_module.all_depends.clone();
        let module = &mut symbol_table[symbol_key];
        module.all_depends.extend(dep_module_dependencies);
        symbol_table.add_dependency(symbol_key.into(), dependency.into(), BuildSteps::ARCH, BuildSteps::ARCH);
    }

    fn check_data(module_key: ModuleKey, session: &mut SessionInfo) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        let module = &session.st()[module_key];
        let module_path = module.path.clone();
        let data_paths = module.data.clone();
        for (data_url, data_range) in data_paths.iter() {
            //check if the file exists
            let path = PathBuf::from(module_path.clone()).join(data_url);
            if !path.exists() {
                session.st_mut()[module_key].not_found_data.insert(path.sanitize(), BuildSteps::ARCH);
                session.st_mut().get_entry(module_key).borrow_mut().not_found_symbols.insert(module_key.into());
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05049, &[&path.sanitize()]) {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(data_range.start().to_u32(), 0), Position::new(data_range.end().to_u32(), 0)),
                        ..diagnostic.clone()
                    });
                }
            } else if path.extension().map_or(true, |ext| !["xml", "csv", "sql"].contains(&ext.to_str().unwrap_or(""))) {
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

    pub fn validate_manifest(module_key: ModuleKey, session: &mut SessionInfo){
        let module = &session.st()[module_key];
        let module_path = module.path.clone();
        let data_paths = module.data.clone();
        let root_path = module.root_path.clone();
        let mut diagnostics = vec![];
        for (data_url, data_range) in data_paths.iter() {
            // validate csv file names, check that their models exist
            let path = PathBuf::from(module_path.clone()).join(data_url);
            if path.extension().unwrap_or_default() != "csv" || !path.exists(){
                continue;
            }
            let Some(model_name) = path.file_stem().and_then(OsStr::to_str).map(|n| Sy!(n.to_string())) else {
                continue;
            };
            let maybe_model = session.sync_odoo.models.get(&model_name).cloned();
            let model_exists = maybe_model.as_ref().map(|m| m.borrow_mut().has_symbols(session.st())).unwrap_or(false);
            if !model_exists {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05056, &[&model_name]) {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(data_range.start().to_u32(), 0), Position::new(data_range.end().to_u32(), 0)),
                        ..diagnostic.clone()
                    });
                }
                session.st_mut()[module_key].not_found_models.insert(model_name.clone(), BuildSteps::VALIDATION);
                session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(module_key.into());
            }
        }
        let manifest_path = PathBuf::from(root_path).join("__manifest__.py");
        let manifest_file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest_path.sanitize()).expect("file not found in cache").clone();
        let mut manifest_file_info = (*manifest_file_info).borrow_mut();
        manifest_file_info.replace_diagnostics(crate::constants::BuildSteps::VALIDATION, diagnostics);
        manifest_file_info.publish_diagnostics(session);
    }

    pub fn load_data(symbol_key: ModuleKey, session: &mut SessionInfo) {
        let module = &session.st()[symbol_key];
        let module_path = module.path.clone();
        let data_paths = module.data.clone();
        for (data_url, _data_range) in data_paths.iter() {
            //load data from file
            let path = PathBuf::from(module_path.clone()).join(data_url);
            let (_, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, &path.sanitize(), None, None, false); //create ast if not in cache
            let mut file_info = file_info.borrow_mut();
            let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
            if file_name.ends_with(".xml") {
                let xml_sym = session.st_mut().add_new_xml_file(symbol_key, &file_name, &path.sanitize());
                Self::on_data_file_load(session.st(), xml_sym.into());
                session.st_mut().add_dependency(symbol_key.into(), xml_sym.into(), BuildSteps::ARCH, BuildSteps::ARCH);
                if file_info.file_info_ast.borrow().text_document.as_ref().is_none() {
                    //TODO do we want to add a diagnostic here?
                    continue;
                }
                //That's a little bit crappy, but the SYNTAX step of XML files are done here, as lifetime of roXMLTree are not flexible enough to be separated from the Arch building
                let data = file_info.file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
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
                let csv_sym = session.st_mut().add_new_csv_file(symbol_key, &file_name, &path.sanitize());
                Self::on_data_file_load(session.st(), csv_sym.into());
                session.st_mut().add_dependency(symbol_key.into(), csv_sym.into(), BuildSteps::ARCH, BuildSteps::ARCH);
                if file_info.file_info_ast.borrow().text_document.as_ref().is_none() {
                    //TODO do we want to add a diagnostic here?
                    continue;
                }
                let data = file_info.file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
                let mut csv_builder = CsvArchBuilder::new();
                let diagnostics = csv_builder.load_csv(session, csv_sym, &data);
                file_info.replace_diagnostics(BuildSteps::SYNTAX, diagnostics);
                file_info.publish_diagnostics(session);
            } else if !file_name.ends_with(".sql") { // Do nothing for sql files for now, but also no error log
                error!("Unsupported data file type: {}", file_name);
            }
        }
    }

    fn on_data_file_load(symbol_table: &SymbolTable, data_file: SourceFileKey) {
        let path = symbol_table.path(data_file);
        let entry = symbol_table.get_entry(data_file);
        entry.borrow_mut().data_symbols.insert(path.to_string(), data_file.into());
    }

    pub fn on_data_file_unload(session: &mut SessionInfo, data_file: SourceFileKey) {
        let path = session.st().path(data_file);
        let entry = session.st().get_entry(data_file);
        entry.borrow_mut().data_symbols.remove(path);
        data_hooks::on_file_unload(session, data_file);
    }

    fn load_arch(module_key: ModuleKey, session: &mut SessionInfo) -> Vec<Diagnostic> {
        let symbol_table = &session.sync_odoo.symbol_table;
        let module_symbol = &symbol_table[module_key];
        let root_path = module_symbol.root_path.clone();
        let tests_path = PathBuf::from(root_path).join("tests");
        if tests_path.exists() {
            let symbol = SymbolTable::create_from_path(session, &tests_path, module_key.into(), false);
            if let Some(sym) = symbol && !matches!(sym, SymbolKey::Namespace(_)) {
                session.sync_odoo.add_to_rebuild_arch(sym);
            }
        }
        vec![]
    }

    pub fn is_in_deps(symbol_table: &SymbolTable, module_key: ModuleKey, dir_name: &OYarn) -> bool {
        let module = &symbol_table[module_key];
        module.dir_name == *dir_name || module.all_depends.contains(dir_name)
    }

    pub fn get_all_depends(&self) -> &HashSet<OYarn> {
        &self.all_depends
    }

    pub fn insert_xml_id(symbol_table: &mut SymbolTable, target: ModuleKey, xml_id: OYarn, xml_data: XmlId) {
        symbol_table[target].xml_ids.entry(xml_id).or_default().insert(xml_data);
    }

    //given an xml_id without "module." part, return all XmlData that declare it ("this_module.xml_id"), regardless of the module declaring it.
    pub fn get_xml_id(symbol_table: &SymbolTable, target: ModuleKey, xml_id: &str) -> Option<WeakSet<XmlId>> {
        let target_module = &symbol_table[target];
        return target_module.xml_ids.get(xml_id).cloned();
    }

}
