use crate::utils::HashMap;
use crate::{
    S,
    core::{
        evaluation_context::ContextValue,
        model::Model,
        odoo::SyncOdoo,
        symbols::{
            ModuleSymbol,
            symbol_keys::{ModuleKey, SourceFileKey, SymbolKey, XmlId},
        },
    },
    threads::SessionInfo,
};
use roxmltree::Node;
use std::ops::Range;

pub struct XmlAstUtils {}

impl XmlAstUtils {

    pub fn get_symbols(session: &mut SessionInfo, file_symbol: SourceFileKey, root: roxmltree::Node, offset: usize, on_dep_only: bool) -> (Vec<SymbolKey>, Option<Range<usize>>) {
        let mut results = (vec![], None);
        let from_module = session.sync_odoo.symbol_table.find_module(file_symbol);
        let mut context_xml = HashMap::default();
        for node in root.children() {
            XmlAstUtils::visit_node(session, &node, offset, from_module, &mut context_xml, &mut results, on_dep_only);
        }
        results
    }

    fn visit_node(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        if node.is_element() {
            match node.tag_name().name()  {
                "record" => {
                    XmlAstUtils::visit_record(session, &node, offset, from_module, ctxt, results, on_dep_only);
                }
                "field" => {
                    XmlAstUtils::visit_field(session, &node, offset, from_module, ctxt, results, on_dep_only);
                },
                "menuitem" => {
                    XmlAstUtils::visit_menu_item(session, &node, offset, from_module, ctxt, results, on_dep_only);
                },
                "template" => {
                    XmlAstUtils::visit_template(session, &node, offset, from_module, ctxt, results, on_dep_only);
                }
                _ => {
                    for child in node.children() {
                        XmlAstUtils::visit_node(session, &child, offset, from_module, ctxt, results, on_dep_only);
                    }
                }
            }
        } else if node.is_text() {
            XmlAstUtils::visit_text(session, &node, offset, from_module, ctxt, results, on_dep_only);
        }
    }

    fn visit_record(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "model" {
                let model_name = attr.value().to_string();
                ctxt.insert(S!("record_model"), ContextValue::STRING(model_name.clone()));
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    if let Some(model) = session.sync_odoo.models.get(model_name.as_str()).cloned() {
                        let from_module = match on_dep_only {
                            true => from_module,
                            false => None,
                        };
                        results.0.extend(
                            model.borrow().get_model_symbols(session.st(), from_module)
                                .into_iter().map(SymbolKey::from)
                            );
                        results.1 = Some(attr.range_value());
                    }
                }
            } else if attr.name() == "id" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module, ctxt, results, on_dep_only);
        }
        ctxt.remove("record_model");
    }

    fn visit_field(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "name" {
                ctxt.insert(S!("field_name"), ContextValue::STRING(attr.value().to_string()));
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    let model_name = ctxt.get("record_model").map(ContextValue::as_str).unwrap_or_default();
                    if model_name.is_empty() {
                        continue;
                    }
                    if let Some(model) = session.sync_odoo.models.get(model_name).cloned() {
                        let from_module = match on_dep_only {
                            true => from_module,
                            false => None,
                        };
                        let field_name = attr.value();
                        for class_key in Model::get_full_model_classes(model.clone(), session, from_module) {
                            let content = session.st().get_content_symbol(class_key.into(), field_name, u32::MAX);
                            for symbol in content.symbols {
                                results.0.push(symbol);
                            }
                        }
                        let model_ref = model.borrow();
                        for xml_record_key in model_ref.get_xml_model_field_symbols(session.st(), from_module) {
                            let record = &session.st()[xml_record_key];
                            if let Some(&name_field_key) = record.fields().get("name") {
                                let name_field = &session.st()[name_field_key];
                                if name_field.text.as_deref() == Some(field_name) {
                                    results.0.push(xml_record_key.into());
                                }
                            }
                        }
                        results.1 = Some(attr.range_value());
                    }
                }
            } else if attr.name() == "ref" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module, ctxt, results, on_dep_only);
        }
        ctxt.remove("field_name");
    }

    fn visit_text(session: &mut SessionInfo, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        if node.range().start <= offset && node.range().end >= offset {
            let model = ctxt.get("record_model").map(ContextValue::as_str).unwrap_or_default();
            let field = ctxt.get("field_name").map(ContextValue::as_str).unwrap_or_default();
            if model.is_empty() || field.is_empty() {
                return;
            }
            if field == "model" || field == "res_model" { //do not check model, let's assume it will contains a model name
                XmlAstUtils::add_model_result(session, node, from_module, results, on_dep_only);
            }
        }
    }

    fn visit_menu_item(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "action" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            } else if attr.name() == "groups" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module, ctxt, results, on_dep_only);
        }
    }

    fn visit_template(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "inherit_id" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            } else if attr.name() == "groups" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module, ctxt, results, on_dep_only);
        }
    }

    fn add_model_result(session: &mut SessionInfo, node: &Node, from_module: Option<ModuleKey>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        if let Some(model) = session.sync_odoo.models.get(node.text().unwrap()).cloned() {
            let from_module = match on_dep_only {
                true => from_module,
                false => None,
            };
            results.0.extend(
                model.borrow().get_model_symbols(session.st(), from_module)
                    .into_iter().map(SymbolKey::from)
            );
            results.1 = Some(node.range());
        }
    }

    fn add_xml_id_result(session: &mut SessionInfo, xml_id: &str, file_symbol: SourceFileKey, range: Range<usize>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        let xml_ids = SyncOdoo::get_xml_ids(session, file_symbol, xml_id, &range, &mut vec![]);

        for xml_id in xml_ids.iter_valid(session.st()) {
            if on_dep_only {
                if let Some(module) = session.st().find_module(xml_id) {
                    if !ModuleSymbol::is_in_deps(
                        session.st(),
                        session.st().find_module(file_symbol).unwrap(),
                        &session.st()[module].name,
                    ) {
                        continue;
                    }
                }
            }
            if let XmlId::XmlRecord(record_key) = xml_id {
                results.0.push(record_key.into());
            } else if let XmlId::PythonClass(record_key) = xml_id {
                results.0.push(record_key.into());
            }
        }
    }

}
