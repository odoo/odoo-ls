use crate::{
    S, constants::SymType, core::{
        evaluation_context::ContextValue,
        file_mgr::FileMgr,
        symbols::{
            storage::SymbolTable, symbol_keys::{ModuleKey, SymbolKey, XmlFileKey}
        },
    }, features::{references::ReferenceTarget, xml_ast_utils::XmlAstUtils}, threads::SessionInfo
};
use lsp_types::Location;
use roxmltree::Node;
use std::{ops::Range};
use crate::utils::HashMap;

pub enum XmlAstReferenceVisitor {

}

impl XmlAstReferenceVisitor {

    pub fn search_target(session: &mut SessionInfo, file_symbol: XmlFileKey, root: roxmltree::Node, target: &ReferenceTarget) -> Vec<Location> {
        let mut results = vec![];
        let from_module = session.st().find_module(file_symbol);
        let mut context_xml = HashMap::default();
        for node in root.children() {
            XmlAstReferenceVisitor::visit_node(session, &node, from_module, &mut context_xml, &mut results, target);
        }
        let path = session.st()[file_symbol].path.clone();
        let uri = FileMgr::pathname2uri(&path);
        let result_locations = results.iter().map(|range|
            Location {
                uri: uri.clone(),
                range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &path, range),
            }
        ).collect();
        result_locations
    }

    fn visit_node(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        if node.is_element() {
            XmlAstReferenceVisitor::scan_format_xml_id_refs(session.st(), node, from_module, target, results);
            match node.tag_name().name()  {
                "record" => {
                    XmlAstReferenceVisitor::visit_record(session, &node, from_module, ctxt, results, target);
                }
                "field" => {
                    XmlAstReferenceVisitor::visit_field(session, &node, from_module, ctxt, results, target);
                },
                "menuitem" => {
                    XmlAstReferenceVisitor::visit_menu_item(session, &node, from_module, ctxt, results, target);
                },
                "template" => {
                    XmlAstReferenceVisitor::visit_template(session, &node, from_module, ctxt, results, target);
                }
                "button" => {
                    XmlAstReferenceVisitor::visit_button(session, &node, from_module, ctxt, results, target);
                }
                _ => {
                    for child in node.children() {
                        XmlAstReferenceVisitor::visit_node(session, &child, from_module, ctxt, results, target);
                    }
                }
            }
        } else if node.is_text() {
            XmlAstReferenceVisitor::visit_text(session, &node, from_module, ctxt, results, target);
        }
    }

    /// `<button name="method_x" type="object">` invokes a method on the current
    /// record's model. When the rename/find-refs target is a method (Function on
    /// a model class), match the button's `name` value against the method name
    /// AND verify the surrounding `record_model` matches the method's class model.
    fn visit_button(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        let is_action_type = node.attribute("type") == Some("action");
        if !is_action_type {
            if let &ReferenceTarget::Symbol(SymbolKey::Function(target_fn)) = target {
                for attr in node.attributes() {
                    if attr.name() != "name" { continue; }
                    if session.st()[target_fn].name != attr.value() { continue; }
                    let target_class = session.st().get_in_parents(target_fn.into(), &[SymType::CLASS], true);
                    let Some(SymbolKey::Class(target_class)) = target_class else { continue; };
                    let Some(target_model) = session.st()[target_class]._model.as_ref() else { continue; };
                    let record_model = ctxt.get("record_model").map(ContextValue::as_str).unwrap_or_default();
                    if record_model.is_empty() { continue; }
                    if target_model.name == *record_model {
                        results.push(attr.range_value());
                    }
                }
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module, ctxt, results, target);
        }
    }

    fn visit_record(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        for attr in node.attributes() {
            if attr.name() == "model" {
                let model_name = attr.value().to_string();
                ctxt.insert(S!("record_model"), ContextValue::STRING(model_name.clone()));
                match target {
                    ReferenceTarget::String(s) => {
                        if attr.value() == s {
                            results.push(attr.range_value());
                        }
                    },
                    ReferenceTarget::Symbol(s) => {
                        if let &SymbolKey::Class(class_key) = s {
                            if let Some(model) = &session.st()[class_key]._model {
                                if model.name == attr.value() {
                                    results.push(attr.range_value());
                                }
                            }
                        }
                    }
                }
            } else if attr.name() == "id" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module, ctxt, results, target);
        }
        ctxt.remove(&S!("record_model"));
    }

    fn visit_field(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        for attr in node.attributes() {
            if attr.name() == "name" {
                ctxt.insert(S!("field_name"), ContextValue::STRING(attr.value().to_string()));
                let &ReferenceTarget::Symbol(SymbolKey::Variable(target)) = target else {continue;};
                if !SymbolTable::is_field(session, target.into()) {continue;}
                if session.st()[target].name != attr.value() {continue;}
                //field name matches, but we still have to check model is the same
                let model_name = ctxt.get("record_model").map(ContextValue::as_str).unwrap_or_default();
                if model_name.is_empty() {continue;}
                let field_model = session.st().get_in_parents(target.into(), &[SymType::CLASS], true);
                let Some(SymbolKey::Class(field_model)) = field_model else {continue;};
                let Some(model) = session.st()[field_model]._model.as_ref() else {continue;};
                if model.name == *model_name {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "ref" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
            }
        }
        let arch_model = XmlAstUtils::arch_field_target_model(node);
        let prev_record_model = arch_model.as_ref()
            .map(|m| ctxt.insert(S!("record_model"), ContextValue::STRING(m.clone())));
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module, ctxt, results, target);
        }
        if arch_model.is_some() {
            match prev_record_model.flatten() {
                Some(prev) => { ctxt.insert(S!("record_model"), prev); }
                None => { ctxt.remove("record_model"); }
            }
        }
        ctxt.remove("field_name");
    }

    fn visit_text(session: &mut SessionInfo, node: &Node, _from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        let model = ctxt.get("record_model").map(ContextValue::as_str).unwrap_or_default();
        let field = ctxt.get("field_name").map(ContextValue::as_str).unwrap_or_default();
        if model.is_empty() || field.is_empty() {
            return;
        }
        if field == "model" || field == "res_model" {
            if let &ReferenceTarget::Symbol(SymbolKey::Class(target)) = target {
                if let Some(model) = &session.st()[target]._model {
                    if model.name == node.text().unwrap() {
                        results.push(node.range());
                    }
                }
            }
        }
        if field == "context" {
            //TODO
        }
    }

    fn scan_format_xml_id_refs(symbol_table: &SymbolTable, node: &Node, from_module: Option<ModuleKey>, target: &ReferenceTarget, results: &mut Vec<Range<usize>>) {
        let ReferenceTarget::String(target_str) = target else { return };
        let module_dir = from_module.map(|m| symbol_table[m].name.as_str());
        for attr in node.attributes() {
            XmlAstUtils::for_each_format_xml_id_ref(&attr, |inner, range| {
                let matches = if inner.contains('.') {
                    inner == target_str.as_str()
                } else if let Some(dir) = module_dir {
                    target_str.len() == dir.len() + 1 + inner.len()
                        && target_str.as_bytes().get(dir.len()) == Some(&b'.')
                        && target_str.starts_with(dir)
                        && target_str.ends_with(inner)
                } else {
                    false
                };
                if matches {
                    results.push(range);
                }
            });
        }
    }

    fn test_attr_as_xml_id(symbol_table: &SymbolTable, attr: &roxmltree::Attribute, from_module: Option<ModuleKey>, target: &ReferenceTarget) -> bool {
        let attr_full = if attr.value().contains(".") {
            attr.value().to_string()
        } else {
            if let Some(module) = from_module {
                format!("{}.{}", symbol_table[module].name, attr.value())
            } else {
                attr.value().to_string()
            }
        };
        if let ReferenceTarget::String(target_str) = target {
            return attr_full == *target_str;
        }
        false
    }

    fn visit_menu_item(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        for attr in node.attributes() {
            if attr.name() == "action" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "groups" {
                //TODO
            } else if attr.name() == "id" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "parent" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module, ctxt, results, target);
        }
    }

    fn visit_template(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        for attr in node.attributes() {
            if attr.name() == "id" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "inherit_id" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "groups" {
                //TODO
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module, ctxt, results, target);
        }
    }
}
