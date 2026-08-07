use std::{
    cell::RefCell,
    rc::Rc,
};
use crate::{constants::BuildStatus, core::{file_mgr::FileMgr, odoo::SyncOdoo, symbols::{ModuleSymbol, storage::xml::xml_field_symbol::XmlFieldName}}, utils::{HashMap, HashSet}};

use lsp_types::{Diagnostic, Position, Range};
use tracing::info;

use crate::{constants::DiagnosticSource, core::{model::Model, symbols::{storage::SymbolTable, symbol_keys::{XmlAssetKey, XmlDataKey, XmlDeleteKey, XmlMenuItemKey, XmlRecordKey, XmlTemplateKey}}}};
use crate::{
    constants::{BuildSteps, MissingDataSource, OYarn, DEBUG_STEPS},
    core::{
        diagnostics::{create_diagnostic, DiagnosticCode},
        entry_point::{EntryPoint, EntryPointType},
        symbols::symbol_keys::{ModuleKey, SourceFileKey, XmlFileKey},
    },
    oyarn,
    threads::SessionInfo,
    Sy,
};

pub struct XmlValidator {
    pub xml_symbol: XmlFileKey,
    pub is_in_main_ep: bool,
    module: ModuleKey,
    fields_cache: HashMap<OYarn, HashSet<OYarn>>,
}

impl XmlValidator {

    pub fn new(entry: &Rc<RefCell<EntryPoint>>, symbol: XmlFileKey, symbol_table: &SymbolTable) -> Self {
        let is_in_main_ep = entry.borrow().typ == EntryPointType::MAIN || entry.borrow().typ == EntryPointType::ADDON;
        let module = symbol_table.find_module(symbol).unwrap();
        Self {
            xml_symbol: symbol,
            is_in_main_ep,
            module,
            fields_cache: HashMap::default(),
        }
    }

