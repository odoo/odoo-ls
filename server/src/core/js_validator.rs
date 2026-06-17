use std::{
    cell::RefCell,
    rc::Rc,
};

use lsp_types::{Diagnostic, Position, Range};
use tracing::trace;

use crate::{Sy, constants::{DataType, DiagnosticLevel}, core::{diagnostics::{DiagnosticCode, create_diagnostic}}, features::xml_ast_utils::XmlAstUtils};
use crate::{
    constants::{BuildSteps, OYarn, DEBUG_STEPS},
    core::{
        file_mgr::FileInfo,
        odoo::SyncOdoo,
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

    fn get_file_info(&mut self, odoo: &mut SyncOdoo) -> Rc<RefCell<FileInfo>> {
        let path = &odoo.symbol_table[self.js_symbol].path;
        let file_info_rc = odoo.get_file_mgr().borrow().get_file_info(path).expect("File not found in cache").clone();
        file_info_rc
    }

    pub fn validate(&mut self, session: &mut SessionInfo) {
        if DEBUG_STEPS {
            let name = &session.st()[self.js_symbol].name;
            trace!("Validating JS File {}", name);
        }
        let mut diagnostics = vec![];
        let file_info = self.get_file_info(&mut session.sync_odoo);
        let mut file_info = file_info.borrow_mut();
        let ast = file_info.file_info_ast.borrow();
        let template_refs = ast.js_template_refs.clone();
        let component_descriptors = ast.js_component_descriptors.clone();
        drop(ast);

        // Populate template→class_name mapping
        for (_, template_name, class_name) in &template_refs {
            if let Some(cn) = class_name {
                session.sync_odoo.js_component_by_template.insert(template_name.clone(), cn.clone());
            }
        }

        // Populate component descriptors from this file
        for descriptor in component_descriptors {
            session.sync_odoo.component_descriptors.insert(descriptor.class_name.clone(), descriptor);
        }

        for template_ref in template_refs.iter() {
            if !XmlAstUtils::check_js_template_validity_for_key(session, &template_ref.1) {
                session.st_mut()[self.js_symbol].not_found_data_ids.insert(DataType::TEMPLATE(Sy!(template_ref.1.clone())), BuildSteps::VALIDATION);
                session.sync_odoo.get_main_entry().borrow_mut().not_found_data_ids.insert(self.js_symbol.into());
                let start = file_info.position_to_offset(template_ref.0.start.line, template_ref.0.start.character, session.sync_odoo.encoding);
                let end = file_info.position_to_offset(template_ref.0.end.line, template_ref.0.end.character, session.sync_odoo.encoding);
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS06000, &[]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(start as u32, 0), end: Position::new(end as u32, 0) },
                        ..diagnostic
                    });
                }
            }
        }
        for js_template in template_refs {
            if let Some(imps) = session.sync_odoo.js_templates.get_mut(&js_template.1) {
                for template_imp in imps.iter_valid(&session.sync_odoo.symbol_table) {
                    let xml_file = session.st().get_file(template_imp.into()).expect("Template should be in a file");
                    session.st_mut().add_dependency(self.js_symbol.into(), xml_file.into(), BuildSteps::VALIDATION, BuildSteps::ARCH_EVAL);
                }
            }
        }
        file_info.replace_diagnostics(DiagnosticLevel::JS_VALIDATION, diagnostics);
        file_info.publish_diagnostics(session);
    }
}
