use std::path::{Path, PathBuf};
use std::sync::Arc;

use lsp_types::{Diagnostic, Position, Range};
use tracing::info;

use crate::core::build_scheduler::BuildScheduler;
use crate::core::js_module_scope;
use crate::core::js_utils::read_module_header;
use crate::core::symbols::symbol_keys::JsFileKey;
use crate::{constants::{BuildStatus, BuildSteps, DEBUG_STEPS, DiagnosticSource}, core::{csv_arch_builder::CsvArchBuilder, data_hooks, diagnostics::{DiagnosticCode, create_diagnostic}, file_mgr::FileInfo, symbols::{ModuleSymbol, SymbolTable, XmlFileSymbol, symbol_keys::{BuildableSymbolKey, ModuleKey, SourceFileKey, XmlFileKey}}, xml_arch_builder::XmlArchBuilder}, threads::SessionInfo, utils::PathSanitizer};

const ASSET_FOLDERS: [&str; 3] = ["src", "tests", "lib"];



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
            if session.st()[symbol_key].data_file_symbols().contains_key(&path_string) {
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
            } else if path.extension().is_none_or(|ext| !["xml", "csv", "sql"].contains(&ext.to_str().unwrap_or(""))) {
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
                let xml_sym = session.st_mut().add_new_xml_file(symbol_key, &file_name, &path_string)
                    .expect("path should not already exist, as checked above");
                Self::on_data_file_load(session.st(), xml_sym.into());
                session.st_mut().add_dependency(symbol_key.into(), xml_sym.into(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
                ModuleSymbol::load_xml_arch(session, xml_sym, &mut file_info, false);
            } else if file_name.ends_with(".csv") {
                let csv_sym = session.st_mut().add_new_csv_file(symbol_key, &file_name, &path_string)
                    .expect("path should not already exist, as checked above");
                Self::on_data_file_load(session.st(), csv_sym.into());
                session.st_mut().add_dependency(symbol_key.into(), csv_sym.into(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
                let Some(data) = file_info.file_info_ast.borrow().text_document.as_ref().map(|td| td.contents().to_string()) else {
                    // File can be invalid (not valid UTF-8 and so text_document is empty)
                    continue;
                };
                let mut csv_builder = CsvArchBuilder::new();
                let diagnostics = csv_builder.load_csv(session, csv_sym, &data);
                file_info.replace_diagnostics(DiagnosticSource::CSV_SYNTAX, diagnostics);
                file_info.publish_diagnostics(session);
            }
        }
        let manifest_path = Path::new(&module_path).join("__manifest__.py");
        let Some(manifest_file_info) = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest_path.sanitize_cow()) else {
            return;
        };
        let mut manifest_file_info = (*manifest_file_info).borrow_mut();
        manifest_file_info.replace_diagnostics(DiagnosticSource::PY_ARCH_EVAL, diagnostics);
        manifest_file_info.publish_diagnostics(session);
    }

    pub fn on_data_file_load(symbol_table: &SymbolTable, data_file: SourceFileKey) {
        let path = symbol_table.path(data_file);
        let entry = symbol_table.get_entry(data_file);
        entry.borrow_mut().data_file_symbols.insert(path.to_string(), data_file.into());
    }

    pub fn on_data_file_unload(session: &mut SessionInfo, data_file: SourceFileKey) {
        let path = session.st().path(data_file);
        let entry = session.st().get_entry(data_file);
        entry.borrow_mut().data_file_symbols.remove(path);
        data_hooks::on_file_unload(session, data_file);
    }

    pub fn on_js_file_load(symbol_table: &SymbolTable, js_file: JsFileKey) {
        let path = symbol_table.path(js_file.into());
        let entry = symbol_table.get_entry(js_file);
        entry.borrow_mut().js_symbols.insert(path.to_string(), js_file.into());
    }

    pub fn on_js_file_unload(session: &mut SessionInfo, js_file: JsFileKey) {
        let path = session.st().path(js_file.into());
        let entry = session.st().get_entry(js_file);
        entry.borrow_mut().js_symbols.remove(path);
    }

    pub fn load_assets(module: ModuleKey, session: &mut SessionInfo) {
        if session.sync_odoo.config.is_javascript_disabled() {
            return;
        }
        let module_path = session.st()[module].path.clone();
        // Set for the duration of `build_modules` only: outside of it there are no
        // workers to share the walk with, and nothing to invalidate the memo.
        let cache = session.sync_odoo.pre_parse_cache().cloned();
        let files_to_imports = match &cache {
            Some(cache) => cache.resolve_assets(&module_path),
            None => Arc::new(Self::asset_paths(&module_path)),
        };
        //xml have to be loaded first
        Self::load_xml_assets(session, module, &files_to_imports);
        Self::load_js_assets(session, module, &files_to_imports);
        }
    }

    fn load_xml_assets(session: &mut SessionInfo, module: ModuleKey, files_to_imports: &[PathBuf]) {
        for file_path in files_to_imports.iter().filter(|p| p.extension().map(|ext| ext == "xml").unwrap_or(false)) {
            let file_path_str = file_path.sanitize_cow();
            let file_name = file_path.file_name().unwrap().to_str().unwrap().to_string();
            if session.st()[module].data_file_symbols().contains_key(file_path_str.as_ref()) { //already imported. can happen if the file is in multiple bundle or caught by multiple regex
                continue;
            }
            let xml_sym = session.st_mut().add_new_xml_file(module, &file_name, file_path_str.as_ref())
                .expect("path should not already exist, as checked above");
            Self::on_data_file_load(session.st(), xml_sym.into());
            session.st_mut().add_dependency(module.into(), xml_sym.into(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
            let (_, file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, file_path_str.as_ref(), None, None, false); //create ast if not in cache
            let mut file_info = file_info.borrow_mut();
            file_info.publish_diagnostics(session);
            if file_info.file_info_ast.borrow().text_document.as_ref().is_none() { // File can be invalid (not valid UTF-8 and so text_document is empty)
                continue;
            }
            ModuleSymbol::load_xml_arch(session, xml_sym, &mut file_info, true);
        }
    }

    fn load_xml_arch(session: &mut SessionInfo, xml_sym: XmlFileKey, file_info: &mut FileInfo, web_asset: bool) {
        let Some(data) = file_info.file_info_ast.borrow().text_document.as_ref().map(|td| td.contents().to_string()) else {
            // File can be invalid (not valid UTF-8 and so text_document is empty)
            return;
        };
        //That's a little bit crappy, but the SYNTAX step of XML files are done here, as lifetime of roXMLTree are not flexible enough to be separated from the Arch building
        let document = roxmltree::Document::parse(&data);
        if let Ok(document) = document {
            file_info.replace_diagnostics(DiagnosticSource::XML_SYNTAX, vec![]);
            let root = document.root_element();
            let mut xml_builder = XmlArchBuilder::new(xml_sym, web_asset);
            xml_builder.load_arch(session, file_info, &root);
        } else if !data.is_empty() {
            let mut diagnostics = vec![];
            XmlFileSymbol::build_syntax_diagnostics(session, &mut diagnostics, file_info, &document.unwrap_err());
            file_info.replace_diagnostics(DiagnosticSource::XML_SYNTAX, diagnostics);
            file_info.publish_diagnostics(session);
        }
    }

    fn load_js_assets(session: &mut SessionInfo, module: ModuleKey, files_to_imports: &[PathBuf]) {
        for file_path in files_to_imports.iter().filter(|p| p.extension().is_some_and(|ext| ext == "js")) {
            let file_path_str = file_path.sanitize_cow();
            if session.st()[module].js_symbols().contains_key(file_path_str.as_ref()) {
                continue;
            }
            let file_name = file_path.file_name().unwrap().to_str().unwrap().to_string();
            let js_key = session.st_mut().add_new_js_file(module.into(), &file_name, file_path_str.as_ref())
                .expect("path should not already exist, as checked above");
            session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, file_path_str.as_ref(), None, None, false); //create ast if not in cache
            // as the update_file_info built the arch, let's add the file to the validation queue.
            BuildScheduler::queue(session, js_key);
        }
    }

    /// Every asset of the module rooted at `module_path`.
    ///
    /// Pure but disk-bound: memoized by
    /// [`crate::core::pre_parser::PreParseCache::resolve_assets`] for the duration of
    /// the module build, and called by the pre-parse workers.
    pub(crate) fn asset_paths(module_path: &str) -> Vec<PathBuf> {
        let static_dir = Path::new(module_path).join("static");
        let mut results = vec![];
        for folder in ASSET_FOLDERS {
            collect_assets(&static_dir.join(folder), folder == "lib", &mut results);
        }
        results
    }
}

