use std::path::Path;

use lsp_types::Diagnostic;
use ruff_text_size::TextRange;
use tracing::warn;

use crate::{Sy, constants::{BuildSteps, DiagnosticSource, OYarn}, core::{build_scheduler::BuildScheduler, diagnostics::{DiagnosticCode, create_diagnostic}, file_mgr::FileMgr, import_resolver::create_module_from_name, symbols::{ModuleSymbol, SymbolTable, symbol_keys::{ModuleKey, NamespaceKey, SymbolKey}}}, threads::SessionInfo, utils::PathSanitizer};
use crate::core::symbols::symbol_table_impl::CreateError;



impl ModuleSymbol {

    pub fn load_module_arch(module_key: ModuleKey, session: &mut SessionInfo, odoo_addons: NamespaceKey) {
        let (diagnostics, _loaded) = ModuleSymbol::load_depends(module_key, session, odoo_addons);
        let module = &session.st()[module_key];
        if !module.loaded {
            ModuleSymbol::load_tests(module_key, session);
        }
        let module = &mut session.st_mut()[module_key];
        module.loaded = true;
        let manifest_path = Path::new(&module.root_path).join("__manifest__.py");
        let manifest_file_info = session.file_mgr().get_file_info(&manifest_path.sanitize_cow()).expect("file not found in cache");
        session.file_mgr_mut()[manifest_file_info].replace_diagnostics(DiagnosticSource::PY_ARCH, diagnostics);
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
        for (depend, range) in dependencies {
            match session.st()[odoo_addons].get_child(&depend) {
                Some(SymbolKey::Module(dependency)) => {
                    // Dependency already in modules
                    BuildScheduler::build_now(session, dependency, BuildSteps::ARCH);
                    if session.st()[dependency].all_depends.contains(&name)
                        && let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS04012, &[&depend]) {
                            diagnostics.push(Diagnostic {
                                range: FileMgr::textRange_to_temporary_Range(&range),
                                ..diagnostic_base.clone()
                            });
                        }
                    ModuleSymbol::extend_dependencies(session.st_mut(), symbol_key, dependency);
                },
                Some(other) => {
                    // odoo addons has a child symbol with the same name but not a module
                    warn!("dependency '{depend}' of '{name}' is {:?} at {:?}", other.typ(), session.st().paths(other));
                    Self::report_missing_dependency(session, &mut diagnostics, symbol_key, &name, &depend, &range);
                },
                None => match create_module_from_name(session, odoo_addons, &depend) {
                    Ok(dependency) => {
                        // Dependency just added to modules
                        loaded.push(depend);
                        ModuleSymbol::extend_dependencies(session.st_mut(), symbol_key, dependency);
                    },
                    Err(CreateError::Existing(_)) => unreachable!("name checked vacant on odoo_addons above"),
                    Err(CreateError::NothingOnDisk) => {
                        // Dependency not found nor created
                        Self::report_missing_dependency(session, &mut diagnostics, symbol_key, &name, &depend, &range);
                    }
                }
            }
        }
        (diagnostics, loaded)
    }

    fn report_missing_dependency(session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>, module: ModuleKey, name: &OYarn, depend: &OYarn, range: &TextRange) {
        let entry = session.st().get_entry(module);
        session.ep_mgr_mut()[entry].not_found_symbols.insert(module.into());
        session.st_mut()[module].not_found_paths.push((BuildSteps::ARCH, vec![Sy!("odoo"), Sy!("addons"), depend.clone()]));
        if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS04010, &[name, depend]) {
            diagnostics.push(Diagnostic {
                range: FileMgr::textRange_to_temporary_Range(range),
                ..diagnostic_base
            });
        }
    }

    fn extend_dependencies(symbol_table: &mut SymbolTable, symbol_key: ModuleKey, dependency: ModuleKey) {
        let dep_module = &symbol_table[dependency];
        let dep_module_dependencies = dep_module.all_depends.clone();
        let module = &mut symbol_table[symbol_key];
        module.all_depends.extend(dep_module_dependencies);
        symbol_table.add_dependency(symbol_key.into(), dependency.into(), BuildSteps::ARCH, BuildSteps::ARCH);
    }

    fn load_tests(module_key: ModuleKey, session: &mut SessionInfo) {
        let symbol_table = &session.sync_odoo.symbol_table;
        let module_symbol = &symbol_table[module_key];
        let root_path = module_symbol.root_path.clone();
        let tests_path = Path::new(&root_path).join("tests");
        if tests_path.exists() && !session.st()[module_key].module_symbols().contains_key("tests") {
            let symbol = SymbolTable::create_from_path(session, &tests_path, module_key.into(), false);
            if let Ok(sym) = symbol && !matches!(sym, SymbolKey::Namespace(_)) {
                BuildScheduler::queue(session, sym.unwrap_buildable_key());
            }
        }
    }
}
