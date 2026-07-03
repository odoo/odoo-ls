use crate::{
    S, core::{
        evaluation_context::ContextValue, js_arch_builder::ComponentMember, odoo::SyncOdoo, symbols::{
            ModuleSymbol, symbol_keys::{ModuleKey, SourceFileKey, SymbolKey, XmlId}
        }
    }, threads::SessionInfo
};
use roxmltree::Node;
use ruff_text_size::{TextRange, TextSize};
use std::{ops::Range};
use crate::utils::HashMap;

/// A variable declared by a `t-set` or `t-as` directive in an OWL template.
#[derive(Clone)]
pub struct TemplateVarDecl {
    name: String,
    /// Range in the source file where the variable name starts (inside the attribute value).
    pub range: TextRange,
}

/// Context returned when the cursor lands inside an OWL directive attribute value.
#[derive(Clone)]
pub struct OWLAttrCtx {
    pub attr_name: String,
    pub attr_value: String,
    /// Byte offset of the opening quote of the attribute value in the source file.
    pub attr_value_start: usize,
    pub template_name: String,
    /// Byte range within `attr_value` of the word under the cursor.
    pub word_range: std::ops::Range<usize>,
    /// Template variables in scope at the cursor position.
    pub template_vars: Vec<TemplateVarDecl>,
}

// classe used to store data during node traversal
#[derive(Clone)]
struct OwlXmlAstUtilsContext {
    pub current_template: Option<String>, //frontend template we are currently visiting
    pub owl_attr_ctx: Option<OWLAttrCtx>,
    pub inherited_vars: Vec<TemplateVarDecl>,
}

pub enum GetSymbolsResult {
    SymbolKey(SymbolKey),
    ComponentMember{component_member: ComponentMember, uri: String}, //owlResult handle all results from attributes specific to owl: t-foreach,etc..
    TemplateVarDecl(TemplateVarDecl),
}

pub struct XmlAstUtils {}

impl XmlAstUtils {

    pub fn get_symbols(session: &mut SessionInfo, file_symbol: SourceFileKey, root: roxmltree::Node, offset: usize, on_dep_only: bool) -> (Vec<GetSymbolsResult>, Option<Range<usize>>) {
        let mut results = (vec![], None);
        let from_module = session.sync_odoo.symbol_table.find_module(file_symbol);
        let mut context_xml = HashMap::default();
        let mut traversal_context = OwlXmlAstUtilsContext {
            current_template: None,
            owl_attr_ctx: None,
            inherited_vars: vec![],
        };
        for node in root.children() {
            XmlAstUtils::visit_node(session, &node, offset, from_module, &mut traversal_context, &mut context_xml, &mut results, on_dep_only);
        }

        results
    }

