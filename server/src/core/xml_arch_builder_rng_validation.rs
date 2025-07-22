use std::{cell::RefCell, collections::HashMap, fmt, fs, path::PathBuf, rc::Rc};

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use once_cell::sync::Lazy;
use regex::Regex;
use roxmltree::Node;
use tracing::{error, warn};

use crate::{constants::{BuildStatus, BuildSteps, OYarn, EXTENSION_NAME}, core::xml_data::{XmlData, XmlDataActWindow, XmlDataDelete, XmlDataField, XmlDataMenuItem, XmlDataRecord, XmlDataReport, XmlDataTemplate}, oyarn, threads::SessionInfo, Sy, S};

use super::{file_mgr::FileInfo, odoo::SyncOdoo, symbols::{symbol::Symbol, xml_file_symbol::XmlFileSymbol}, xml_arch_builder::XmlArchBuilder};

static BINDING_VIEWS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([a-z]+(,[a-z]+)*)?$").unwrap());

/* Contains the RelaxNG Validation part of the XmlArchBuilder */
impl XmlArchBuilder {

    pub fn load_odoo_openerp_data(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        match node.tag_name().name() {
            "odoo" | "openerp" | "data" => {
                for attr in node.attributes() {
                    match attr.name() {
                        "noupdate" | "auto_sequence" | "uid" | "context" => {},
                        _ => {
                            diagnostics.push(Diagnostic::new(
                                Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                Some(DiagnosticSeverity::ERROR),
                                Some(lsp_types::NumberOrString::String(S!("OLS30400"))),
                                Some(EXTENSION_NAME.to_string()),
                                format!("Invalid attribute in {} node {}", attr.name(), node.tag_name().name()),
                                None,
                                None));
                        }
                    }
                }

                for child in node.children().filter(|n| n.is_element()) {
                    if !(self.load_odoo_openerp_data(session, &child, diagnostics)
                        || self.load_menuitem(session, &child, false, diagnostics)
                        || self.load_record(session, &child, diagnostics)
                        || self.load_template(session, &child, diagnostics)
                        || self.load_delete(session, &child, diagnostics)
                        || self.load_act_window(session, &child, diagnostics)
                        || self.load_report(session, &child, diagnostics)
                        || self.load_function(session, &child, diagnostics)
                        || child.is_text() || child.is_comment()) {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS30401"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("Invalid child node {} in {}", child.tag_name().name(), node.tag_name().name()),
                            None,
                            None));
                    }
                }
                return true;
            }
            _ => { return false;},
        }
    }

    fn load_menuitem(&mut self, session: &mut SessionInfo, node: &Node, is_submenu: bool, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "menuitem" { return false; }
        let mut found_id = None;
        let has_parent = node.attribute("parent").is_some();
        for attr in node.attributes() {
            match attr.name() {
                "id" => {
                    found_id = Some(attr.value().to_string());
                },
                "sequence" => {
                    if attr.value().parse::<i32>().is_err() {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS30404"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("Sequence attribute must be a string representing a number"),
                            None,
                            None));
                    }
                },
                "groups" => {
                    for group in attr.value().split(",") {
                        let group = group.trim_start_matches("-");
                        if self.get_group_ids(session, group, &attr, diagnostics).is_empty() {
                            diagnostics.push(Diagnostic::new(
                                Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                Some(DiagnosticSeverity::ERROR),
                                Some(lsp_types::NumberOrString::String(S!("OLS30449"))),
                                Some(EXTENSION_NAME.to_string()),
                                format!("Group with id '{}' does not exist", group),
                                None,
                                None));
                        }
                    }
                },
                "name" | "active" => {},
                "action" => {
                    if (has_parent || is_submenu) && node.has_children() {
                        let other_than_text = node.children().any(|c| !c.is_text() && !c.is_comment());
                        if other_than_text {
                            diagnostics.push(Diagnostic::new(
                                Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                Some(DiagnosticSeverity::ERROR),
                                Some(lsp_types::NumberOrString::String(S!("OLS30405"))),
                                Some(EXTENSION_NAME.to_string()),
                                format!("submenuitems are not allowed when Action attribute and parent are specified"),
                                None,
                                None));
                            continue;
                        }
                    }
                    //check that action exists
                    if session.sync_odoo.get_xml_ids(&self.xml_symbol, attr.value(), &attr.range(), diagnostics).is_empty() {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS30448"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("Action with id '{}' does not exist", attr.value()),
                            None,
                            None));
                    }
                }
                "parent" => {
                    if is_submenu {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS30408"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("parent attribute is not allowed in submenuitems"),
                            None,
                            None));
                    } else {
                        //check that parent exists
                        if session.sync_odoo.get_xml_ids(&self.xml_symbol, attr.value(), &attr.range(), diagnostics).is_empty() {
                            diagnostics.push(Diagnostic::new(
                                Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                Some(DiagnosticSeverity::ERROR),
                                Some(lsp_types::NumberOrString::String(S!("OLS30447"))),
                                Some(EXTENSION_NAME.to_string()),
                                format!("Parent menuitem with id '{}' does not exist", attr.value()),
                                None,
                                None));
                        }
                    }
                }
                "web_icon" => {
                    if has_parent || is_submenu {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS30406"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("web_icon attribute is not allowed when parent is specified"),
                            None,
                            None));
                    }
                }
                _ => {
                    diagnostics.push(Diagnostic::new(
                        Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                        Some(DiagnosticSeverity::ERROR),
                        Some(lsp_types::NumberOrString::String(S!("OLS30403"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("Invalid attribute {} in menuitem node", attr.name()),
                        None,
                        None));
                }
            }
        }
        if found_id.is_none() {
            diagnostics.push(Diagnostic::new(
                Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                Some(DiagnosticSeverity::ERROR),
                Some(lsp_types::NumberOrString::String(S!("OLS30402"))),
                Some(EXTENSION_NAME.to_string()),
                format!("menuitem node must contain an id attribute"),
                None,
                None));
        }
        for child in node.children().filter(|n| n.is_element()) {
            if child.tag_name().name() != "menuitem" {
                diagnostics.push(Diagnostic::new(
                    Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                    Some(DiagnosticSeverity::ERROR),
                    Some(lsp_types::NumberOrString::String(S!("OLS30407"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("Invalid child node {} in menuitem", child.tag_name().name()),
                    None,
                    None));
            }
            else {
                self.load_menuitem(session, &child, true, diagnostics);
            }
        }
        let data = XmlData::MENUITEM(XmlDataMenuItem {
            file_symbol: Rc::downgrade(&self.xml_symbol),
            xml_id: found_id.clone().map(|id| oyarn!("{}", id)),
        });
        self.on_operation_creation(session, found_id, node, data, diagnostics);
        true
    }

    fn load_record(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "record" { return false; }
        let mut found_model = false;
        let mut found_id = None;
        for attr in node.attributes() {
            match attr.name() {
                "id" => {found_id = Some(attr.value().to_string());},
                "forcecreate" => {},
                "model" => {found_model = true;},
                "uid" => {},
                "context" => {},
                _ => {
                    diagnostics.push(Diagnostic::new(
                        Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                        Some(DiagnosticSeverity::ERROR),
                        Some(lsp_types::NumberOrString::String(S!("OLS30409"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("Invalid attribute {} in record node", attr.name()),
                        None,
                        None));
                }
            }
        }

        if !found_model {
            diagnostics.push(Diagnostic::new(
                Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                Some(DiagnosticSeverity::ERROR),
                Some(lsp_types::NumberOrString::String(S!("OLS304010"))),
                Some(EXTENSION_NAME.to_string()),
                format!("record node must contain a model attribute"),
                None,
                None));
            return false;
        }
        let mut data = XmlDataRecord {
            file_symbol: Rc::downgrade(&self.xml_symbol),
            model: (oyarn!("{}", node.attribute("model").unwrap()), node.attribute_node("model").unwrap().range()),
            xml_id: found_id.clone().map(|id| oyarn!("{}", id)),
            fields: vec![],
            range: std::ops::Range::<usize> {
                start: node.range().start as usize,
                end: node.range().end as usize,
            }
        };
        for child in node.children().filter(|n| n.is_element()) {
            if let Some(field) = self.load_field(session, &child, diagnostics) {
                data.fields.push(field);
            } else {
                diagnostics.push(Diagnostic::new(
                    Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                    Some(DiagnosticSeverity::ERROR),
                    Some(lsp_types::NumberOrString::String(S!("OLS304011"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("Invalid child node {} in record. Only field node is allowed", child.tag_name().name()),
                    None,
                    None));
            }
        }
        let data = XmlData::RECORD(data);
        self.on_operation_creation(session, found_id, node, data, diagnostics);
        true
    }

    fn load_field(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> Option<XmlDataField> {
        if node.tag_name().name() != "field" { return None; }
        if node.attribute("name").is_none() {
            diagnostics.push(Diagnostic::new(
                Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                Some(DiagnosticSeverity::ERROR),
                Some(lsp_types::NumberOrString::String(S!("OLS304012"))),
                Some(EXTENSION_NAME.to_string()),
                format!("field node must contain a name attribute"),
                None,
                None));
         }

        let has_type = node.attribute("type").is_some();
        let has_ref = node.attribute("ref").is_some();
        let has_eval = node.attribute("eval").is_some();
        let has_search = node.attribute("search").is_some();
        if [has_type, has_ref, has_eval, has_search].iter().filter(|b| **b).count() > 1 {
            diagnostics.push(Diagnostic::new(
                Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                Some(DiagnosticSeverity::ERROR),
                Some(lsp_types::NumberOrString::String(S!("OLS304018"))),
                Some(EXTENSION_NAME.to_string()),
                format!("field node cannot have more than one of the attributes type, ref, eval or search"),
                None,
                None));
            return None;
        }
        let mut is_xml_or_html = false;
        if let Some(field_type) = node.attribute("type") {
            match field_type {
                "int" => {
                    let content = node.text().unwrap_or("");
                    if !(content.parse::<i32>().is_ok() || content == "None") {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304013"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("Invalid content for int field: {}", content),
                            None,
                            None));
                    }
                }
                "float" => {
                    let content = node.text().unwrap_or("");
                    if content.parse::<f64>().is_err() {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304014"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("Invalid content for float field: {}", content),
                            None,
                            None));
                    }
                }
                "list" | "tuple" => {
                    for child in node.children() {
                        if !self.load_value(session, &child, diagnostics) {
                            diagnostics.push(Diagnostic::new(
                                Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                                Some(DiagnosticSeverity::ERROR),
                                Some(lsp_types::NumberOrString::String(S!("OLS304015"))),
                                Some(EXTENSION_NAME.to_string()),
                                format!("Invalid child node {} in list/tuple field", child.tag_name().name()),
                                None,
                                None));
                        }
                    }
                }
                "html" | "xml" => {
                    is_xml_or_html = true;
                }
                "base64" | "char" | "file" => {
                    if node.has_attribute("file") {
                        if node.text().is_some() {
                            diagnostics.push(Diagnostic::new(
                                Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                                Some(DiagnosticSeverity::ERROR),
                                Some(lsp_types::NumberOrString::String(S!("OLS304017"))),
                                Some(EXTENSION_NAME.to_string()),
                                format!("text content is not allowed on a value that contains a file attribute"),
                                None,
                                None));
                        }
                    }
                }
                _ => {},
            }
        } 
        for attr in node.attributes() {
            match attr.name() {
                "name" | "type" | "file" => {},
                "ref" | "eval" | "search" => {
                    if node.text().is_some() {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304019"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("text content is not allowed on a field with {} attribute", attr.name()),
                            None,
                            None));
                    }
                },
                "model" => {
                    if !has_eval && !has_search {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304020"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("model attribute is not allowed on field node without eval or search attribute"),
                            None,
                            None));
                    }
                },
                "use" => {
                    if !has_search {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304021"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("use attribute is only allowed on field node with search attribute"),
                            None,
                            None));
                    }
                }
                _ => {
                    diagnostics.push(Diagnostic::new(
                        Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                        Some(DiagnosticSeverity::ERROR),
                        Some(lsp_types::NumberOrString::String(S!("OLS304016"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("Invalid attribute {} in field node", attr.name()),
                        None,
                        None));
                }
            }
        }
        for child in node.children() {
            if !self.load_record(session, &child, diagnostics) && !child.is_text() && !child.is_comment() && !is_xml_or_html {
                diagnostics.push(Diagnostic::new(
                    Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                    Some(DiagnosticSeverity::ERROR),
                    Some(lsp_types::NumberOrString::String(S!("OLS304022"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("Fields only allow 'record' children nodes"),
                    None,
                    None));
            }
        }
        let mut text = None;
        let mut text_range = None;
        for child in node.children() {
            if child.is_text() {
                text = child.text().map(|s| s.to_string());
                text_range = Some(child.range());
            }
        }
        Some(XmlDataField {
            name: oyarn!("{}", node.attribute("name").unwrap()),
            range: node.attribute_node("name").unwrap().range(),
            text: text,
            text_range: text_range,
        })
    }

    fn load_value(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "value" { return false; }
        let mut has_search = false;
        let mut has_eval = false;
        let has_type = node.has_attribute("type");
        for attr in node.attributes() {
            match attr.name() {
                "name" | "model" | "use" => {},
                "search" => {
                    has_search = true;
                    if has_eval || has_type {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304024"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("search attribute is not allowed when eval or type attribute is present"),
                            None,
                            None));
                    }
                },
                "eval" => {
                    has_eval = true;
                    if has_search || has_type {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304025"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("eval attribute is not allowed when search or type attribute is present"),
                            None,
                            None));
                    }
                },
                "type" => {
                    if has_search || has_eval {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304026"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("type attribute is not allowed when search or eval attribute is present"),
                            None,
                            None));
                    }
                    if node.has_attribute("file") && node.text().is_some() {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304027"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("text content is not allowed on a value that contains a file attribute"),
                            None,
                            None));

                    }
                },
                "file" => {
                    if !has_type {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304028"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("file attribute is only allowed on value node with type attribute"),
                            None,
                            None));
                    }
                }
                _ => {
                    diagnostics.push(Diagnostic::new(
                        Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                        Some(DiagnosticSeverity::ERROR),
                        Some(lsp_types::NumberOrString::String(S!("OLS304023"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("Invalid attribute {} in value node", attr.name()),
                        None,
                        None));
                }
            }
        }
        true
    }

    fn load_template(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "template" { return false; }
        //no interesting rule to check, as 'any' is valid
        let found_id = node.attribute("id").map(|s| s.to_string());
        let data = XmlData::TEMPLATE(XmlDataTemplate {
            file_symbol: Rc::downgrade(&self.xml_symbol),
            xml_id: found_id.clone().map(|id| oyarn!("{}", id)),
        });
        self.on_operation_creation(session, found_id, node, data, diagnostics);
        true
    }

    fn load_delete(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "delete" { return false; }
        if node.attribute("model").is_none() {
            diagnostics.push(Diagnostic::new(
                Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                Some(DiagnosticSeverity::ERROR),
                Some(lsp_types::NumberOrString::String(S!("OLS304029"))),
                Some(EXTENSION_NAME.to_string()),
                format!("delete node must contain a model attribute"),
                None,
                None));
        }
        let found_id = node.attribute("id").map(|s| s.to_string());
        let has_search = node.attribute("search").is_some();
        if found_id.is_some() && has_search {
            diagnostics.push(Diagnostic::new(
                Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                Some(DiagnosticSeverity::ERROR),
                Some(lsp_types::NumberOrString::String(S!("OLS304030"))),
                Some(EXTENSION_NAME.to_string()),
                format!("delete node cannot have both id and search attributes"),
                None,
                None));
        }
        if found_id.is_none() && !has_search {
            diagnostics.push(Diagnostic::new(
                Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                Some(DiagnosticSeverity::ERROR),
                Some(lsp_types::NumberOrString::String(S!("OLS304031"))),
                Some(EXTENSION_NAME.to_string()),
                format!("delete node must have either id or search attribute"),
                None,
                None));
        }
        let data = XmlData::DELETE(XmlDataDelete {
            file_symbol: Rc::downgrade(&self.xml_symbol),
            xml_id: found_id.clone().map(|id| oyarn!("{}", id)),
            model: Sy!(node.attribute("model").unwrap().to_string()),
        });
        self.on_operation_creation(session, found_id, node, data, diagnostics);
        true
    }

    fn load_act_window(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "act_window" { return false; }
        let mut found_id = None;
        for attr in ["id", "name", "res_model"] {
            if node.attribute(attr).is_none() {
                diagnostics.push(Diagnostic::new(
                    Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    Some(DiagnosticSeverity::ERROR),
                    Some(lsp_types::NumberOrString::String(S!("OLS304032"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("act_window node must contain a {} attribute", attr),
                    None,
                    None));
            }
            if attr == "id" {
                found_id = Some(node.attribute(attr).unwrap().to_string());
            }
        }
        for attr in node.attributes() {
            match attr.name() {
                "id" | "name" | "res_model" => {},
                "domain" | "view_mode" | "view_id" | "target" | "context" | "groups" | "limit" | "usage" | "binding_model" => {},
                "binding_type" => {
                    if attr.value() != "action" && attr.value() != "report" {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304034"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("binding_type attribute must be either 'action' or 'report', found {}", attr.value()),
                            None,
                            None));
                    }
                },
                "binding_views" => {
                    if !BINDING_VIEWS_RE.is_match(attr.value()) {
                        diagnostics.push(Diagnostic::new(
                            Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            Some(DiagnosticSeverity::ERROR),
                            Some(lsp_types::NumberOrString::String(S!("OLS304035"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("binding_views attribute must be a comma-separated list of view types matching ^([a-z]+(,[a-z]+)*)?$, found {}", attr.value()),
                            None,
                            None));
                    }
                },
                _ => {
                    diagnostics.push(Diagnostic::new(
                        Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                        Some(DiagnosticSeverity::ERROR),
                        Some(lsp_types::NumberOrString::String(S!("OLS304033"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("Invalid attribute {} in act_window node", attr.name()),
                        None,
                        None));
                }
            }
        }
        if node.text().is_some() {
            diagnostics.push(Diagnostic::new(
                Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                Some(DiagnosticSeverity::ERROR),
                Some(lsp_types::NumberOrString::String(S!("OLS304033"))),
                Some(EXTENSION_NAME.to_string()),
                format!("act_window node cannot have text content"),
                None,
                None));
        }
        let data = XmlData::ACT_WINDOW(XmlDataActWindow {
            file_symbol: Rc::downgrade(&self.xml_symbol),
            xml_id: found_id.clone().map(|id| oyarn!("{}", id)),
            res_model: Sy!(node.attribute("res_model").unwrap().to_string()),
            name: Sy!(node.attribute("name").unwrap().to_string()),
        });
        self.on_operation_creation(session, found_id, node, data, diagnostics);
        true
    }

    fn load_report(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "report" { return false; }
        let mut found_id = None;
        for attr in ["string", "model", "name"] {
            if node.attribute(attr).is_none() {
                diagnostics.push(Diagnostic::new(
                    Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    Some(DiagnosticSeverity::ERROR),
                    Some(lsp_types::NumberOrString::String(S!("OLS304036"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("report node must contain a {} attribute", attr),
                    None,
                    None));
            }
        }
        for attr in node.attributes() {
            match attr.name() {
                "id" => { found_id = Some(attr.value().to_string()); },
                "print_report_name" | "report_type" | "multi"| "menu" | "keyword" | "file" |
                "xml" | "parser" | "auto" | "header" | "attachment" | "attachment_use" | "groups" | "paperformat" | "usage" => {},
                _ => {
                    diagnostics.push(Diagnostic::new(
                        Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                        Some(DiagnosticSeverity::ERROR),
                        Some(lsp_types::NumberOrString::String(S!("OLS304037"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("Invalid attribute {} in report node", attr.name()),
                        None,
                        None));
                }
            }
        }
        if node.text().is_some() {
            diagnostics.push(Diagnostic::new(
                Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                Some(DiagnosticSeverity::ERROR),
                Some(lsp_types::NumberOrString::String(S!("OLS304038"))),
                Some(EXTENSION_NAME.to_string()),
                format!("report node cannot have text content"),
                None,
                None));
        }
        let data = XmlData::REPORT(XmlDataReport {
            file_symbol: Rc::downgrade(&self.xml_symbol),
            xml_id: found_id.clone().map(|id| oyarn!("{}", id)),
            name: Sy!(node.attribute("name").unwrap().to_string()),
            model: Sy!(node.attribute("model").unwrap().to_string()),
            string: Sy!(node.attribute("string").unwrap().to_string()),
        });
        self.on_operation_creation(session, found_id, node, data, diagnostics);
        true
    }

    fn load_function(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "function" { return false; }
        for attr in ["model", "name"] {
            if node.attribute(attr).is_none() {
                diagnostics.push(Diagnostic::new(
                    Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    Some(DiagnosticSeverity::ERROR),
                    Some(lsp_types::NumberOrString::String(S!("OLS304039"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("function node must contain a {} attribute", attr),
                    None,
                    None));
            }
        }
        let mut has_eval = false;
        for attr in node.attributes() {
            match attr.name() {
                "model" | "name" => {},
                "uid" => {},
                "context" => {}
                "eval" => {
                    has_eval = true;
                }
                _ => {
                    diagnostics.push(Diagnostic::new(
                        Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                        Some(DiagnosticSeverity::ERROR),
                        Some(lsp_types::NumberOrString::String(S!("OLS304041"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("Invalid attribute {} in function node", attr.name()),
                        None,
                        None));
                }
            }
        }
        for child in node.children().filter(|n| n.is_element()) {
            if self.load_value(session, &child, diagnostics) {
                if has_eval {
                    diagnostics.push(Diagnostic::new(
                        Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                        Some(DiagnosticSeverity::ERROR),
                        Some(lsp_types::NumberOrString::String(S!("OLS304040"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("function node cannot have value children when eval attribute is present"),
                        None,
                        None));
                }
            } else if self.load_function(session, &child, diagnostics) {
                if has_eval {
                    diagnostics.push(Diagnostic::new(
                        Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                        Some(DiagnosticSeverity::ERROR),
                        Some(lsp_types::NumberOrString::String(S!("OLS304042"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("function node cannot have function children when eval attribute is present"),
                        None,
                        None));
                }
            } else {
                diagnostics.push(Diagnostic::new(
                    Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                    Some(DiagnosticSeverity::ERROR),
                    Some(lsp_types::NumberOrString::String(S!("OLS304043"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("Invalid child node {} in function node", child.tag_name().name()),
                    None,
                    None));
            }
        }
        true
    }
}