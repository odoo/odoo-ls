use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use lsp_types::{Diagnostic, Position, Range};
use tracing::{info, trace};

use crate::core::{model::Model, symbols::{storage::SymbolTable, symbol_keys::{XmlAssetKey, XmlDataKey, XmlDeleteKey, XmlMenuItemKey, XmlRecordKey, XmlTemplateKey}}};
use crate::{
    constants::{BuildSteps, OYarn, DEBUG_STEPS},
    core::{
        diagnostics::{create_diagnostic, DiagnosticCode},
        entry_point::{EntryPoint, EntryPointType},
        symbols::symbol_keys::{ModuleKey, SourceFileKey, SymbolKey, XmlFileKey},
    },
    threads::SessionInfo,
    utils::compare_semver,
    Sy,
};

pub struct XmlValidator {
    pub xml_symbol: XmlFileKey,
    pub is_in_main_ep: bool,
    module: ModuleKey,
    fields_cache: HashMap<OYarn, HashMap<OYarn, Vec<(SymbolKey, Option<OYarn>)>>>,
}

impl XmlValidator {

    pub fn new(entry: &Rc<RefCell<EntryPoint>>, symbol: XmlFileKey, symbol_table: &SymbolTable) -> Self {
        let is_in_main_ep = entry.borrow().typ == EntryPointType::MAIN || entry.borrow().typ == EntryPointType::ADDON;
        let module = symbol_table.find_module(symbol).unwrap();
        Self {
            xml_symbol: symbol,
            is_in_main_ep,
            module,
            fields_cache: HashMap::new(),
        }
    }

    pub fn validate(&mut self, session: &mut SessionInfo) {
        if DEBUG_STEPS {
            let name = &session.st()[self.xml_symbol].name;
            trace!("Validating XML File {}", name);
        }
        let mut dependencies = vec![];
        let mut model_dependencies = vec![];
        let mut missing_model_dependencies = HashSet::new();
        let mut diagnostics = vec![];
        for data_key in session.st()[self.xml_symbol].symbols().iter().cloned().collect::<Vec<_>>() {
            self.validate_data(session, data_key, &mut diagnostics, &mut dependencies, &mut model_dependencies, &mut missing_model_dependencies);
        }
        for dep in dependencies {
            session.st_mut().add_dependency(self.xml_symbol.into(), dep, BuildSteps::VALIDATION, BuildSteps::ARCH_EVAL);
        }
        for model in model_dependencies.iter() {
            session.st_mut().add_model_dependencies(self.xml_symbol.into(), &model);
        }
        if !missing_model_dependencies.is_empty() {
            session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(self.xml_symbol.into());
        }
        session.st_mut()[self.xml_symbol].not_found_models.extend(missing_model_dependencies.into_iter().map(|m| (m, BuildSteps::VALIDATION)));
        let Some(file_info) = SymbolTable::get_file_info_for_validation(session, self.xml_symbol.into()) else {
            return;
        };
        file_info.borrow_mut().replace_diagnostics(BuildSteps::VALIDATION, diagnostics);
        file_info.borrow_mut().publish_diagnostics(session);
    }

