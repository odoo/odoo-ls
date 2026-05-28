use std::path::{Path, PathBuf};

use lsp_types::{Diagnostic, Position, Range};
use tracing::info;

use crate::{constants::{BuildSteps, DEBUG_STEPS}, core::{csv_arch_builder::CsvArchBuilder, data_hooks, diagnostics::{DiagnosticCode, create_diagnostic}, symbols::{ModuleSymbol, SymbolTable, XmlFileSymbol, symbol_keys::{ModuleKey, SourceFileKey}}, xml_arch_builder::XmlArchBuilder}, threads::SessionInfo, utils::PathSanitizer};



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
                    file_info.replace_diagnostics(BuildSteps::SYNTAX, vec![]);
                    let root = document.root_element();
                    let mut xml_builder = XmlArchBuilder::new(xml_sym);
                    xml_builder.load_arch(session, &mut file_info, &root);
                } else if !data.is_empty() {
                    let mut diagnostics = vec![];
                    XmlFileSymbol::build_syntax_diagnostics(&session, &mut diagnostics, &mut file_info, &document.unwrap_err());
                    file_info.replace_diagnostics(BuildSteps::ARCH_EVAL, diagnostics);
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
                file_info.replace_diagnostics(BuildSteps::SYNTAX, diagnostics);
                file_info.publish_diagnostics(session);
            }
        }
        let manifest_path = PathBuf::from(&module_path).join("__manifest__.py");
        let Some(manifest_file_info) = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest_path.sanitize_cow()) else {
            return;
        };
        let mut manifest_file_info = (*manifest_file_info).borrow_mut();
        manifest_file_info.replace_diagnostics(crate::constants::BuildSteps::SYNTAX, diagnostics);
        manifest_file_info.publish_diagnostics(session);
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
}