/// The asset folder `path` sits in — either as a descendant of it or as the folder itself.
fn asset_folder_of(module_path: &str, path: &str) -> Option<&'static str> {
    let relative = path.strip_prefix(module_path)?.strip_prefix("/static/")?;
    ASSET_FOLDERS.into_iter().find(|folder| {
        relative == *folder || relative.strip_prefix(folder).is_some_and(|rest| rest.starts_with('/'))
    })
}

fn collect_assets(dir: &Path, is_lib: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|typ| typ.is_dir()) {
            if entry.file_name() != "node_modules" {
                collect_assets(&path, is_lib, out);
            }
        } else if is_asset_file(&path, is_lib) {
            out.push(path);
        }
    }
}

/// In `static/lib` the `@odoo-module` header is what makes a JS file a module, and no XML
/// there is ever an asset.
fn is_asset_file(path: &Path, is_lib: bool) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("js") => !is_lib || read_module_header(path).is_some_and(|header| !header.ignore),
        Some("xml") => !is_lib,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_three_odoo_folders_hold_assets() {
        let module = "/addons/mod";
        assert_eq!(asset_folder_of(module, "/addons/mod/static/src/a.js"), Some("src"));
        assert_eq!(asset_folder_of(module, "/addons/mod/static/tests/deep/a.js"), Some("tests"));
        // A folder rename delivers the folder itself.
        assert_eq!(asset_folder_of(module, "/addons/mod/static/lib"), Some("lib"));
        // A prefix is not a segment.
        assert_eq!(asset_folder_of(module, "/addons/mod/static/source/a.js"), None);
        assert_eq!(asset_folder_of(module, "/addons/mod/static/description/icon.png"), None);
        assert_eq!(asset_folder_of(module, "/addons/mod/tools/a.js"), None);
        // `mod` must not claim `mod_extra`.
        assert_eq!(asset_folder_of(module, "/addons/mod_extra/static/src/a.js"), None);
    }
}