    pub fn validate(&mut self, session: &mut SessionInfo) {
        if !session.st().ready_for_step(self.xml_symbol.into(), BuildSteps::VALIDATION) {
            return;
        }
        session.st_mut().set_build_status(self.xml_symbol.into(), BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
        if DEBUG_STEPS {
            info!("VALIDATION - XML FILE: {}", &session.st()[self.xml_symbol].path);
        }
        let mut dependencies = vec![];
        let mut model_dependencies = vec![];
        let mut missing_model_dependencies = HashSet::default();
        let mut diagnostics = vec![];
        for data_key in session.st()[self.xml_symbol].data_symbols().iter().cloned().collect::<Vec<_>>() {
            self.validate_data(session, data_key, &mut diagnostics, &mut dependencies, &mut model_dependencies, &mut missing_model_dependencies);
        }
        for dep in dependencies.into_iter() {
            session.st_mut().add_dependency(self.xml_symbol.into(), dep, BuildSteps::VALIDATION, BuildSteps::ARCH_EVAL);
        }
        for model in model_dependencies.iter() {
            session.st_mut().add_model_dependencies(self.xml_symbol.into(), model);
        }
        if !missing_model_dependencies.is_empty() {
            session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(self.xml_symbol.into());
        }
        session.st_mut()[self.xml_symbol].not_found_models.extend(missing_model_dependencies.into_iter().map(|m| (m, BuildSteps::VALIDATION)));
        session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(self.xml_symbol.into());
        let (file_info, loaded) = FileMgr::get_or_recreate_file_info(session, self.xml_symbol.into());
        if !loaded {
            return;
        }
        file_info.borrow_mut().replace_diagnostics(DiagnosticSource::XML_VALIDATION, diagnostics);
        file_info.borrow_mut().publish_diagnostics(session);
        session.st_mut().set_build_status(self.xml_symbol.into(), BuildSteps::VALIDATION, BuildStatus::DONE);
    }

    fn validate_data(&mut self, session: &mut SessionInfo, data: XmlDataKey, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<SourceFileKey>, model_dependencies: &mut Vec<Rc<RefCell<Model>>>, missing_model_dependencies: &mut HashSet<OYarn>) {
        let Some(_) = session.st().get_file((data).into()) else {
            return;
        };
        match data {
            XmlDataKey::XmlRecord(xml_data_record) => self.validate_record(session, xml_data_record, diagnostics, dependencies, model_dependencies, missing_model_dependencies),
            XmlDataKey::XmlMenuItem(xml_data_menu_item) => self.validate_menu_item(session, xml_data_menu_item, diagnostics, dependencies, model_dependencies, missing_model_dependencies),
            XmlDataKey::XmlTemplate(xml_data_template) => self.validate_template(session, xml_data_template, diagnostics, dependencies, model_dependencies, missing_model_dependencies),
            XmlDataKey::XmlDelete(xml_data_delete) => self.validate_delete(session, xml_data_delete, diagnostics, dependencies, model_dependencies, missing_model_dependencies),
            XmlDataKey::XmlAsset(xml_data_asset) => self.validate_asset(session, xml_data_asset, diagnostics, dependencies, model_dependencies, missing_model_dependencies),
        }
    }
    fn validate_record(&mut self, session: &mut SessionInfo, xml_data_record: XmlRecordKey, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<SourceFileKey>, model_dependencies: &mut Vec<Rc<RefCell<Model>>>, missing_model_dependencies: &mut HashSet<OYarn>) {
        let xml_record = &session.st()[xml_data_record];
        let (model_name, model_range) = &xml_record.model;
        let maybe_model = session.sync_odoo.models.get(model_name).cloned();
        let model_exists = maybe_model.as_ref().map(|m| m.borrow_mut().has_symbols(session.st())).unwrap_or(false);
        if !model_exists {
            missing_model_dependencies.insert(model_name.clone());
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05056, &[model_name]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(model_range.start.try_into().unwrap(), 0), end: Position::new(model_range.end.try_into().unwrap(), 0) },
                    ..diagnostic
                });
            }
            info!("Model '{}' does not exist", model_name);
            return;
        }
        let Some(model) = maybe_model else {unreachable!();};
        model_dependencies.push(model.clone());
        // Here we want ALL model definitions
        let main_symbols = model.borrow().get_main_symbols(session, Some(self.module)).collect::<Vec<_>>();
        if main_symbols.is_empty() {
            missing_model_dependencies.insert(model_name.clone());
            let module_name = session.st().name(self.module);
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05055, &[model_name, module_name]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(model_range.start.try_into().unwrap(), 0), end: Position::new(model_range.end.try_into().unwrap(), 0) },
                    ..diagnostic
                });
            }
            info!("Model '{}' has no symbols in module '{}'", model_name, module_name);
            return;
        }

        dependencies.extend(
            main_symbols
            .iter().copied()
            .map(|sym|
                session.st().get_file(sym.into()).unwrap()
            )
        );
        let model_name = model_name.clone();
        if !self.fields_cache.contains_key(&model_name) {
            let py_fields = main_symbols.first().map(|&main_symbol| {
                SymbolTable::all_fields(main_symbol.into(), session, Some(self.module))
            });
            let model_ref = model.borrow();
            let xml_field_names_yarn = {
                model_ref
                    .get_xml_model_field_symbols(session.st(), Some(self.module))
                    .filter_map(|rec_key| {
                        session.st()[rec_key]
                                .get_field_text(XmlFieldName::Name, session.st())
                    })
                    .map(OYarn::from)
            };
            let all_fields = py_fields
                .into_iter()
                .flat_map(|m| m.into_keys())
                .chain(xml_field_names_yarn)
                .collect::<HashSet<_>>();
            self.fields_cache.insert(model_name.clone(), all_fields);
        }
        let all_fields = self.fields_cache.get(&model_name).unwrap();
        self.validate_fields(session, xml_data_record, all_fields, diagnostics, missing_model_dependencies);

        // For view-like records, register the XML file as a dependent of the
        // inner `<field name="model">target</field>` so find-refs on target's
        // Python methods/fields walks this XML.
        let inner_model_name = session.st()[xml_data_record]
            .get_field_text(XmlFieldName::Model, session.st())
            .map(|t| oyarn!("{}", t.trim()));
        if let Some(inner_model_name) = inner_model_name
            && !inner_model_name.is_empty() && inner_model_name != model_name
            && let Some(target_model) = session.sync_odoo.models.get(&inner_model_name).cloned()
        {
            model_dependencies.push(target_model);
        }
    }

    fn validate_fields(
        &self,
        session: &mut SessionInfo,
        xml_data_record: XmlRecordKey,
        all_fields: &HashSet<OYarn>,
        diagnostics: &mut Vec<Diagnostic>,
        missing_model_dependencies: &mut HashSet<OYarn>,
    ) {
        let xml_record = &session.st()[xml_data_record];
        let fields = xml_record.fields().clone();
        //check each field in the record
        for (field_name, field_key) in &fields {
            let mut has_translation = false;
            if session.sync_odoo.version >= (18, 2) {
                // Check for translation
                // obs: some language codes contain "@", e.g. "sr@latin", so we only split once
                if let Some((_fname, lang_code)) = field_name.split_once("@") {
                    has_translation = true;

                    // Validate language code
                    if !session.sync_odoo.check_language_and_track(lang_code, self.xml_symbol.into())
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05068, &[lang_code]) {
                            let field = &session.st()[*field_key];
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(field.range.start().into(), 0), end: Position::new(field.range.end().into(), 0) },
                                ..diagnostic
                            });
                        }
                }
            }
            // Validate field ref_key
            let ref_key = session.st()[*field_key].ref_key.clone();
            if let Some((ref_key_val, ref_key_range)) = ref_key.as_ref(){
                let xml_id_split: Vec<_> = ref_key_val.split('.').collect();
                match xml_id_split.len() {
                    0 => {}, // Should not happen
                    1 => { // Local reference, check that it is not empty
                        let ref_xml_id = xml_id_split[0];
                        if ref_xml_id.is_empty() {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05039, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(ref_key_range.start().into(), 0), end: Position::new(ref_key_range.end().into(), 0) },
                                    ..diagnostic
                                });
                            }
                        } else {
                            let range = std::ops::Range {
                                start: ref_key_range.start().to_usize(),
                                end: ref_key_range.end().to_usize(),
                            };
                            if SyncOdoo::get_xml_ids(session, self.xml_symbol.into(), ref_xml_id, &range, diagnostics).is_empty(&session.sync_odoo.symbol_table)
                            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05001, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(ref_key_range.start().into(), 0), end: Position::new(ref_key_range.end().into(), 0) },
                                    ..diagnostic
                                });
                            }
                        }
                    },
                    2 => {
                        let module_name = xml_id_split[0];
                        if !session.sync_odoo.modules.contains_key(module_name)
                            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05003, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(ref_key_range.start().into(), 0), end: Position::new(ref_key_range.end().into(), 0) },
                                    ..diagnostic
                                });
                            }},
                    _ => { // >= 2
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05051, &[ref_key_val]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(ref_key_range.start().into(), 0), end: Position::new(ref_key_range.end().into(), 0) },
                                ..diagnostic
                            });
                        }
                    }
                }
            }
            let field = &session.st()[*field_key];
            //Check that the field belong to the model
            if all_fields.contains(field_name) {
                //Check specific attributes
                let (Some(field_text), Some(field_text_range)) = (field.text.as_ref(), field.text_range.as_ref()) else {
                    continue;
                };
                let record = &session.st()[xml_data_record];
                match (record.model.0.as_str(), field_name.as_str()) {
                    ("ir.ui.view", "model") | ("ir.actions.act_window", "res_model") => {
                        let model = session.sync_odoo.models.get(field_text.as_str()).cloned();
                        let model_exists = model.as_ref().map(|m| m.borrow_mut().has_symbols(session.st())).unwrap_or(false);
                        if !model_exists {
                            missing_model_dependencies.insert(Sy!(field_text.clone()));
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05056, &[field_text, &record.model.0]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(field_text_range.start().into(), 0), end: Position::new(field_text_range.end().into(), 0) },
                                    ..diagnostic
                                });
                            }
                        } else {
                            let model_in_deps = model
                                .is_some_and(|m| m.borrow().model_in_deps(session, self.module));
                            if !model_in_deps
                                && let Some(diagnostic) = create_diagnostic(
                                    session,
                                    DiagnosticCode::OLS05055,
                                    &[field_text, session.st().name(self.module)],
                                )
                            {
                                diagnostics.push(Diagnostic {
                                    range: Range {
                                        start: Position::new(
                                            field_text_range.start().into(),
                                            0,
                                        ),
                                        end: Position::new(
                                            field_text_range.end().into(),
                                            0,
                                        ),
                                    },
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
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05057, &[field_name, &record.model.0]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(field.range.start().into(), 0), end: Position::new(field.range.end().into(), 0) },
                        ..diagnostic
                    });
                }
            }
        }
    }

    fn validate_menu_item(&self, _session: &mut SessionInfo, _xml_data_menu_item: XmlMenuItemKey, _diagnostics: &mut Vec<Diagnostic>, _dependencies: &mut Vec<SourceFileKey>, _model_dependencies: &mut Vec<Rc<RefCell<Model>>>, _missing_model_dependencies: &mut HashSet<OYarn>) {

    }

    fn validate_template(&self, session: &mut SessionInfo, xml_data_template: XmlTemplateKey, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<SourceFileKey>, _model_dependencies: &mut Vec<Rc<RefCell<Model>>>, _missing_model_dependencies: &mut HashSet<OYarn>) {
        if !self.is_in_main_ep {
            return;
        }
        let for_web = session.st()[xml_data_template].is_web;
        if for_web {
            self.validate_t_calls_for_frontend(session, xml_data_template, diagnostics, dependencies, _model_dependencies, _missing_model_dependencies);
        } else {
            self.validate_t_calls_for_backend(session, xml_data_template, diagnostics, dependencies, _model_dependencies, _missing_model_dependencies);
        }
    }

    fn validate_t_calls_for_frontend(&self, session: &mut SessionInfo, xml_data_template: XmlTemplateKey, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<SourceFileKey>, _model_dependencies: &mut Vec<Rc<RefCell<Model>>>, _missing_model_dependencies: &mut HashSet<OYarn>) {
        let t_calls = session.st()[xml_data_template].t_calls.clone();
        for (t_call_name, t_call_range) in &t_calls {
            let t_call_str = t_call_name.as_str();
            if t_call_str.contains("{{") || t_call_str.contains("#{") {
                continue;
            }
            let Some(templates) = session.sync_odoo.js_templates.get(t_call_str) else {continue};
            if templates.is_empty(&session.sync_odoo.symbol_table) {
                session.st_mut()[self.xml_symbol].not_found_data_ids.insert(
                    MissingDataSource::TEMPLATE(t_call_name.clone()),
                    BuildSteps::VALIDATION,
                );
                session.sync_odoo.get_main_entry().borrow_mut().not_found_data_ids.insert(self.xml_symbol.into());
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05073, &[t_call_str]) {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position::new(t_call_range.start().into(), 0),
                            end: Position::new(t_call_range.end().into(), 0),
                        },
                        ..diagnostic
                    });
                }
            } else {
                let mut found_one_valid = false;
                for template in templates.iter_valid(&session.sync_odoo.symbol_table) {
                    let module = session.st().find_module(template);
                    if let Some(module) = module {
                        let dir_name = &session.st()[module].dir_name;
                        if ModuleSymbol::is_in_deps(session.st(), self.module, dir_name) {
                            found_one_valid = true;
                            break;
                        }
                    }
                }
                // Check that the template's module is a declared dependency
                if !found_one_valid
                    && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05074, &[t_call_str, session.st().name(self.module)]) {
                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position::new(t_call_range.start().into(), 0),
                                end: Position::new(t_call_range.end().into(), 0),
                            },
                            ..diagnostic
                        });
                    }
                // Add file-level dependencies so re-validation is triggered when the template changes
                for template_key in templates.iter_valid(session.st()) {
                    if let Some(xml_file) = session.st().get_file(template_key.into()) {
                        dependencies.push(xml_file);
                    }
                }
            }
        }
    }

    fn validate_t_calls_for_backend(&self, session: &mut SessionInfo, xml_data_template: XmlTemplateKey, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<SourceFileKey>, _model_dependencies: &mut Vec<Rc<RefCell<Model>>>, _missing_model_dependencies: &mut HashSet<OYarn>) {
        let Some(file) = session.st().get_file(xml_data_template.into()) else {return};
        let t_calls = session.st()[xml_data_template].t_calls.clone();
        for (t_call_name, t_call_range) in &t_calls {
            let t_call_str = t_call_name.as_str();
            if t_call_str.contains("{{") || t_call_str.contains("#{") {
                continue;
            }
            let range = std::ops::Range {
                start: t_call_range.start().to_usize(),
                end: t_call_range.end().to_usize(),
            };
            let xml_ids = SyncOdoo::get_xml_ids(session, file, t_call_str, &range, diagnostics);
            if xml_ids.is_empty(&session.sync_odoo.symbol_table) {
                session.st_mut()[self.xml_symbol].not_found_data_ids.insert(
                    MissingDataSource::XML_ID(t_call_name.clone()),
                    BuildSteps::VALIDATION,
                );
                session.sync_odoo.get_main_entry().borrow_mut().not_found_data_ids.insert(self.xml_symbol.into());
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05073, &[t_call_str]) {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position::new(t_call_range.start().into(), 0),
                            end: Position::new(t_call_range.end().into(), 0),
                        },
                        ..diagnostic
                    });
                }
            } else {
                let mut found_one_valid = false;
                for template in xml_ids.iter_valid(&session.sync_odoo.symbol_table) {
                    let module = session.st().find_module(template);
                    if let Some(module) = module {
                        let dir_name = &session.st()[module].dir_name;
                        if ModuleSymbol::is_in_deps(session.st(), self.module, dir_name) {
                            found_one_valid = true;
                            break;
                        }
                    }
                }
                // Check that the template's module is a declared dependency
                if !found_one_valid
                    && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05074, &[t_call_str, session.st().name(self.module)]) {
                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position::new(t_call_range.start().into(), 0),
                                end: Position::new(t_call_range.end().into(), 0),
                            },
                            ..diagnostic
                        });
                    }
                // Add file-level dependencies so re-validation is triggered when the template changes
                for template_key in xml_ids.iter_valid(session.st()) {
                    if let Some(xml_file) = session.st().get_file(template_key.into()) {
                        dependencies.push(xml_file);
                    }
                }
            }
        }
    }

    fn validate_delete(&self, _session: &mut SessionInfo, _xml_data_delete: XmlDeleteKey, _diagnostics: &mut Vec<Diagnostic>, _dependencies: &mut Vec<SourceFileKey>, _model_dependencies: &mut Vec<Rc<RefCell<Model>>>, _missing_model_dependencies: &mut HashSet<OYarn>) {

    }

    fn validate_asset(&self, _session: &mut SessionInfo, _xml_data_asset: XmlAssetKey, _diagnostics: &mut Vec<Diagnostic>, _dependencies: &mut Vec<SourceFileKey>, _model_dependencies: &mut Vec<Rc<RefCell<Model>>>, _missing_model_dependencies: &mut HashSet<OYarn>) {

    }
}
