use lsp_types::{Diagnostic, Position, Range};
use tracing::trace;

use crate::{Sy, constants::{BuildStatus, DiagnosticSource, MissingDataSource}, core::{diagnostics::{DiagnosticCode, create_diagnostic}, file_mgr::{FileMgr, JsImport}}, features::xml_ast_utils::XmlAstUtils};
use crate::{
    constants::{BuildSteps, OYarn, DEBUG_STEPS},
    core::{
        symbols::{ModuleSymbol, symbol_keys::JsFileKey},
    },
    threads::SessionInfo,
};

pub struct JsValidator {
    pub js_symbol: JsFileKey,
}

impl JsValidator {

    pub fn new(symbol: JsFileKey) -> Self {
        Self {
            js_symbol: symbol,
        }
    }

    pub fn validate(&mut self, session: &mut SessionInfo) {
        if DEBUG_STEPS {
            let name = &session.st()[self.js_symbol].name;
            trace!("Validating JS File {}", name);
        }
        if !session.st().ready_for_step(self.js_symbol.into(), BuildSteps::VALIDATION) {
            return;
        }
        session.st_mut().set_build_status(self.js_symbol.into(), BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
        let mut diagnostics = vec![];
        let (file_info, loaded) = FileMgr::get_or_recreate_file_info(session, self.js_symbol.into());
        if !loaded {
            session.st_mut().set_build_status(self.js_symbol.into(), BuildSteps::VALIDATION, BuildStatus::INVALID);
            return;
        }
        if !file_info.borrow().file_info_ast.borrow().ast.is_built() {
            // JS Symbols do not go through the arch step,
            // Custom JS files may not have been built yet, so we need to build the AST first before validating
            file_info.borrow_mut().prepare_ast(session);
            if !file_info.borrow().file_info_ast.borrow().ast.is_built() {
                // Still not built, something went wrong, we cannot validate this file
                session.st_mut().set_build_status(self.js_symbol.into(), BuildSteps::VALIDATION, BuildStatus::INVALID);
                return;
            }
        }
        let mut file_info = file_info.borrow_mut();
        let file_info_ast = file_info.file_info_ast.borrow();
        let template_refs = file_info_ast.ast.as_js_ast().js_template_refs.clone();
        let js_imports = file_info_ast.ast.as_js_ast().js_imports.clone();
        drop(file_info_ast);

        if session.sync_odoo.symbol_table.get_entry(self.js_symbol).borrow().is_main() {
            for template_ref in template_refs.iter() {
                if !XmlAstUtils::ensure_js_template_validity(session, &template_ref.t_name) {
                    session.st_mut()[self.js_symbol].not_found_data_ids.insert(MissingDataSource::TEMPLATE(Sy!(template_ref.t_name.clone())), BuildSteps::VALIDATION);
                        session.sync_odoo.get_main_entry().borrow_mut().not_found_data_ids.insert(self.js_symbol.into());
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS06000, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(template_ref.range.start().to_u32(), 0), end: Position::new(template_ref.range.end().to_u32(), 0) },
                            ..diagnostic
                        });
                    }
                } else if let Some(imps) = session.sync_odoo.js_templates.get_mut(&template_ref.t_name) {
                    for template_imp in imps.iter_valid(&session.sync_odoo.symbol_table) {
                        let xml_file = session.st().get_file(template_imp.into()).expect("Template should be in a file");
                        session.st_mut().add_dependency(self.js_symbol.into(), xml_file, BuildSteps::VALIDATION, BuildSteps::ARCH_EVAL);
                    }
                }
            }
        }
        self.validate_import_scope(session, &js_imports, &mut diagnostics);
        file_info.replace_diagnostics(DiagnosticSource::JS_VALIDATION, diagnostics);
        file_info.publish_diagnostics(session);
        session.st_mut().set_build_status(self.js_symbol.into(), BuildSteps::VALIDATION, BuildStatus::DONE);
    }

    /// Whether `mod` in "import from @mod/some/path" is part of the current module's dependencies
    fn validate_import_scope(&self, session: &SessionInfo, imports: &[JsImport], diagnostics: &mut Vec<Diagnostic>) {
        let Some(module) = session.st().find_module(self.js_symbol) else {
            return;
        };
        for import in imports.iter() {
            // `@mail/core/store` -> `mail`.
            let target = import.specifier.strip_prefix('@')
                .and_then(|rest| rest.split_once('/'))
                .and_then(|(name, _sub_path)| session.sync_odoo.modules.get(name))
                .and_then(|module| module.upgrade(session.st()));
            let Some(target) = target else {
                continue;
            };
            let target_name = session.st()[target].dir_name.clone();
            if ModuleSymbol::is_in_deps(session.st(), module, &target_name) {
                continue;
            }
            if let Some(diagnostic) = create_diagnostic(
                session,
                DiagnosticCode::OLS06001,
                &[&target_name, &session.st()[module].dir_name],
            ) {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position::new(import.range.start().into(), 0),
                        end: Position::new(import.range.end().into(), 0),
                    },
                    ..diagnostic
                });
            }
        }
    }
}
