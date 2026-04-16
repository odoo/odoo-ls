use crate::{
    S, core::{
        evaluation::ContextValue,
        odoo::SyncOdoo,
        symbols::{
            ModuleSymbol,
            symbol_keys::{ModuleKey, SourceFileKey, SymbolKey},
        },
        xml_data::{OdooData, OdooDataRecord},
    }, threads::SessionInfo
};
use roxmltree::Node;
use std::{collections::HashMap, ops::Range};

pub enum XmlAstResult {
    SYMBOL(SymbolKey),
    #[allow(non_camel_case_types)]
    XML_DATA(OdooDataRecord),
}

impl XmlAstResult {
    pub fn as_symbol(&self) -> SymbolKey {
        match self {
            XmlAstResult::SYMBOL(sym) => *sym,
            XmlAstResult::XML_DATA(_) =>panic!("Xml Data is not a symbol"),
        }
    }

    pub fn as_xml_data(&self) -> OdooDataRecord {
        match self {
            XmlAstResult::SYMBOL(_) => panic!("Symbol is not an XML Data"),
            XmlAstResult::XML_DATA(record) => record.clone(),
        }
    }
}

pub struct XmlAstUtils {}

impl XmlAstUtils {

    pub fn get_symbols(session: &mut SessionInfo, file_symbol: SourceFileKey, root: roxmltree::Node, offset: usize, on_dep_only: bool) -> (Vec<XmlAstResult>, Option<Range<usize>>) {
        let mut results = (vec![], None);
        let from_module = session.sync_odoo.symbol_table.find_module(file_symbol);
        let mut context_xml = HashMap::new();
        for node in root.children() {
            XmlAstUtils::visit_node(session, &node, offset, from_module, &mut context_xml, &mut results, on_dep_only);
        }
        results
    }

    fn visit_node(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
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

    fn visit_record(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
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
                        results.0.extend(model.borrow().all_symbols(session, from_module, false).iter().filter(|s| s.1.is_none()).map(|s| XmlAstResult::SYMBOL(s.0.into())));
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

    fn visit_field(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
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
                        for (class_key, missing_dep) in model.borrow().all_symbols(session, from_module, true) {
                            if missing_dep.is_none() {
                                let content = session.sync_odoo.symbol_table.get_content_symbol(class_key.into(), attr.value(), u32::MAX);
                                for symbol in content.symbols.iter() {
                                    results.0.push(XmlAstResult::SYMBOL(*symbol));
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

    fn visit_text(session: &mut SessionInfo, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
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

    fn visit_menu_item(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
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

    fn visit_template(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
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

    fn add_model_result(session: &mut SessionInfo, node: &Node, from_module: Option<ModuleKey>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
        if let Some(model) = session.sync_odoo.models.get(node.text().unwrap()).cloned() {
            let from_module = match on_dep_only {
                true => from_module,
                false => None,
            };
            results.0.extend(model.borrow().all_symbols(session, from_module, false).iter().filter(|s| s.1.is_none()).map(|s| XmlAstResult::SYMBOL(s.0.into())));
            results.1 = Some(node.range());
        }
    }

    fn add_xml_id_result(session: &mut SessionInfo, xml_id: &str, file_symbol: SourceFileKey, range: Range<usize>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
        let mut xml_ids = SyncOdoo::get_xml_ids(session, file_symbol, xml_id, &range, &mut vec![]);
        if on_dep_only {
            let st = &session.sync_odoo.symbol_table;
            xml_ids = xml_ids.into_iter().filter(|x|
                if let Some(file_weak) = x.get_file_symbol(st)
                && let Some(file) = file_weak.upgrade(st)
                && let Some(module) = st.find_module(file) {
                    ModuleSymbol::is_in_deps(st, st.find_module(file_symbol).unwrap(), &st[module].name)
                } else {
                    false
                }
            ).collect::<Vec<_>>();
        }
        for xml_data in xml_ids.iter() {
            match xml_data {
                OdooData::RECORD(r) => {
                    results.0.push(XmlAstResult::XML_DATA(r.clone()));
                },
                _ => {}
            }
        }
    }

}
