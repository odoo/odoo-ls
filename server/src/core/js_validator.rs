use lsp_types::{Diagnostic, Position, Range};
use tracing::trace;

use crate::{Sy, constants::{BuildStatus, DiagnosticSource, MissingDataSource}, core::{diagnostics::{DiagnosticCode, create_diagnostic}, symbols::SymbolTable}, features::xml_ast_utils::XmlAstUtils};
use crate::{
    constants::{BuildSteps, OYarn, DEBUG_STEPS},
    core::{
        symbols::symbol_keys::JsFileKey,
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
        session.st_mut().set_build_status(self.js_symbol.into(), BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
        let mut diagnostics = vec![];
        let Some(file_info) = SymbolTable::get_file_info_for_validation(session, self.js_symbol.into()) else {
            session.st_mut().set_build_status(self.js_symbol.into(), BuildSteps::VALIDATION, BuildStatus::INVALID);
            return;
        };
        let mut file_info = file_info.borrow_mut();
        let file_info_ast = file_info.file_info_ast.borrow();
        let template_refs = file_info_ast.ast.as_js_ast().js_template_refs.clone();
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
        file_info.replace_diagnostics(DiagnosticSource::JS_VALIDATION, diagnostics);
        file_info.publish_diagnostics(session);
        session.st_mut().set_build_status(self.js_symbol.into(), BuildSteps::VALIDATION, BuildStatus::DONE);
    }
}
