use std::{cell::RefCell, cmp::Ordering, collections::HashMap, hash::Hash, path::PathBuf, rc::Rc};

use lsp_types::{Diagnostic, Position, Range};
use tracing::{info, trace};

use crate::{constants::{BuildSteps, OYarn, SymType, DEBUG_STEPS, EXTENSION_NAME}, core::{diagnostics::{create_diagnostic, DiagnosticCode}, entry_point::{EntryPoint, EntryPointType}, evaluation::ContextValue, file_mgr::FileInfo, model::Model, odoo::SyncOdoo, symbols::symbol::Symbol, xml_data::{XmlData, XmlDataDelete, XmlDataMenuItem, XmlDataRecord, XmlDataTemplate}}, oyarn, threads::SessionInfo, utils::compare_semver, Sy, S};



pub struct XmlValidator {
    pub xml_symbol: Rc<RefCell<Symbol>>,
    pub is_in_main_ep: bool,
}

impl XmlValidator {

    pub fn new(entry: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>) -> Self {
        let is_in_main_ep = entry.borrow().typ == EntryPointType::MAIN || entry.borrow().typ == EntryPointType::ADDON;
        Self {
            xml_symbol: symbol,
            is_in_main_ep,
        }
    }

    fn get_file_info(&mut self, odoo: &mut SyncOdoo) -> Rc<RefCell<FileInfo>> {
        let file_symbol = self.xml_symbol.borrow();
        let mut path = file_symbol.paths()[0].clone();
        let file_info_rc = odoo.get_file_mgr().borrow().get_file_info(&path).expect("File not found in cache").clone();
        file_info_rc
    }

    pub fn validate(&mut self, session: &mut SessionInfo) {
        if DEBUG_STEPS {
            trace!("Validating XML File {}", self.xml_symbol.borrow().name());
        }
        let module = self.xml_symbol.borrow().find_module().unwrap();
        let mut dependencies = vec![];
        let mut model_dependencies = vec![];
        let mut diagnostics = vec![];
        for xml_ids in self.xml_symbol.borrow().as_xml_file_sym().xml_ids.values() {
            for xml_id in xml_ids.iter() {
                self.validate_xml_id(session, &module, xml_id, &mut diagnostics, &mut dependencies, &mut model_dependencies);
            }
        }
        for dep in dependencies.iter_mut() {
            self.xml_symbol.borrow_mut().add_dependency(&mut dep.borrow_mut(), BuildSteps::VALIDATION, BuildSteps::ARCH_EVAL);
        }
        for model in model_dependencies.iter() {
            self.xml_symbol.borrow_mut().add_model_dependencies(&model);
        }
        let file_info = self.get_file_info(&mut session.sync_odoo);
        file_info.borrow_mut().replace_diagnostics(BuildSteps::VALIDATION, diagnostics);
        file_info.borrow_mut().publish_diagnostics(session);
    }

    pub fn validate_xml_id(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, data: &XmlData, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>, model_dependencies: &mut Vec<Rc<RefCell<Model>>>) {
        let Some(xml_file) = data.get_xml_file_symbol() else {
            return;
        };
        let path = xml_file.borrow().paths()[0].clone();
        match data {
            XmlData::RECORD(xml_data_record) => self.validate_record(session, module, xml_data_record, diagnostics, dependencies, model_dependencies),
            XmlData::MENUITEM(xml_data_menu_item) => self.validate_menu_item(session, module, xml_data_menu_item, diagnostics, dependencies, model_dependencies),
            XmlData::TEMPLATE(xml_data_template) => self.validate_template(session, module, xml_data_template, diagnostics, dependencies, model_dependencies),
            XmlData::DELETE(xml_data_delete) => self.validate_delete(session, module, xml_data_delete, diagnostics, dependencies, model_dependencies),
        }
    }

