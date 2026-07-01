use crate::{
    core::{
        model::Model,
        odoo::SyncOdoo,
        symbols::{
            ModuleSymbol,
            storage::xml::xml_field_symbol::XmlFieldName,
            symbol_keys::{ModuleKey, SourceFileKey, SymbolKey, XmlId},
        },
    },
    threads::SessionInfo,
};
use roxmltree::{Attribute, Node};
use std::ops::Range;

/// Inherited state threaded top-down through the XML walk (replacing a
/// string-keyed context map). Both fields borrow the parsed document, so the
/// scope is `Copy` and each visitor just hands a tweaked copy to its children.
#[derive(Clone, Copy, Default)]
pub struct XmlScope<'a> {
    /// Model the surrounding `<record>`/arch subtree resolves fields against.
    pub record_model: Option<&'a str>,
    /// `name` of the enclosing `<field>` (drives `<field name="model">` text).
    pub field_name: Option<&'a str>,
    /// For an `ir.ui.view` record, the model its arch targets (captured from the
    /// record's `<field name="model">`). Applied to the `<field name="arch">` subtree.
    pub view_target_model: Option<&'a str>,
}

pub struct XmlAstUtils {}

impl XmlAstUtils {

    pub fn get_symbols(session: &mut SessionInfo, file_symbol: SourceFileKey, root: roxmltree::Node, offset: usize, on_dep_only: bool) -> (Vec<SymbolKey>, Option<Range<usize>>) {
        let mut results = (vec![], None);
        let from_module = session.sync_odoo.symbol_table.find_module(file_symbol);
        for node in root.children() {
            XmlAstUtils::visit_node(session, &node, offset, from_module, XmlScope::default(), &mut results, on_dep_only);
        }
        results
    }

