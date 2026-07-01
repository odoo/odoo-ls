use crate::{
    constants::SymType, core::{
        file_mgr::FileMgr,
        symbols::{
            storage::SymbolTable, symbol_keys::{ModuleKey, SymbolKey, XmlFileKey}
        },
    }, features::{references::ReferenceTarget, xml_ast_utils::{XmlAstUtils, XmlScope}}, threads::SessionInfo
};
use lsp_types::Location;
use roxmltree::Node;
use std::ops::Range;

pub enum XmlAstReferenceVisitor {

}

impl XmlAstReferenceVisitor {

    pub fn search_target(session: &mut SessionInfo, file_symbol: XmlFileKey, root: roxmltree::Node, target: &ReferenceTarget) -> Vec<Location> {
        let mut results = vec![];
        let from_module = session.st().find_module(file_symbol);
        for node in root.children() {
            XmlAstReferenceVisitor::visit_node(session, &node, from_module, XmlScope::default(), &mut results, target);
        }
        let path = session.st()[file_symbol].path.clone();
        let uri = FileMgr::pathname2uri(&path);
        
        results.iter().map(|range|
            Location {
                uri: uri.clone(),
                range: session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &path, range),
            }
        ).collect()
    }

    fn visit_node<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, from_module: Option<ModuleKey>, scope: XmlScope<'a>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        if node.is_element() {
            XmlAstReferenceVisitor::scan_format_xml_id_refs(session.st(), node, from_module, target, results);
            match node.tag_name().name()  {
                "record" => {
                    XmlAstReferenceVisitor::visit_record(session, node, from_module, scope, results, target);
                }
                "field" => {
                    XmlAstReferenceVisitor::visit_field(session, node, from_module, scope, results, target);
                },
                "menuitem" => {
                    XmlAstReferenceVisitor::visit_menu_item(session, node, from_module, scope, results, target);
                },
                "template" => {
                    XmlAstReferenceVisitor::visit_template(session, node, from_module, scope, results, target);
                }
                "button" => {
                    XmlAstReferenceVisitor::visit_button(session, node, from_module, scope, results, target);
                }
                _ => {
                    for child in node.children() {
                        XmlAstReferenceVisitor::visit_node(session, &child, from_module, scope, results, target);
                    }
                }
            }
        } else if node.is_text() {
            XmlAstReferenceVisitor::visit_text(session, node, from_module, scope, results, target);
        }
    }

    /// `<button name="method_x" type="object">` invokes a method on the current
    /// record's model. When the rename/find-refs target is a method (Function on
    /// a model class), match the button's `name` value against the method name
    /// AND verify the surrounding `record_model` matches the method's class model.
    fn visit_button<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, from_module: Option<ModuleKey>, scope: XmlScope<'a>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        let is_action_type = node.attribute("type") == Some("action");
        if !is_action_type {
            if let &ReferenceTarget::Symbol(SymbolKey::Function(target_fn)) = target {
                for attr in node.attributes() {
                    if attr.name() != "name" { continue; }
                    if session.st()[target_fn].name != attr.value() { continue; }
                    let target_class = session.st().get_in_parents(target_fn.into(), &[SymType::CLASS], true);
                    let Some(SymbolKey::Class(target_class)) = target_class else { continue; };
                    let Some(target_model) = session.st()[target_class]._model.as_ref() else { continue; };
                    let Some(record_model) = scope.record_model.filter(|m| !m.is_empty()) else { continue; };
                    if target_model.name == *record_model {
                        results.push(attr.range_value());
                    }
                }
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module, scope, results, target);
        }
    }

    fn visit_record<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, from_module: Option<ModuleKey>, mut scope: XmlScope<'a>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        for attr in node.attributes() {
            if attr.name() == "model" {
                scope.record_model = Some(attr.value());
                match target {
                    ReferenceTarget::String(s) => {
                        if attr.value() == s {
                            results.push(attr.range_value());
                        }
                    },
                    ReferenceTarget::Symbol(s) => {
                        if let &SymbolKey::Class(class_key) = s
                            && let Some(model) = &session.st()[class_key]._model
                                && model.name == attr.value() {
                                    results.push(attr.range_value());
                                }
                    }
                }
            } else if attr.name() == "id"
                && XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
        }
        if scope.record_model == Some("ir.ui.view") {
            scope.view_target_model = XmlAstUtils::view_target_model(node);
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module, scope, results, target);
        }
    }

    fn visit_field<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, from_module: Option<ModuleKey>, scope: XmlScope<'a>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        let mut child_scope = scope;
        for attr in node.attributes() {
            if attr.name() == "name" {
                child_scope.field_name = Some(attr.value());
                let &ReferenceTarget::Symbol(SymbolKey::Variable(target)) = target else {continue;};
                if !SymbolTable::is_field(session, target.into()) {continue;}
                if session.st()[target].name != attr.value() {continue;}
                //field name matches, but we still have to check model is the same
                let Some(model_name) = scope.record_model.filter(|m| !m.is_empty()) else {continue;};
                let field_model = session.st().get_in_parents(target.into(), &[SymType::CLASS], true);
                let Some(SymbolKey::Class(field_model)) = field_model else {continue;};
                let Some(model) = session.st()[field_model]._model.as_ref() else {continue;};
                if model.name == *model_name {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "ref"
                && XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
        }
        // Inside a view's `<field name="arch">`, sub-elements resolve against the
        // view's target model (captured at the ir.ui.view record), not the
        // surrounding ir.ui.view itself.
        if node.attribute("name") == Some("arch") {
            if let Some(target) = scope.view_target_model {
                child_scope.record_model = Some(target);
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module, child_scope, results, target);
        }
    }

    fn visit_text(session: &mut SessionInfo, node: &Node, _from_module: Option<ModuleKey>, scope: XmlScope, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        let (Some(_model), Some(field)) = (
            scope.record_model.filter(|m| !m.is_empty()),
            scope.field_name.filter(|f| !f.is_empty()),
        ) else {
            return;
        };
        if (field == "model" || field == "res_model")
            && let &ReferenceTarget::Symbol(SymbolKey::Class(target)) = target
                && let Some(model) = &session.st()[target]._model
                    && model.name == node.text().unwrap() {
                        results.push(node.range());
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

    fn visit_menu_item<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, from_module: Option<ModuleKey>, scope: XmlScope<'a>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
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
            } else if attr.name() == "parent"
                && XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module, scope, results, target);
        }
    }

    fn visit_template<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, from_module: Option<ModuleKey>, scope: XmlScope<'a>, results: &mut Vec<Range<usize>>, target: &ReferenceTarget) {
        for attr in node.attributes() {
            if matches!(attr.name(), "id" | "inherit_id") {
                if XmlAstReferenceVisitor::test_attr_as_xml_id(session.st(), &attr, from_module, target) {
                    results.push(attr.range_value());
                }
            } else if attr.name() == "groups" {
                //TODO
            }
        }
        for child in node.children() {
            XmlAstReferenceVisitor::visit_node(session, &child, from_module, scope, results, target);
        }
    }
}