    fn validate_record(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_record: &XmlDataRecord, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>, model_dependencies: &mut Vec<Rc<RefCell<Model>>>) {
        let Some(model) = session.sync_odoo.models.get(&xml_data_record.model.0).cloned() else {
            //TODO register to not_found_models
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05055, &[&xml_data_record.model.0, module.borrow().name()]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(xml_data_record.model.1.start.try_into().unwrap(), 0), end: Position::new(xml_data_record.model.1.end.try_into().unwrap(), 0) },
                    ..diagnostic.clone()
                });
            }
            info!("Model '{}' not found in module '{}'", xml_data_record.model.0, module.borrow().name());
            return;
        };
        model_dependencies.push(model.clone());
        let main_symbols = model.borrow().get_main_symbols(session, Some(module.clone()));
        for main_sym in main_symbols.iter() {
            dependencies.push(main_sym.borrow().get_file().unwrap().upgrade().unwrap());
        }
        let all_fields = Symbol::all_fields(&main_symbols[0], session, Some(module.clone()));
        self.validate_fields(session, xml_data_record, &all_fields, diagnostics);
    }

    fn validate_fields(&self, session: &mut SessionInfo, xml_data_record: &XmlDataRecord, all_fields: &HashMap<OYarn, Vec<(Rc<RefCell<Symbol>>, Option<OYarn>)>>, diagnostics: &mut Vec<Diagnostic>) {
        //Compute mandatory fields
        let mut mandatory_fields: Vec<String> = vec![];
        for (field_name, field_sym) in all_fields.iter() {
            for (fs, deps) in field_sym.iter() {
                if deps.is_none() {
                    let has_required = fs.borrow().evaluations().unwrap_or(&vec![]).iter()
                    .any(|eval| 
                        eval.symbol.get_symbol_as_weak(session, &mut None, diagnostics, None)
                        .context.get("required").unwrap_or(&ContextValue::BOOLEAN(false)).as_bool()
                    );
                    let has_default = fs.borrow().evaluations().unwrap_or(&vec![]).iter()
                    .any(|eval| 
                        eval.symbol.get_symbol_as_weak(session, &mut None, diagnostics, None)
                        .context.contains_key("default")
                    );
                    if has_required && !has_default {
                        mandatory_fields.push(field_name.to_string());
                    }
                }
            }
        }
        //check each field in the record
        for field in &xml_data_record.fields {
            let mut field_name = field.name.clone();
            let mut has_translation = false;
            if compare_semver(&session.sync_odoo.full_version, "18.2.0") >= Ordering::Equal {
                let translation = field.name.split("@").collect::<Vec<&str>>();
                if translation.len() > 1 {
                    field_name = oyarn!("{}", translation[0]);
                    has_translation = true;
                    //TODO check that the language exists
                }
            }
            //Check that the field belong to the model
            let declared_field = all_fields.get(&field.name);
            if let Some(_declared_field) = declared_field {
                mandatory_fields.retain(|f| f != &field.name.as_str());
                //Check specific attributes
                match xml_data_record.model.0.as_str() {
                    "ir.ui.view" => {
                        if field.name == "model" && field.text.is_some() && field.text_range.is_some() {
                            //TODO text that field.text is a valid model
                            let model = session.sync_odoo.models.get(&Sy!(field.text.as_ref().unwrap().clone())).cloned();
                            let mut main_sym = vec![];
                            if let Some(model) = model {
                                let from_module = self.xml_symbol.borrow().find_module();
                                main_sym = model.borrow().get_main_symbols(session, from_module);
                            }
                            if main_sym.is_empty() {
                                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05056, &[&field.text.as_ref().unwrap()]) {
                                    diagnostics.push(Diagnostic {
                                        range: Range { start: Position::new(field.text_range.as_ref().unwrap().start.try_into().unwrap(), 0), end: Position::new(field.text_range.as_ref().unwrap().end.try_into().unwrap(), 0) },
                                        ..diagnostic.clone()
                                    });
                                }
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
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05057, &[&field.name, &xml_data_record.model.0]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(field.range.start.try_into().unwrap(), 0), end: Position::new(field.range.end.try_into().unwrap(), 0) },
                        ..diagnostic.clone()
                    });
                }
            }
        }
        //Diagnostic if some mandatory fields are not detected
        // if !mandatory_fields.is_empty() {
        // We have to check  that remaining fields are not declared in an inherited record or is automatically field (delegate=True)
        //     diagnostics.push(Diagnostic::new(
        //         Range::new(Position::new(xml_data_record.range.start.try_into().unwrap(), 0), Position::new(xml_data_record.range.end.try_into().unwrap(), 0)),
        //         Some(lsp_types::DiagnosticSeverity::ERROR),
        //         Some(lsp_types::NumberOrString::String(S!("OLS30452"))),
        //         Some(EXTENSION_NAME.to_string()),
        //         format!("Some mandatory fields are not declared in the record: {:?}", mandatory_fields),
        //         None,
        //         None
        //     ));
        // }
    }

    fn validate_menu_item(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_menu_item: &XmlDataMenuItem, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>, model_dependencies: &mut Vec<Rc<RefCell<Model>>>) {
        
    }

    fn validate_template(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_template: &XmlDataTemplate, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>, model_dependencies: &mut Vec<Rc<RefCell<Model>>>) {
        
    }

    fn validate_delete(&self, session: &mut SessionInfo, module: &Rc<RefCell<Symbol>>, xml_data_delete: &XmlDataDelete, diagnostics: &mut Vec<Diagnostic>, dependencies: &mut Vec<Rc<RefCell<Symbol>>>, model_dependencies: &mut Vec<Rc<RefCell<Model>>>) {
        
    }
}