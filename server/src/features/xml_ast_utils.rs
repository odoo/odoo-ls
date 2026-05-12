use crate::utils::HashMap;
use crate::{
    S,
    core::{
        evaluation_context::ContextValue,
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
        if node.range().start > offset {
            return;
        }
        if node.is_element() {
            XmlAstUtils::scan_format_xml_id_under_cursor(session, node, offset, from_module, results, on_dep_only);
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
                "button" => {
                    XmlAstUtils::visit_button(session, &node, offset, from_module, ctxt, results, on_dep_only);
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

    fn visit_button(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        // Implicit `type` is `object` in views; only `type="action"` puts us in
        // the xml-id case (handled by scan_format_xml_id_under_cursor).
        let is_action_type = node.attribute("type") == Some("action");
        for attr in node.attributes() {
            let in_range = attr.range_value().start <= offset && attr.range_value().end >= offset;
            if !in_range { continue; }
            if attr.name() == "name" && !is_action_type {
                let model_name = ctxt.get("record_model").map(ContextValue::as_str).unwrap_or_default().to_string();
                if model_name.is_empty() { continue; }
                let found = XmlAstUtils::resolve_member_on_model(session, &model_name, attr.value(), from_module, on_dep_only);
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
            XmlAstUtils::visit_node(session, &child, offset, from_module, ctxt, results, on_dep_only);
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
                    let model_name = ctxt.get("record_model").map(ContextValue::as_str).unwrap_or_default().to_string();
                    if !model_name.is_empty() {
                        let found = XmlAstUtils::resolve_member_on_model(session, &model_name, attr.value(), from_module, on_dep_only);
                        if !found.is_empty() {
                            results.0.extend(found);
                            results.1 = Some(attr.range_value());
                        }
                    }
                }
            } else if attr.name() == "ref" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), from_module.unwrap().into(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            }
        }
        let arch_model = XmlAstUtils::arch_field_target_model(node);
        let prev_record_model = arch_model.as_ref()
            .map(|m| ctxt.insert(S!("record_model"), ContextValue::STRING(m.clone())));
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module, ctxt, results, on_dep_only);
        }
        if arch_model.is_some() {
            match prev_record_model.flatten() {
                Some(prev) => { ctxt.insert(S!("record_model"), prev); }
                None => { ctxt.remove("record_model"); }
            }
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

    /// If `node` is `<field name="arch">`, return the text of its sibling
    /// `<field name="model">…</field>` (the view's target model).
    pub fn arch_field_target_model(node: &Node) -> Option<String> {
        if node.attribute("name") != Some("arch") {
            return None;
        }
        let parent = node.parent()?;
        for sibling in parent.children() {
            if sibling.is_element()
                && sibling.tag_name().name() == "field"
                && sibling.attribute("name") == Some("model")
            {
                let model = sibling.text()?.trim();
                if !model.is_empty() {
                    return Some(model.to_string());
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
