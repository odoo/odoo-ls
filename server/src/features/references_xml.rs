use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};

use lsp_types::Location;
use roxmltree::Node;

use crate::{S, constants::SymType, core::{evaluation::ContextValue, file_mgr::FileMgr, symbols::symbol::Symbol}, features::{references::ReferenceTarget}, threads::SessionInfo};


pub enum XmlAstReferenceVisitor {

}

impl XmlAstReferenceVisitor {
    

    pub fn search_target(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, root: roxmltree::Node, target: &ReferenceTarget) -> Vec<Location> {
        let mut results = vec![];
        let from_module = file_symbol.borrow().find_module();
        let mut context_xml = HashMap::new();
        for node in root.children() {
            XmlAstReferenceVisitor::visit_node(session, &node, from_module.clone(), &mut context_xml, &mut results, target);
        }
        let uri = FileMgr::pathname2uri(&file_symbol.borrow().paths()[0]);
        let result_locations = results.iter().map(|range|
            Location {
                uri: uri.clone(),
                range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &file_symbol.borrow().paths()[0], range),
            }
        ).collect();
        result_locations
    }

    fn visit_node(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        if node.is_element() {
            match node.tag_name().name()  {
                "record" => {
                    XmlAstReferenceVisitor::visit_record(session, &node, from_module.clone(), ctxt, results, target);
                }
                "field" => {
                    XmlAstReferenceVisitor::visit_field(session, &node, from_module.clone(), ctxt, results, target);
                },
                "menuitem" => {
                    XmlAstReferenceVisitor::visit_menu_item(session, &node, from_module.clone(), ctxt, results, target);
                },
                "template" => {
                    XmlAstReferenceVisitor::visit_template(session, &node, from_module.clone(), ctxt, results, target);
                }
                _ => {
                    for child in node.children() {
                        XmlAstReferenceVisitor::visit_node(session, &child, from_module.clone(), ctxt, results, target);
                    }
                }
            }
        } else if node.is_text() {
            XmlAstReferenceVisitor::visit_text(session, &node, from_module, ctxt, results, target);
        }
    }

    fn visit_record(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
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
                        if s.borrow().typ() == SymType::CLASS {
                            if let Some(model) = &s.borrow().as_class_sym()._model {
                                if model.name == attr.value() {
                                    results.push(attr.range_value());
                                }
                            }
                        }
                    }
                }
            } else if attr.name() == "id" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(&attr, &from_module, target) {
                    results.push(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module.clone(), ctxt, results, target);
        }
        ctxt.remove(&S!("record_model"));
    }

    fn visit_field(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        for attr in node.attributes() {
            if attr.name() == "name" {
                ctxt.insert(S!("field_name"), ContextValue::STRING(attr.value().to_string()));
                let ReferenceTarget::Symbol(target) = target else {continue;};
                if target.borrow().typ() != SymType::VARIABLE {continue;}
                if !target.borrow().is_field(session) {continue;}
                if target.borrow().name() != attr.value() {continue;}
                //field name matches, but we still have to check model is the same
                let model_name = ctxt.get(&S!("record_model")).cloned().unwrap_or(ContextValue::STRING(S!(""))).as_string();
                if model_name.is_empty() {continue;}
                let field_model = target.borrow().get_in_parents(&vec![SymType::CLASS], true);
                let Some(field_model) = field_model else {continue;};
                let Some(field_model) = field_model.upgrade() else {continue;};
                let field_model = field_model.borrow();
                let Some(model) = field_model.as_class_sym()._model.as_ref() else {continue;};
                if model.name == model_name {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "ref" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(&attr, &from_module, target) {
                    results.push(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module.clone(), ctxt, results, target);
        }
        ctxt.remove(&S!("field_name"));
    }

    fn visit_text(_session: &mut SessionInfo, node: &Node, _from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        let model = ctxt.get(&S!("record_model")).cloned().unwrap_or(ContextValue::STRING(S!(""))).as_string();
        let field = ctxt.get(&S!("field_name")).cloned().unwrap_or(ContextValue::STRING(S!(""))).as_string();
        if model.is_empty() || field.is_empty() {
            return;
        }
        if field == "model" || field == "res_model" {
            if let ReferenceTarget::Symbol(target) = target {
                if target.borrow().typ() == SymType::CLASS {
                    if let Some(model) = &target.borrow().as_class_sym()._model {
                        if model.name == node.text().unwrap() {
                            results.push(node.range());
                        }
                    }
                }
            }
        }
        if field == "context" {
            //TODO
        }
    }

    fn test_attr_as_xml_id(attr: &roxmltree::Attribute, from_module: &Option<Rc<RefCell<Symbol>>>, target: &ReferenceTarget) -> bool {
        let attr_full = if attr.value().contains(".") {
            attr.value().to_string()
        } else {
            if let Some(module) = from_module {
                format!("{}.{}", module.borrow().name(), attr.value())
            } else {
                attr.value().to_string()
            }
        };
        if let ReferenceTarget::String(target_str) = target {
            return attr_full == *target_str;
        }
        false
    }

    fn visit_menu_item(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        for attr in node.attributes() {
            if attr.name() == "action" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(&attr, &from_module, target) {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "groups" {
                //TODO
            } else if attr.name() == "id" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(&attr, &from_module, target) {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "parent" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(&attr, &from_module, target) {
                    results.push(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module.clone(), ctxt, results, target);
        }
    }

    fn visit_template(session: &mut SessionInfo<'_>, node: &Node, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        for attr in node.attributes() {
            if attr.name() == "id" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(&attr, &from_module, target) {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "inherit_id" {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(&attr, &from_module, target) {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "groups" {
                //TODO
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module.clone(), ctxt, results, target);
        }
    }
}