    fn validate_data(&mut self, session: &mut SessionInfo, data: XmlDataKey, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<SourceFileKey>, model_dependencies: &mut Vec<Rc<RefCell<Model>>>, missing_model_dependencies: &mut HashSet<OYarn>) {
        let Some(_) = session.st().get_file((data).into()) else {
            return;
        };
        match data {
            XmlDataKey::RECORD(xml_data_record) => self.validate_record(session, xml_data_record, diagnostics, dependencies, model_dependencies, missing_model_dependencies),
            XmlDataKey::MENUITEM(xml_data_menu_item) => self.validate_menu_item(session, xml_data_menu_item, diagnostics, dependencies, model_dependencies, missing_model_dependencies),
            XmlDataKey::TEMPLATE(xml_data_template) => self.validate_template(session, xml_data_template, diagnostics, dependencies, model_dependencies, missing_model_dependencies),
            XmlDataKey::DELETE(xml_data_delete) => self.validate_delete(session, xml_data_delete, diagnostics, dependencies, model_dependencies, missing_model_dependencies),
            XmlDataKey::ASSET(xml_data_asset) => self.validate_asset(session, xml_data_asset, diagnostics, dependencies, model_dependencies, missing_model_dependencies),
        }
    }
    fn validate_record(&mut self, session: &mut SessionInfo, xml_data_record: XmlRecordKey, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<SourceFileKey>, model_dependencies: &mut Vec<Rc<RefCell<Model>>>, missing_model_dependencies: &mut HashSet<OYarn>) {
        let xml_record = &session.st()[xml_data_record];
        let maybe_model = session.sync_odoo.models.get(&xml_record.model.0).cloned();
        let model_exists = maybe_model.as_ref().map(|m| m.borrow_mut().has_symbols(session.st())).unwrap_or(false);
        if !model_exists {
            missing_model_dependencies.insert(xml_record.model.0.clone());
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05056, &[&xml_record.model.0]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(xml_record.model.1.start.try_into().unwrap(), 0), end: Position::new(xml_record.model.1.end.try_into().unwrap(), 0) },
                    ..diagnostic
                });
            }
            info!("Model '{}' does not exist", xml_record.model.0);
            return;
        }
        let Some(model) = maybe_model else {unreachable!();};
        model_dependencies.push(model.clone());
        let main_symbols = model.borrow().get_main_symbols(session, Some(self.module));
        if main_symbols.is_empty() {
            missing_model_dependencies.insert(xml_record.model.0.clone());
            let module_name = session.st().name(self.module);
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05055, &[&xml_record.model.0, module_name]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(xml_record.model.1.start.try_into().unwrap(), 0), end: Position::new(xml_record.model.1.end.try_into().unwrap(), 0) },
                    ..diagnostic
                });
            }
            info!("Model '{}' has no symbols in module '{}'", xml_record.model.0, module_name);
            return;
        }
        for &main_sym in main_symbols.iter() {
            dependencies.push(session.st().get_file(main_sym.into()).unwrap());
        }
        let Some(&main_symbol) = main_symbols.get(0) else { return; };
        let model_name = xml_record.model.0.clone();
        if !self.fields_cache.contains_key(&model_name) {
            let all_fields = SymbolTable::all_fields(main_symbol.into(), session, Some(self.module));
            self.fields_cache.insert(model_name.clone(), all_fields);
        }
        let all_fields = self.fields_cache.get(&model_name).unwrap();
        self.validate_fields(session, xml_data_record, all_fields, diagnostics, missing_model_dependencies);
    }

    fn validate_fields(&self, session: &mut SessionInfo, xml_data_record: XmlRecordKey, all_fields: &HashMap<OYarn, Vec<(SymbolKey, Option<OYarn>)>>, diagnostics: &mut Vec<Diagnostic>, missing_model_dependencies: &mut HashSet<OYarn>) {
        let xml_record = &session.st()[xml_data_record];
        let fields = xml_record.fields().clone();
        //check each field in the record
        for (field_name, field_key) in &fields {
            let mut has_translation = false;
            if compare_semver(&session.sync_odoo.full_version, "18.2.0") >= Ordering::Equal {
                // Check for translation
                // obs: some language codes contain "@", e.g. "sr@latin", so we only split once
                if let Some((_fname, lang_code)) = field_name.split_once("@") {
                    has_translation = true;

                    // Validate language code
                    if !session.sync_odoo.check_language_and_track(lang_code, self.xml_symbol.into()) {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05068, &[lang_code]) {
                            let field = &session.st()[*field_key];
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(field.range.start().try_into().unwrap(), 0), end: Position::new(field.range.end().try_into().unwrap(), 0) },
                                ..diagnostic
                            });
                        }
                    }
                }
            }
            let field = &session.st()[*field_key];
            // Validate field ref_key
            if let Some((ref_key_val, ref_key_range)) = field.ref_key.as_ref(){
                let xml_id_split: Vec<_> = ref_key_val.split('.').collect();
                match xml_id_split.len() {
                    0 => {}, // Should not happen
                    1 => { // Local reference, check that it is not empty
                        let ref_xml_id = xml_id_split[0];
                        if ref_xml_id.is_empty() {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05039, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(ref_key_range.start().try_into().unwrap(), 0), end: Position::new(ref_key_range.end().try_into().unwrap(), 0) },
                                    ..diagnostic
                                });
                            }
                        }

                    },
                    2 => {
                        let module_name = xml_id_split[0];
                        if session.sync_odoo.modules.get(module_name).is_none() {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05003, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(ref_key_range.start().try_into().unwrap(), 0), end: Position::new(ref_key_range.end().try_into().unwrap(), 0) },
                                    ..diagnostic
                                });
                            }
                        }},
                    _ => { // >= 2
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05051, &[ref_key_val]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(ref_key_range.start().try_into().unwrap(), 0), end: Position::new(ref_key_range.end().try_into().unwrap(), 0) },
                                ..diagnostic
                            });
                        }
                    }
                }
            }
            //Check that the field belong to the model
            if all_fields.contains_key(field_name) {
                //Check specific attributes
                let (Some(field_text), Some(field_text_range)) = (field.text.as_ref(), field.text_range.as_ref()) else {
                    continue;
                };
                let record = &session.st()[xml_data_record];
                match (record.model.0.as_str(), field_name.as_str()) {
                    ("ir.ui.view", "model") | ("ir.actions.act_window", "res_model") => {
                        let model = session.sync_odoo.models.get(&Sy!(field_text.clone())).cloned();
                        let model_exists = model.as_ref().map(|m| m.borrow_mut().has_symbols(session.st())).unwrap_or(false);
                        if !model_exists {
                            missing_model_dependencies.insert(Sy!(field_text.clone()));
                        }
                        let mut main_sym = vec![];
                        if let Some(model) = model {
                            main_sym = model.borrow().get_main_symbols(session, Some(self.module));
                        }
                        if !model_exists {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05056, &[field_text, &record.model.0]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(field_text_range.start().try_into().unwrap(), 0), end: Position::new(field_text_range.end().try_into().unwrap(), 0) },
                                    ..diagnostic
                                });
                            }
                        }
                        if model_exists && main_sym.is_empty() {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05055, &[field_text, session.st().name(self.module)]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(field_text_range.start().try_into().unwrap(), 0), end: Position::new(field_text_range.end().try_into().unwrap(), 0) },
                                    ..diagnostic
                                });
                            }
                        }
                    },
                    _ => {}
                }
                //TODO check type
            } else {
                if has_translation {
                    continue;
                }
                let record = &session.st()[xml_data_record];
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05057, &[&field_name, &record.model.0]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(field.range.start().try_into().unwrap(), 0), end: Position::new(field.range.end().try_into().unwrap(), 0) },
                        ..diagnostic
                    });
                }
            }
        }
    }

    fn validate_menu_item(&self, _session: &mut SessionInfo, _xml_data_menu_item: XmlMenuItemKey, _diagnostics: &mut Vec<Diagnostic>, _dependencies: &mut Vec<SourceFileKey>, _model_dependencies: &mut Vec<Rc<RefCell<Model>>>, _missing_model_dependencies: &mut HashSet<OYarn>) {

    }

    fn validate_template(&self, _session: &mut SessionInfo, _xml_data_template: XmlTemplateKey, _diagnostics: &mut Vec<Diagnostic>, _dependencies: &mut Vec<SourceFileKey>, _model_dependencies: &mut Vec<Rc<RefCell<Model>>>, _missing_model_dependencies: &mut HashSet<OYarn>) {

    }

    fn validate_delete(&self, _session: &mut SessionInfo, _xml_data_delete: XmlDeleteKey, _diagnostics: &mut Vec<Diagnostic>, _dependencies: &mut Vec<SourceFileKey>, _model_dependencies: &mut Vec<Rc<RefCell<Model>>>, _missing_model_dependencies: &mut HashSet<OYarn>) {

    }

    fn validate_asset(&self, _session: &mut SessionInfo, _xml_data_asset: XmlAssetKey, _diagnostics: &mut Vec<Diagnostic>, _dependencies: &mut Vec<SourceFileKey>, _model_dependencies: &mut Vec<Rc<RefCell<Model>>>, _missing_model_dependencies: &mut HashSet<OYarn>) {

    }
}
