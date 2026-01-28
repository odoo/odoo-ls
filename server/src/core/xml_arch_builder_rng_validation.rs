use std::rc::Rc;

use lsp_types::{Diagnostic, Position, Range};
use roxmltree::Node;

use crate::{constants::OYarn, core::{diagnostics::{create_diagnostic, DiagnosticCode}, odoo::SyncOdoo, xml_data::{OdooData, XmlDataDelete, OdooDataField, XmlDataMenuItem, OdooDataRecord, XmlDataTemplate}}, oyarn, threads::SessionInfo, Sy};

use super::xml_arch_builder::XmlArchBuilder;

/* Contains the RelaxNG Validation part of the XmlArchBuilder */
impl XmlArchBuilder {

    pub fn load_odoo_openerp_data(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        match node.tag_name().name() {
            "odoo" | "openerp" | "data" => {
                for attr in node.attributes() {
                    match attr.name() {
                        "noupdate" | "auto_sequence" | "uid" | "context" => {},
                        _ => {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05004, &[attr.name(), node.tag_name().name()]) {
                                diagnostics.push(
                                    Diagnostic {
                                        range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                        ..diagnostic.clone()
                                    }
                                );
                            }
                        }
                    }
                }

                for child in node.children().filter(|n| n.is_element()) {
                    if !(self.load_odoo_openerp_data(session, &child, diagnostics)
                        || self.load_menuitem(session, &child, false, diagnostics)
                        || self.load_record(session, &child, diagnostics)
                        || self.load_template(session, &child, diagnostics)
                        || self.load_delete(session, &child, diagnostics)
                        || self.load_function(session, &child, diagnostics)
                        || child.is_text() || child.is_comment()) {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05005, &[child.tag_name().name(), node.tag_name().name()]) {
                            diagnostics.push(
                                Diagnostic {
                                    range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                                    ..diagnostic.clone()
                                }
                            );
                        }
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
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05008, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                },
                "groups" => {
                    let missing_groups = attr.value().split(",")
                        .filter(|group| self.get_group_ids(session, group.trim_start_matches("-"), &attr, diagnostics).is_empty())
                        .collect::<Vec<&str>>()
                        .join(",");
                    if missing_groups.len() > 0 {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05054, &[&missing_groups]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                },
                "name" | "active" => {},
                "action" => {
                    if (has_parent || is_submenu) && node.has_children() {
                        for sub_menu in node.children().filter(|c| c.is_element() && c.tag_name().name() == "menuitem") {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05009, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(sub_menu.range().start as u32, 0), end: Position::new(sub_menu.range().end as u32, 0) },
                                    ..diagnostic.clone()
                                });
                            }
                        }
                    }
                    //check that action exists
                    if SyncOdoo::get_xml_ids(session, &self.xml_symbol, attr.value(), &attr.range(), diagnostics).is_empty() {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05053, &[attr.value()]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                }
                "parent" => {
                    if is_submenu {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05012, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    } else {
                        //check that parent exists
                        if SyncOdoo::get_xml_ids(session, &self.xml_symbol, attr.value(), &attr.range(), diagnostics).is_empty() {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05052, &[attr.value()]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                    ..diagnostic.clone()
                                });
                            }
                        }
                    }
                },
                "web_icon" => {
                    if has_parent || is_submenu {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05010, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                }
                _ => {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05007, &[attr.name()]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            }
        }
        if found_id.is_none() {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05006, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        }
        for child in node.children().filter(|n| n.is_element()) {
            if child.tag_name().name() != "menuitem" {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05011, &[child.tag_name().name()]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                        ..diagnostic.clone()
                    });
                }
            }
            else {
                self.load_menuitem(session, &child, true, diagnostics);
            }
        }
        let data = OdooData::MENUITEM(XmlDataMenuItem {
            file_symbol: Rc::downgrade(&self.xml_symbol),
            xml_id: found_id.clone().map(|id| oyarn!("{}", id)),
            range: node.range().clone()
        });
        self.on_operation_creation(session, found_id, node, data, diagnostics);
        true
    }

    /// Load a <record> node, returning true if node is a record node
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
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05013, &[attr.name()]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            }
        }

        if !found_model {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05014, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
            return true;
        }
        let mut data = OdooDataRecord {
            file_symbol: Rc::downgrade(&self.xml_symbol),
            model: (oyarn!("{}", node.attribute("model").unwrap()), node.attribute_node("model").unwrap().range()),
            xml_id: found_id.clone().map(|id| oyarn!("{}", id)),
            fields: vec![],
            range: node.range().clone()
        };
        for child in node.children().filter(|n| n.is_element()) {
            if let Some(field) = self.load_field(session, &child, diagnostics) {
                data.fields.push(field);
            } else if child.tag_name().name() != "field" {
                // Diagnostic only for non-field tags, not for invalid ones
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05015, &[child.tag_name().name()]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                        ..diagnostic.clone()
                    });
                }
            }
        }
        let data = OdooData::RECORD(data);
        self.on_operation_creation(session, found_id, node, data, diagnostics);
        true
    }

    fn load_field(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> Option<OdooDataField> {
        if node.tag_name().name() != "field" { return None; }
        let Some(node_name_node) = node.attribute_node("name") else {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05016, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
            return None;
        };

        let has_type = node.attribute("type").is_some();
        let ref_key = node.attribute_node("ref").map(|rk| (rk.value().to_string(), rk.range()));
        let has_ref = ref_key.is_some();
        let has_eval = node.attribute("eval").is_some();
        let has_search = node.attribute("search").is_some();
        if [has_type, has_ref, has_eval, has_search].iter().filter(|b| **b).count() > 1 {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05017, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
            return None;
        }
        let mut is_xml_or_html = false;
        let mut iterable_child_node = false;
        if let Some(field_type) = node.attribute("type") {
            match field_type {
                "int" => {
                    let content = node.text().unwrap_or("");
                    if !(content.parse::<i32>().is_ok() || content == "None") {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05018, &[content]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                }
                "float" => {
                    let content = node.text().unwrap_or("");
                    if content.parse::<f64>().is_err() {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05019, &[content]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                }
                "list" | "tuple" => {
                    iterable_child_node = true;
                    for child in node.children() {
                        if !self.load_value(session, &child, diagnostics) {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05020, &[child.tag_name().name()]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                                    ..diagnostic.clone()
                                });
                            }
                        }
                    }
                }
                "html" | "xml" => {
                    is_xml_or_html = true;
                }
                "base64" | "char" | "file" => {
                    if node.has_attribute("file") {
                        if node.text().is_some() {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05021, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                                    ..diagnostic.clone()
                                });
                            }
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
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05022, &[attr.name()]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                },
                "model" => {
                    if !has_eval && !has_search {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05023, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                },
                "use" => {
                    if !has_search {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05024, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                }
                _ => {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05025, &[attr.name()]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            }
        }
        for child in node.children() {
            if !self.load_record(session, &child, diagnostics) && !child.is_text() && !child.is_comment() && !is_xml_or_html && !iterable_child_node{
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05026, &[]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                        ..diagnostic.clone()
                    });
                }
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
        Some(OdooDataField {
            name: oyarn!("{}", node_name_node.value()),
            range: node_name_node.range(),
            text: text,
            text_range: text_range,
            ref_key,
        })
    }

    fn load_value(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "value" { return false; }
        let mut has_search = false;
        let mut has_eval = false;
        let mut has_type_or_file_or_text =  node.text().is_some();
        for attr in node.attributes() {
            match attr.name() {
                "name" | "model" | "use" => {},
                "search" => {
                    has_search = true;
                    if has_eval || has_type_or_file_or_text {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05027, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                },
                "eval" => {
                    has_eval = true;
                    if has_search || has_type_or_file_or_text {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05028, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                },
                "type" => {
                    has_type_or_file_or_text = true;
                    if has_search || has_eval {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05029, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                            continue;
                        }
                    }
                    if !node.has_attribute("file") && !node.text().is_some() {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05036, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                },
                "file" => {
                    has_type_or_file_or_text = true;
                    if node.text().is_some() {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05030, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                            continue;
                        }
                    }
                    if has_search || has_eval {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05031, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                }
                _ => {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05032, &[attr.name()]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            }
        }
        if !has_search && !has_eval && !has_type_or_file_or_text {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05037, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        }
        true
    }

    fn load_template(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "template" { return false; }
        //no interesting rule to check, as 'any' is valid
        let found_id = node.attribute("id").map(|s| s.to_string());
        let data = OdooData::TEMPLATE(XmlDataTemplate {
            file_symbol: Rc::downgrade(&self.xml_symbol),
            xml_id: found_id.clone().map(|id| oyarn!("{}", id)),
            range: node.range().clone(),
        });
        self.on_operation_creation(session, found_id, node, data, diagnostics);
        true
    }

    fn load_delete(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "delete" { return false; }
        if node.attribute("model").is_none() {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05033, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
            return true;
        }
        let found_id = node.attribute("id").map(|s| s.to_string());
        let has_search = node.attribute("search").is_some();
        if found_id.is_some() && has_search {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05034, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        }
        if found_id.is_none() && !has_search {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05035, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        }
        let data = OdooData::DELETE(XmlDataDelete {
            file_symbol: Rc::downgrade(&self.xml_symbol),
            xml_id: found_id.clone().map(|id| oyarn!("{}", id)),
            range: node.range().clone(),
            model: Sy!(node.attribute("model").unwrap().to_string()),
        });
        self.on_operation_creation(session, found_id, node, data, diagnostics);
        true
    }

    fn load_function(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "function" { return false; }
        for attr in ["model", "name"] {
            if node.attribute(attr).is_none() {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05044, &[attr]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                        ..diagnostic.clone()
                    });
                }
            }
        }
        let mut has_eval = false;
        for attr in node.attributes() {
            match attr.name() {
                "model" | "name" => {},
                "uid" => {},
                "context" => {},
                "eval" => {
                    has_eval = true;
                }
                _ => {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05046, &[attr.name()]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            }
        }
        let mut has_value_or_function_child = false;
        for child in node.children().filter(|n| n.is_element()) {
            if self.load_value(session, &child, diagnostics) {
                has_value_or_function_child = true;
                if has_eval {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05045, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            } else if self.load_function(session, &child, diagnostics) {
                has_value_or_function_child = true;
                if has_eval {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05047, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            } else {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05048, &[child.tag_name().name()]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                        ..diagnostic.clone()
                    });
                }
            }
        }
        if !has_eval && !has_value_or_function_child {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05038, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        }
        true
    }
}