    fn visit_node<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, offset: usize, from_module: Option<ModuleKey>, scope: XmlScope<'a>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        if node.range().start > offset {
            return;
        }
        if node.is_element() {
            XmlAstUtils::scan_format_xml_id_under_cursor(session, node, offset, from_module, results, on_dep_only);
            match node.tag_name().name()  {
                "record" => {
                    XmlAstUtils::visit_record(session, node, offset, from_module, scope, results, on_dep_only);
                }
                "field" => {
                    XmlAstUtils::visit_field(session, node, offset, from_module, scope, results, on_dep_only);
                },
                "menuitem" => {
                    XmlAstUtils::visit_menu_item(session, node, offset, from_module, scope, results, on_dep_only);
                },
                "template" => {
                    XmlAstUtils::visit_template(session, node, offset, from_module, scope, results, on_dep_only);
                }
                "button" => {
                    XmlAstUtils::visit_button(session, node, offset, from_module, scope, results, on_dep_only);
                }
                _ => {
                    for child in node.children() {
                        XmlAstUtils::visit_node(session, &child, offset, from_module, scope, results, on_dep_only);
                    }
                }
            }
        } else if node.is_text() {
            XmlAstUtils::visit_text(session, node, offset, from_module, scope, results, on_dep_only);
        }
    }

    fn visit_button<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, offset: usize, from_module: Option<ModuleKey>, scope: XmlScope<'a>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        // Implicit `type` is `object` in views; only `type="action"` puts us in
        // the xml-id case (handled by scan_format_xml_id_under_cursor).
        let is_action_type = node.attribute("type") == Some("action");
        for attr in node.attributes() {
            let in_range = attr.range_value().start <= offset && attr.range_value().end >= offset;
            if !in_range { continue; }
            if attr.name() == "name" && !is_action_type {
                let Some(model_name) = scope.record_model.filter(|m| !m.is_empty()) else { continue };
                let found = XmlAstUtils::resolve_member_on_model(session, model_name, attr.value(), from_module, on_dep_only);
                if !found.is_empty() {
                    results.0.extend(found);
                    results.1 = Some(attr.range_value());
                }
            } else if attr.name() == "groups" {
                if let Some(file_module) = from_module {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), file_module.into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module, scope, results, on_dep_only);
        }
    }

    /// Find a member named `name` on every class registered for `model_name`,
    /// honoring `_inherit` chains. Returns the concrete Variable/Function
    /// symbols. Drives goto-def / hover for `<field name="X"/>` and
    /// `<button name="X"/>` resolution.
    fn resolve_member_on_model(session: &mut SessionInfo, model_name: &str, member_name: &str, from_module: Option<ModuleKey>, on_dep_only: bool) -> Vec<SymbolKey> {
        let mut out = Vec::new();
        let Some(model) = session.sync_odoo.models.get(model_name).cloned() else { return out };
        let from_module = if on_dep_only { from_module } else { None };
        for class_key in Model::get_full_model_classes(model.clone(), session, from_module) {
            let content = session.st().get_content_symbol(class_key.into(), member_name, u32::MAX);
            out.extend(content.symbols);
        }
        let model_ref = model.borrow();
        for xml_record_key in model_ref.get_xml_model_field_symbols(session.st(), from_module) {
            let field_name = session.st()[xml_record_key].get_field_text(XmlFieldName::Name, session.st());
            if field_name.as_deref() == Some(member_name) {
                out.push(xml_record_key.into());
            }
        }
        out
    }

    fn visit_record<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, offset: usize, from_module: Option<ModuleKey>, mut scope: XmlScope<'a>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "model" {
                scope.record_model = Some(attr.value());
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    if let Some(model) = session.sync_odoo.models.get(attr.value()).cloned() {
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
            } else if attr.name() == "id"
                && attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
        }
        if scope.record_model == Some("ir.ui.view") {
            scope.view_target_model = XmlAstUtils::view_target_model(node);
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module, scope, results, on_dep_only);
        }
    }

    fn visit_field<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, offset: usize, from_module: Option<ModuleKey>, scope: XmlScope<'a>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        let mut child_scope = scope;
        for attr in node.attributes() {
            if attr.name() == "name" {
                child_scope.field_name = Some(attr.value());
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    if let Some(model_name) = scope.record_model.filter(|m| !m.is_empty()) {
                        let found = XmlAstUtils::resolve_member_on_model(session, model_name, attr.value(), from_module, on_dep_only);
                        if !found.is_empty() {
                            results.0.extend(found);
                            results.1 = Some(attr.range_value());
                        }
                    }
                }
            } else if attr.name() == "ref"
                && attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
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
            XmlAstUtils::visit_node(session, &child, offset, from_module, child_scope, results, on_dep_only);
        }
    }

    fn visit_text(session: &mut SessionInfo, node: &Node, offset: usize, from_module: Option<ModuleKey>, scope: XmlScope, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        if node.range().start <= offset && node.range().end >= offset {
            let (Some(_model), Some(field)) = (
                scope.record_model.filter(|m| !m.is_empty()),
                scope.field_name.filter(|f| !f.is_empty()),
            ) else {
                return;
            };
            if field == "model" || field == "res_model" { //do not check model, let's assume it will contains a model name
                XmlAstUtils::add_model_result(session, node, from_module, results, on_dep_only);
            }
        }
    }

    fn visit_menu_item<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, offset: usize, from_module: Option<ModuleKey>, scope: XmlScope<'a>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "action" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            } else if attr.name() == "groups"
                && attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module, scope, results, on_dep_only);
        }
    }

    fn visit_template<'a>(session: &mut SessionInfo<'_>, node: &Node<'a, '_>, offset: usize, from_module: Option<ModuleKey>, scope: XmlScope<'a>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "inherit_id" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            } else if attr.name() == "groups"
                && attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module, scope, results, on_dep_only);
        }
    }

    fn scan_format_xml_id_under_cursor(session: &mut SessionInfo, node: &Node, offset: usize, from_module: Option<ModuleKey>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        let Some(file_module) = from_module else { return };
        for attr in node.attributes() {
            let attr_range = attr.range_value();
            if offset < attr_range.start || offset > attr_range.end { continue; }
            let mut hit: Option<(String, Range<usize>)> = None;
            XmlAstUtils::for_each_format_xml_id_ref(&attr, |inner, range| {
                if hit.is_some() { return; }
                if range.start <= offset && offset <= range.end {
                    hit = Some((inner.to_string(), range));
                }
            });
            if let Some((inner, range)) = hit {
                XmlAstUtils::add_xml_id_result(session, &inner, file_module.into(), range.clone(), results, on_dep_only);
                results.1 = Some(range);
                return;
            }
        }
    }

    /// For each `%(xml_id)d|s|i` format-string reference in `attr`'s value, invoke
    /// `f` with the inner xml-id and its absolute byte range (excluding the
    /// `%(` and `)X` wrapper). Used for `<button name="%(...)d">` style action refs.
    pub fn for_each_format_xml_id_ref(attr: &Attribute, mut f: impl FnMut(&str, Range<usize>)) {
        let value = attr.value();
        let bytes = value.as_bytes();
        let attr_start = attr.range_value().start;
        let mut i = 0;
        while i + 3 < bytes.len() {
            if bytes[i] == b'%' && bytes[i + 1] == b'(' {
                if let Some(close_off) = bytes[i + 2..].iter().position(|&b| b == b')') {
                    let inner_start = i + 2;
                    let inner_end = i + 2 + close_off;
                    let after = inner_end + 1;
                    if after < bytes.len() && matches!(bytes[after], b'd' | b's' | b'i') {
                        f(&value[inner_start..inner_end], attr_start + inner_start..attr_start + inner_end);
                        i = after + 1;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    /// For an `ir.ui.view` record, the model its arch targets, read from the
    /// record's direct `<field name="model">…</field>` child.
    pub fn view_target_model<'a>(record: &Node<'a, '_>) -> Option<&'a str> {
        for child in record.children() {
            if child.is_element()
                && child.tag_name().name() == "field"
                && child.attribute("name") == Some("model")
            {
                if let Some(text) = child.text() {
                    let model = text.trim();
                    if !model.is_empty() {
                        return Some(model);
                    }
                }
            }
        }
        None
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
            if on_dep_only
                && let Some(module) = session.st().find_module(xml_id)
                    && !ModuleSymbol::is_in_deps(
                        session.st(),
                        session.st().find_module(file_symbol).unwrap(),
                        &session.st()[module].name,
                    ) {
                        continue;
                    }
            if let XmlId::XmlRecord(record_key) = xml_id {
                results.0.push(record_key.into());
            } else if let XmlId::PythonClass(record_key) = xml_id {
                results.0.push(record_key.into());
            }
        }
    }

    /**
     * Clear invalid weak values from js_templates for this template name.
     * Return true if there is still valid values after the cleanup
     */
    pub fn ensure_js_template_validity(session: &mut SessionInfo, t_name: &str) -> bool {
        let Some(templates) = session.sync_odoo.js_templates.get(t_name) else {
            return false;
        };
        if templates.is_empty(&session.sync_odoo.symbol_table) {
            session.sync_odoo.js_templates.remove(t_name);
            return false;
        }
        true
    }

}
