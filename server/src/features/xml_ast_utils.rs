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

    /// `<button name="method_x" type="object">` invokes `method_x` on the current
    /// record's model. Implicit `type` is `object` in views, so we resolve `name`
    /// against `record_model` whenever there's no `type="action"` (which would put
    /// us in the xml-id case already handled by scan_format_xml_id_under_cursor /
    /// the existing %(...)d path).
    fn visit_button(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        let is_action_type = node.attribute("type") == Some("action");
        for attr in node.attributes() {
            if attr.name() == "name" && !is_action_type {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    let model_name = ctxt.get("record_model").map(ContextValue::as_str).unwrap_or_default();
                    if model_name.is_empty() { continue; }
                    if let Some(model) = session.sync_odoo.models.get(model_name).cloned() {
                        let from_module = match on_dep_only { true => from_module, false => None };
                        for (class_key, missing_dep) in model.borrow().all_symbols(session, from_module, true) {
                            if missing_dep.is_none() {
                                let content = session.sync_odoo.symbol_table.get_content_symbol(class_key.into(), attr.value(), u32::MAX);
                                for symbol in content.symbols {
                                    results.0.push(symbol);
                                }
                            }
                        }
                        results.1 = Some(attr.range_value());
                    }
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
        // Inside a `<field name="arch">...</field>`, sub-elements reference fields/methods
        // on the *view's target* model, not on the surrounding record's model (typically
        // ir.ui.view / ir.actions.act_window). Look at sibling `<field name="model">X</field>`
        // to pick up the right model for the arch subtree.
        let arch_model = XmlAstUtils::pick_arch_target_model(node);
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

    /// If `node` is `<field name="arch">`, return the text of its sibling
    /// `<field name="model">…</field>` (the view's target model).
    fn pick_arch_target_model(node: &Node) -> Option<String> {
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

    /// Detect a `%(xml_id)d` / `%(xml_id)s` / `%(xml_id)i` format reference
    /// in any attribute value on `node` and, if the cursor sits inside the
    /// inner xml-id, resolve it like a normal xml-id reference (powering
    /// goto-def / hover on `<button name="%(...)d">` and similar).
    fn scan_format_xml_id_under_cursor(session: &mut SessionInfo, node: &Node, offset: usize, from_module: Option<ModuleKey>, results: &mut (Vec<SymbolKey>, Option<Range<usize>>), on_dep_only: bool) {
        let Some(file_module) = from_module else { return };
        for attr in node.attributes() {
            let value = attr.value();
            let bytes = value.as_bytes();
            let attr_start = attr.range_value().start;
            let mut i = 0;
            while i + 3 < bytes.len() {
                if bytes[i] == b'%' && bytes[i+1] == b'(' {
                    if let Some(close_off) = bytes[i+2..].iter().position(|&b| b == b')') {
                        let inner_start = i + 2;
                        let inner_end = i + 2 + close_off;
                        let after = inner_end + 1;
                        if after < bytes.len() && matches!(bytes[after], b'd' | b's' | b'i') {
                            let abs_start = attr_start + inner_start;
                            let abs_end = attr_start + inner_end;
                            if abs_start <= offset && offset <= abs_end {
                                let inner = &value[inner_start..inner_end];
                                XmlAstUtils::add_xml_id_result(session, inner, file_module.into(), abs_start..abs_end, results, on_dep_only);
                                results.1 = Some(abs_start..abs_end);
                                return;
                            }
                            i = after + 1;
                            continue;
                        }
                    }
                }
                i += 1;
            }
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