    fn visit_node(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, traversal_context: &mut OwlXmlAstUtilsContext, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<GetSymbolsResult>, Option<Range<usize>>), on_dep_only: bool) {
        if node.range().start > offset {
            return;
        }
        let detected_frontend_template = node.attribute("t-name")
            .map(|s| s.to_string());
        if let Some(template_name) = &detected_frontend_template {
            traversal_context.inherited_vars.clear(); //reset any previous OWL attribute context when entering a new template
            traversal_context.current_template = Some(template_name.clone());
        }
        if node.is_element() {
            match node.tag_name().name()  {
                "record" => {
                    XmlAstUtils::visit_record(session, &node, offset, from_module, traversal_context, ctxt, results, on_dep_only);
                }
                "field" => {
                    XmlAstUtils::visit_field(session, &node, offset, from_module, traversal_context, ctxt, results, on_dep_only);
                },
                "menuitem" => {
                    XmlAstUtils::visit_menu_item(session, &node, offset, from_module, traversal_context, ctxt, results, on_dep_only);
                },
                "template" => {
                    XmlAstUtils::visit_template(session, &node, offset, from_module, traversal_context, ctxt, results, on_dep_only);
                }
                _ => {
                    XmlAstUtils::visit_attributes_for_owl(session, &node, offset, from_module, traversal_context, ctxt, results, on_dep_only);
                    if traversal_context.owl_attr_ctx.is_some() {
                        return; //no need to go deeper
                    }
                    for child in node.children() {
                        XmlAstUtils::visit_node(session, &child, offset, from_module, traversal_context, ctxt, results, on_dep_only);
                    }
                }
            }
        } else if node.is_text() {
            XmlAstUtils::visit_text(session, &node, offset, from_module, traversal_context, ctxt, results, on_dep_only);
        }
        if detected_frontend_template.is_some() {
            traversal_context.current_template = None;
        }
    }

    fn visit_attributes_for_owl(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, traversal_context: &mut OwlXmlAstUtilsContext, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<GetSymbolsResult>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            let attr_range = attr.range_value();
            if attr_range.start <= offset && offset <= attr_range.end {
                let attr_name = attr.name().to_string();
                let Some(ref tmpl) = traversal_context.current_template else { return; };
                let attr_value = attr.value().to_string();
                let pos_in_val = offset.saturating_sub(attr_range.start + 1);
                let Some(word_range) = Self::word_range_at(&attr_value, pos_in_val) else { return;};
                let word = &attr_value[word_range.clone()];
                if word.is_empty() {
                    return;
                }
                results.1 = Some(attr_range.start + word_range.start..attr_range.start + word_range.end);
                // Try to find Js Class member
                let v  = session.sync_odoo.js_component_by_template.get(tmpl);
                if let Some(class_name) = session.sync_odoo.js_component_by_template.get(tmpl).cloned() {
                    if let Some(descriptor) = session.sync_odoo.component_descriptors.get(&class_name).cloned() {
                        if let Some(member) = descriptor.find_member(word) {
                            results.0 = vec![GetSymbolsResult::ComponentMember{component_member: member.clone(), uri: descriptor.file_path.clone()}];
                        }
                    }
                }
                // --- Try template-local variable (t-set / t-as) ---
                let Some(var_decl) = traversal_context.inherited_vars.iter().find(|v| v.name == word) else {
                    return;
                };
                results.0 = vec![GetSymbolsResult::TemplateVarDecl(var_decl.clone())];
                return;
            }
        }
        //we didn't find any attribute containing the offset, so we add data from useful for children evaluation
        if let Some(t_as_val) = node.attribute("t-as") {
            if node.attribute("t-foreach").is_some() {
                if let Some(t_as_attr) = node.attributes().find(|a| a.name() == "t-as") {
                    traversal_context.inherited_vars.push(TemplateVarDecl {
                        name: t_as_val.to_string(),
                        range: TextRange::new(
                            TextSize::new(t_as_attr.range_value().start as u32 + 1),
                            TextSize::new(t_as_attr.range_value().end as u32 - 1)
                        ),
                    });
                }
            }
        }
        if let Some(t_set_val) = node.attribute("t-set") {
            if let Some(t_set_attr) = node.attributes().find(|a| a.name() == "t-set") {
                traversal_context.inherited_vars.push(TemplateVarDecl {
                    name: t_set_val.to_string(),
                    range: TextRange::new(
                        TextSize::new(t_set_attr.range_value().start as u32 + 1),
                        TextSize::new(t_set_attr.range_value().end as u32 - 1)
                    ),
                });
            }
        }
    }

    /// Return the byte range of the identifier word at `pos` within `text`.
    fn word_range_at(text: &str, pos: usize) -> Option<std::ops::Range<usize>> {
        if pos > text.len() {
            return None;
        }
        let start = text[..pos]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = text[pos..]
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| pos + i)
            .unwrap_or(text.len());
        if start == end {
            return None;
        }
        Some(start..end)
    }

    fn visit_record(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, traversal_context: &mut OwlXmlAstUtilsContext, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<GetSymbolsResult>, Option<Range<usize>>), on_dep_only: bool) {
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
                            model.borrow().all_symbols(
                                session,
                                from_module,
                                false)
                            .iter().filter(|s| s.1.is_none())
                            .map(|s| GetSymbolsResult::SymbolKey(SymbolKey::from(s.0))));
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
            XmlAstUtils::visit_node(session, &child, offset, from_module, traversal_context, ctxt, results, on_dep_only);
        }
        ctxt.remove("record_model");
    }

    fn visit_field(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, traversal_context: &mut OwlXmlAstUtilsContext, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<GetSymbolsResult>, Option<Range<usize>>), on_dep_only: bool) {
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
                                for symbol in content.symbols {
                                    results.0.push(GetSymbolsResult::SymbolKey(symbol));
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
            XmlAstUtils::visit_node(session, &child, offset, from_module, traversal_context, ctxt, results, on_dep_only);
        }
        ctxt.remove("field_name");
    }

    fn visit_text(session: &mut SessionInfo, node: &Node, offset: usize, from_module: Option<ModuleKey>, traversal_context: &mut OwlXmlAstUtilsContext, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<GetSymbolsResult>, Option<Range<usize>>), on_dep_only: bool) {
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

    fn visit_menu_item(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, traversal_context: &mut OwlXmlAstUtilsContext, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<GetSymbolsResult>, Option<Range<usize>>), on_dep_only: bool) {
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
            XmlAstUtils::visit_node(session, &child, offset, from_module, traversal_context, ctxt, results, on_dep_only);
        }
    }

    fn visit_template(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<ModuleKey>, traversal_context: &mut OwlXmlAstUtilsContext, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<GetSymbolsResult>, Option<Range<usize>>), on_dep_only: bool) {
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
            XmlAstUtils::visit_node(session, &child, offset, from_module, traversal_context, ctxt, results, on_dep_only);
        }
    }

    fn add_model_result(session: &mut SessionInfo, node: &Node, from_module: Option<ModuleKey>, results: &mut (Vec<GetSymbolsResult>, Option<Range<usize>>), on_dep_only: bool) {
        if let Some(model) = session.sync_odoo.models.get(node.text().unwrap()).cloned() {
            let from_module = match on_dep_only {
                true => from_module,
                false => None,
            };
            results.0.extend(model.borrow().all_symbols(session, from_module, false).iter().filter(|s| s.1.is_none()).map(|s| GetSymbolsResult::SymbolKey(SymbolKey::from(s.0))));
            results.1 = Some(node.range());
        }
    }

    fn add_xml_id_result(session: &mut SessionInfo, xml_id: &str, file_symbol: SourceFileKey, range: Range<usize>, results: &mut (Vec<GetSymbolsResult>, Option<Range<usize>>), on_dep_only: bool) {
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
                results.0.push(GetSymbolsResult::SymbolKey(record_key.into()));
            } else if let XmlId::PythonClass(record_key) = xml_id {
                results.0.push(GetSymbolsResult::SymbolKey(record_key.into()));
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
