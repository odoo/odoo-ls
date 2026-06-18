use std::path::{Path, PathBuf};

use itertools::Itertools;
use lsp_types::{Diagnostic, OneOf, Position, Range};
use tracing::info;

use crate::{constants::{BuildSteps, DEBUG_STEPS, DiagnosticLevel}, core::{csv_arch_builder::CsvArchBuilder, data_hooks, diagnostics::{DiagnosticCode, create_diagnostic}, symbols::{ModuleSymbol, SymbolTable, XmlFileSymbol, symbol_keys::{ModuleKey, SourceFileKey}}, xml_arch_builder::XmlArchBuilder}, threads::SessionInfo, utils::PathSanitizer};



impl ModuleSymbol {

    pub fn load_data(symbol_key: ModuleKey, session: &mut SessionInfo) {
        let mut diagnostics = vec![];
        let module = &session.st()[symbol_key];
        let module_path = module.path.clone();
        if DEBUG_STEPS {
            info!("ARCH_EVAL  - MANIFEST: {}", module_path);
        }
        let data_paths = module.data.clone();
        for (data_url, data_range) in data_paths.iter() {
            let path = Path::new(&module_path).join(data_url);
            let file_name = path.file_name().unwrap().to_str().unwrap().to_string();
            let path_string = path.sanitize();
            //check if already exists
            if session.st()[symbol_key].data_symbols().contains_key(&path_string) {
                continue;
            }
            //load data from file
            if !path.exists() {
                session.st_mut()[symbol_key].not_found_data.insert(path_string.clone(), BuildSteps::ARCH_EVAL);
                session.st_mut().get_entry(symbol_key).borrow_mut().not_found_symbols.insert(symbol_key.into());
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05049, &[&path_string]) {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(data_range.start().to_u32(), 0), Position::new(data_range.end().to_u32(), 0)),
                        ..diagnostic.clone()
                    });
                }
                continue;
            } else if path.extension().map_or(true, |ext| !["xml", "csv", "sql"].contains(&ext.to_str().unwrap_or(""))) {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05050, &[&path_string]) {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(data_range.start().to_u32(), 0), Position::new(data_range.end().to_u32(), 0)),
                        ..diagnostic.clone()
                    });
                }
                continue;
            }
            let (_, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, &path_string, None, None, false); //create ast if not in cache
            let mut file_info = file_info.borrow_mut();
            if file_name.ends_with(".xml") {
                let xml_sym = session.st_mut().add_new_xml_file(symbol_key, &file_name, &path_string);
                Self::on_data_file_load(session.st(), xml_sym.into());
                session.st_mut().add_dependency(symbol_key.into(), xml_sym.into(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
                let data = match file_info.file_info_ast.borrow().text_document.as_ref() {
                    Some(text_document) => text_document.contents().to_string(),
                    None => {
                        //TODO do we want to add a diagnostic here?
                        continue;
                    }
                };
                let document = roxmltree::Document::parse(&data);
                if let Ok(document) = document {
                    file_info.replace_diagnostics(DiagnosticLevel::PY_SYNTAX, vec![]);
                    let root = document.root_element();
                    let mut xml_builder = XmlArchBuilder::new(xml_sym, false);
                    xml_builder.load_arch(session, &mut file_info, &root);
                } else if !data.is_empty() {
                    let mut diagnostics = vec![];
                    XmlFileSymbol::build_syntax_diagnostics(&session, &mut diagnostics, &mut file_info, &document.unwrap_err());
                    file_info.replace_diagnostics(DiagnosticLevel::PY_SYNTAX, diagnostics);
                    file_info.publish_diagnostics(session);
                    continue
                }
            } else if file_name.ends_with(".csv") {
                let csv_sym = session.st_mut().add_new_csv_file(symbol_key, &file_name, &path_string);
                Self::on_data_file_load(session.st(), csv_sym.into());
                session.st_mut().add_dependency(symbol_key.into(), csv_sym.into(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
                if file_info.file_info_ast.borrow().text_document.as_ref().is_none() {
                    //TODO do we want to add a diagnostic here?
                    continue;
                }
                let data = file_info.file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
                let mut csv_builder = CsvArchBuilder::new();
                let diagnostics = csv_builder.load_csv(session, csv_sym, &data);
                file_info.replace_diagnostics(DiagnosticLevel::PY_SYNTAX, diagnostics);
                file_info.publish_diagnostics(session);
            }
        }
        let manifest_path = PathBuf::from(&module_path).join("__manifest__.py");
        let Some(manifest_file_info) = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest_path.sanitize_cow()) else {
            return;
        };
        let mut manifest_file_info = (*manifest_file_info).borrow_mut();
        manifest_file_info.replace_diagnostics(DiagnosticLevel::PY_SYNTAX, diagnostics);
        manifest_file_info.publish_diagnostics(session);
    }

    pub (super) fn on_data_file_load(symbol_table: &SymbolTable, data_file: SourceFileKey) {
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

    pub fn on_js_file_unload(session: &mut SessionInfo, js_file: SourceFileKey) {
        let path = session.st().path(js_file);
        let entry = session.st().get_entry(js_file);
        entry.borrow_mut().js_symbols.remove(path);
    }

    pub(crate) fn load_assets(module: ModuleKey, session: &mut SessionInfo) {
        let asset_paths = session.st()[module].assets.clone();
        for (data_url, _data_range) in asset_paths.iter() {
            //An asset can be from another module. Extract its name
            let mut data_url_splitted = data_url.splitn(2, '/');
            let data_module_name = data_url_splitted.next().unwrap();
            let Some(data_local_url) = data_url_splitted.next() else {
                continue;
            };
            let Some(data_module) = session.sync_odoo.modules.get(data_module_name) else {
                continue;
            };
            let module = data_module.upgrade(session.st()).unwrap();
            let files_to_imports = ModuleSymbol::assets_path_resolver(session, module, data_local_url);
            for file_path in files_to_imports.iter().sorted_by(|a, b| Ord::cmp(&b.split(".").last().unwrap(), &a.split(".").last().unwrap())) { //ensure deterministic order, with xml files before js files
                if file_path.ends_with(".js") {
                    if session.st()[module].js_symbols().contains_key(file_path) {
                        continue;
                    }
                    let file_name = PathBuf::from(file_path).file_name().unwrap().to_str().unwrap().to_string();
                    let js_key = session.st_mut().add_new_js_file(OneOf::Left(module), &file_name, &file_path);
                    session.st_mut().add_dependency(module.into(), js_key.into(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
                    session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, &file_path, None, None, false); //create ast if not in cache
                    session.sync_odoo.add_to_validations(js_key);
                } else if file_path.ends_with(".xml") {
                    let file_name = PathBuf::from(file_path).file_name().unwrap().to_str().unwrap().to_string();
                    let path = PathBuf::from(file_path).sanitize();
                    if session.st()[module].data_symbols().contains_key(&path) { //already imported. can happen if the file is in multiple bundle or caught by multiple regex
                        continue;
                    }
                    let xml_sym = session.st_mut().add_new_xml_file(module, &file_name, &path);
                    Self::on_data_file_load(session.st(), xml_sym.into());
                    session.st_mut().add_dependency(module.into(), xml_sym.into(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
                    let (_, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, &file_path, None, None, false); //create ast if not in cache
                    let mut file_info = file_info.borrow_mut();
                    file_info.publish_diagnostics(session);
                    if file_info.file_info_ast.borrow().text_document.as_ref().is_none() {
                        //TODO do we want to add a diagnostic here?
                        continue;
                    }
                    //That's a little bit crappy, but the SYNTAX step of XML files are done here, as lifetime of roXMLTree are not flexible enough to be separated from the Arch building
                    let data = file_info.file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
                    let document = roxmltree::Document::parse(&data);
                    if let Ok(document) = document {
                        file_info.replace_diagnostics(DiagnosticLevel::XML_SYNTAX, vec![]);
                        let root = document.root_element();
                        let mut xml_builder = XmlArchBuilder::new(xml_sym, true);
                        xml_builder.load_arch(session, &mut file_info, &root);
                    } else if data.len() > 0 {
                        let mut diagnostics = vec![];
                        XmlFileSymbol::build_syntax_diagnostics(&session, &mut diagnostics, &mut file_info, &document.unwrap_err());
                        file_info.replace_diagnostics(DiagnosticLevel::XML_SYNTAX, diagnostics);
                        file_info.publish_diagnostics(session);
                        continue
                    }
                }
            }
        }
    }

    /// Matches a single path component against a glob pattern (e.g. "*.js", "test_*")
    /// Only handles single-level wildcards — no path separators.
    fn matches_single_glob(pattern: &str, name: &str) -> bool {
        let parts: Vec<&str> = pattern.split('*').collect();
        match parts.as_slice() {
            [only] => *only == name,
            [prefix, suffix] => name.starts_with(prefix) && name.ends_with(suffix)
                                && name.len() >= prefix.len() + suffix.len(),
            // e.g. "a*b*c" — walk through parts sequentially
            _ => {
                let mut remaining = name;
                for (i, part) in parts.iter().enumerate() {
                    if i == 0 {
                        if !remaining.starts_with(part) { return false; }
                        remaining = &remaining[part.len()..];
                    } else if i == parts.len() - 1 {
                        if !remaining.ends_with(part) { return false; }
                    } else {
                        match remaining.find(part) {
                            Some(pos) => remaining = &remaining[pos + part.len()..],
                            None => return false,
                        }
                    }
                }
                true
            }
        }
    }

    /// Recursively collects all descendants of a directory (files and subdirs).
    /// The directory itself is included (** matches zero levels too).
    fn collect_recursive(path: &PathBuf, results: &mut Vec<PathBuf>) {
        results.push(path.clone()); // ** can match zero segments
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let child = entry.path();
                if child.is_dir() {
                    ModuleSymbol::collect_recursive(&child, results); // recurse into subdirs
                } else {
                    results.push(child); // include files too
                }
            }
        }
    }

    fn assets_path_resolver(session: &mut SessionInfo, module: ModuleKey, data_local_url: &str) -> Vec<String> {
        let mut results = vec![PathBuf::from(session.st().path(module.into()))];

        for component in PathBuf::from(data_local_url).components() {
            let std::path::Component::Normal(os_str) = component else { continue };
            let segment = os_str.to_str().unwrap();
            let mut new_results = vec![];

            match segment {
                // ** → expand every current path to itself + all descendants
                "**" => {
                    for path in &results {
                        if path.is_dir() {
                            ModuleSymbol::collect_recursive(path, &mut new_results);
                        } else {
                            new_results.push(path.clone());
                        }
                    }
                }

                // * or *.js etc. → list direct children and filter by pattern
                pattern if pattern.contains('*') => {
                    for path in &results {
                        if let Ok(entries) = std::fs::read_dir(path) {
                            for entry in entries.flatten() {
                                let name = entry.file_name();
                                let name_str = name.to_str().unwrap();
                                if ModuleSymbol::matches_single_glob(pattern, name_str) {
                                    new_results.push(entry.path());
                                }
                            }
                        }
                    }
                }

                // Exact segment → just join and check existence
                exact => {
                    for path in &results {
                        let candidate = path.join(exact);
                        if candidate.exists() {
                            new_results.push(candidate);
                        }
                    }
                }
            }

            results = new_results;
        }

        results
            .into_iter()
            .map(|p| p.sanitize())
            .collect()
    }
}
