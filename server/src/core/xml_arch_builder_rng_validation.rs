use std::{cell::RefCell, rc::Rc};

use lsp_types::{Diagnostic, Position, Range};
use roxmltree::Node;
use ruff_text_size::{TextRange, TextSize};

use crate::{Sy, constants::OYarn, core::{diagnostics::{DiagnosticCode, create_diagnostic}, model::Model, odoo::SyncOdoo, symbols::{storage::{XmlFieldParent, xml::xml_field_symbol::XmlFieldName}, symbol_keys::{XmlFieldKey, XmlId, XmlRecordKey}}}, oyarn, threads::SessionInfo, utils};

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
                    if !(self.load_template(session, &child, diagnostics) //template should be tested before odoo_openerp_data
                        || self.load_qweb_template(session, &child, None, false, diagnostics)
                        || self.load_odoo_openerp_data(session, &child, diagnostics)
                        || self.load_menuitem(session, &child, false, diagnostics)
                        || self.load_record(session, &child, diagnostics)
                        || self.load_delete(session, &child, diagnostics)
                        || self.load_function(session, &child, diagnostics)
                        || self.load_asset(session, &child, diagnostics)
                        || child.is_text() || child.is_comment()
                    )
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05005, &[child.tag_name().name(), node.tag_name().name()]) {
                            diagnostics.push(
                                Diagnostic {
                                    range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                                    ..diagnostic.clone()
                                }
                            );
                        }
                }
                true
            },
            _ => false,
        }
    }

    pub fn load_frontend_data(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) {
        for attr in node.attributes() {
            if attr.name() == "t-name" {
                // even if discouraged, load it so it can be used in features
                self.load_qweb_template(session, node, None, true, diagnostics);
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05075, &[]) {
                    diagnostics.push(
                        Diagnostic {
                            range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            ..diagnostic
                        }
                    );
                }
                return;
            }
        }
        for child in node.children().filter(|n| n.is_element()) {
            // if not an owl template, we actually do nothing
            self.load_qweb_template(session, &child, None, true, diagnostics);
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
                    if attr.value().parse::<i32>().is_err()
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05008, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                },
                "groups" => {
                    let missing_groups = attr.value().split(",")
                        .filter(|group| self.get_group_ids(session, group.trim_start_matches("-"), &attr, diagnostics).is_empty())
                        .collect::<Vec<&str>>()
                        .join(",");
                    if !missing_groups.is_empty()
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05054, &[&missing_groups]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
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
                    if SyncOdoo::get_xml_ids(session, self.xml_symbol.into(), attr.value(), &attr.range(), diagnostics).is_empty(&session.sync_odoo.symbol_table)
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05053, &[attr.value()])
                    {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
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
                        if SyncOdoo::get_xml_ids(session, self.xml_symbol.into(), attr.value(), &attr.range(), diagnostics).is_empty(&session.sync_odoo.symbol_table)
                            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05052, &[attr.value()])
                        {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                    }
                },
                "web_icon" => {
                    if (has_parent || is_submenu)
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05010, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
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
        if found_id.is_none()
            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05006, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
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
        let data = session.st_mut().add_new_xml_menuitem(
            self.xml_symbol,
            found_id.clone().map(OYarn::from),
            TextRange::new(TextSize::new(node.range().start as u32), TextSize::new(node.range().end as u32))
        );
        self.on_operation_creation(session, found_id, None, node, data.into(), diagnostics);
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
        let record = session.st_mut().add_new_xml_record(
            self.xml_symbol.into(),
            (oyarn!("{}", node.attribute("model").unwrap()), node.attribute_node("model").unwrap().range()),
            found_id.clone().map(|id| oyarn!("{}", id)),
            TextRange::new(TextSize::new(node.range().start as u32), TextSize::new(node.range().end as u32))
        );
        for child in node.children().filter(|n| n.is_element()) {
            if self.load_field(session, &child, record.into(), diagnostics).is_none() && child.tag_name().name() != "field" {
                // Diagnostic only for non-field tags, not for invalid ones
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05015, &[child.tag_name().name()]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                        ..diagnostic.clone()
                    });
                }
            }
        }
        self.register_ir_model_record(session, record);
        self.register_ir_model_fields_record(session, record);
        self.on_operation_creation(session, found_id, None, node, record.into(), diagnostics);
        true
    }

    // load a field and add it to the parent symbol. Parent could be either XmlRecordKey or XmlAssetKey
    fn load_field(&mut self, session: &mut SessionInfo, node: &Node, parent: XmlFieldParent, diagnostics: &mut Vec<Diagnostic>) -> Option<XmlFieldKey> {
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
        let ref_key = node.attribute_node("ref").map(|rk| (rk.value().to_string(), utils::range_to_text_range(rk.range())));
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
                    if !(content.parse::<i32>().is_ok() || content == "None")
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05018, &[content]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                }
                "float" => {
                    let content = node.text().unwrap_or("");
                    if content.parse::<f64>().is_err()
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05019, &[content]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                }
                "list" | "tuple" => {
                    iterable_child_node = true;
                    for child in node.children() {
                        if !self.load_value(session, &child, diagnostics)
                            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05020, &[child.tag_name().name()]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                                    ..diagnostic.clone()
                                });
                            }
                    }
                }
                "html" | "xml" => {
                    is_xml_or_html = true;
                }
                "base64" | "char" | "file" => {
                    if node.has_attribute("file")
                        && node.text().is_some()
                            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05021, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                                    ..diagnostic.clone()
                                });
                            }
                }
                _ => {},
            }
        }
        for attr in node.attributes() {
            match attr.name() {
                "name" | "type" | "file" => {},
                "ref" | "eval" | "search" => {
                    if node.text().is_some()
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05022, &[attr.name()]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                },
                "model" => {
                    if !has_eval && !has_search
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05023, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                },
                "use" => {
                    if !has_search
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05024, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
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
            if !self.load_record(session, &child, diagnostics) && !child.is_text() && !child.is_comment() && !is_xml_or_html && !iterable_child_node
                && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05026, &[]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                        ..diagnostic.clone()
                    });
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
        let field = session.st_mut().add_new_xml_field(
            parent,
            oyarn!("{}", node_name_node.value()),
            TextRange::new(TextSize::new(node_name_node.range().start as u32), TextSize::new(node_name_node.range().end as u32)),
            text,
            text_range.map(|r| TextRange::new(TextSize::new(r.start as u32), TextSize::new(r.end as u32))),
            ref_key);
        Some(field)
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
                    if (has_eval || has_type_or_file_or_text)
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05027, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                },
                "eval" => {
                    has_eval = true;
                    if (has_search || has_type_or_file_or_text)
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05028, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                },
                "type" => {
                    has_type_or_file_or_text = true;
                    if (has_search || has_eval)
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05029, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                            continue;
                        }
                    if !node.has_attribute("file") && node.text().is_none()
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05036, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                },
                "file" => {
                    has_type_or_file_or_text = true;
                    if node.text().is_some()
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05030, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                            continue;
                        }
                    if (has_search || has_eval)
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05031, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
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
        if !has_search && !has_eval && !has_type_or_file_or_text
            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05037, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        true
    }

    fn load_template(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "template" { return false; }
        //no interesting rule to check, as 'any' is valid
        let found_id = node.attribute("id").map(|s| s.to_string());
        self.load_qweb_template(session, node, found_id, false, diagnostics);
        true
    }

    fn collect_t_calls(node: &Node) -> Vec<(OYarn, TextRange)> {
        let mut result = vec![];
        for desc in node.descendants() {
            if !desc.is_element() { continue; }
            if let Some(attr) = desc.attribute_node("t-call") {
                result.push((
                    oyarn!("{}", attr.value()),
                    TextRange::new(TextSize::new(attr.range().start as u32), TextSize::new(attr.range().end as u32))
                ));
            }
        }
        result
    }

    fn collect_t_inherit(node: &Node) -> Option<(OYarn, TextRange)> {
        let inherit_attr = node.attribute_node("t-inherit")?;
        let r = inherit_attr.range_value();
        Some((
            oyarn!("{}", inherit_attr.value()),
            TextRange::new(TextSize::new(r.start as u32), TextSize::new(r.end as u32)),
        ))
    }

    fn load_qweb_template(&mut self, session: &mut SessionInfo, node: &Node, found_id: Option<String>, for_web: bool, diagnostics: &mut Vec<Diagnostic>) -> bool {
        let found_t_name_node = node.attribute_node("t-name");
        let found_t_name = found_t_name_node.map(|n| n.value().to_string());
        let found_inherit = node.attribute("t-inherit").is_some();
        if found_id.is_none() && found_t_name.is_none() && !found_inherit {
            return false;
        }
        let data = session.st_mut().add_new_xml_template(
            self.xml_symbol,
            found_id.as_ref().map(|id| oyarn!("{}", id)),
            found_t_name.as_ref().map(|t_name| oyarn!("{}", t_name)),
            TextRange::new(TextSize::new(node.range().start as u32), TextSize::new(node.range().end as u32)),
            for_web
        );
        let t_calls = Self::collect_t_calls(node);
        if !t_calls.is_empty() {
            session.st_mut()[data].t_calls = t_calls;
        }
        // Record the `t-inherit` target — powers find-references of a template name.
        if let Some(t_inherit) = Self::collect_t_inherit(node) {
            session.st_mut()[data].t_inherit = Some(t_inherit);
        }
        if let Some(found_t_name_node) = found_t_name_node && let Some(found_t_name) = &found_t_name
            && !found_t_name.contains(".")
            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05071, &[])
        {
            diagnostics.push(Diagnostic {
                range: Range { start: Position::new(found_t_name_node.range().start as u32, 0), end: Position::new(found_t_name_node.range().end as u32, 0) },
                ..diagnostic.clone()
            });
        }
        self.on_operation_creation(session, found_id, found_t_name, node, data.into(), diagnostics);
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
        if found_id.is_some() && has_search
            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05034, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        if found_id.is_none() && !has_search
            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05035, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        let data = session.st_mut().add_new_xml_delete(
            self.xml_symbol,
            found_id.clone().map(|id| oyarn!("{}", id)),
            TextRange::new(TextSize::new(node.range().start as u32), TextSize::new(node.range().end as u32)),
            Sy!(node.attribute("model").unwrap().to_string())
        );
        self.on_operation_creation(session, found_id, None, node, data.into(), diagnostics);
        true
    }

    fn load_function(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "function" { return false; }
        for attr in ["model", "name"] {
            if node.attribute(attr).is_none()
                && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05044, &[attr]) {
                    diagnostics.push(Diagnostic {
                        range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                        ..diagnostic.clone()
                    });
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
        for child in node.children().filter(|n| n.is_element()) {
            if self.load_value(session, &child, diagnostics) {
                if has_eval
                    && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05045, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
            } else if self.load_function(session, &child, diagnostics) {
                if has_eval
                    && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05047, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
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
        true
    }

    fn load_asset(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diagnostic>) -> bool {
        if node.tag_name().name() != "asset" { return false; }
        // Validate required attributes: id, name
        let mut found_id = None;
        let mut has_name = false;
        for attr in node.attributes() {
            match attr.name() {
                "id" => { found_id = Some(attr.value().to_string()); },
                "name" => { has_name = true; },
                "active" => {},
                _ => {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05058, &[attr.name()]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            }
        }
        if found_id.is_none() {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05059, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        }
        else if !has_name
            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05060, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        let asset = session.st_mut().add_new_xml_asset(
            self.xml_symbol,
            found_id.clone().map(OYarn::from),
            TextRange::new(TextSize::new(node.range().start as u32), TextSize::new(node.range().end as u32)));
        // Validate children: must be bundle, path, or field
        let (mut has_bundle, mut has_path) = (false, false);
        for child in node.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "bundle" => {
                    has_bundle = true;
                    for attr in child.attributes() {
                        if attr.name() != "directive"
                            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05061, &[attr.name()]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                    ..diagnostic.clone()
                                });
                            }
                    }
                    if child.text().is_none()
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05066, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                },
                "path" => {
                    has_path = true;
                    if child.attributes().count() > 0 {
                        for attr in child.attributes() {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05062, &[attr.name()]) {
                                diagnostics.push(Diagnostic {
                                    range: Range { start: Position::new(attr.range().start as u32, 0), end: Position::new(attr.range().end as u32, 0) },
                                    ..diagnostic.clone()
                                });
                            }
                        }
                    }
                    if child.text().is_none()
                        && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05067, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                                ..diagnostic.clone()
                            });
                        }
                },
                "field" => {
                    self.load_field(session, &child, asset.into(), diagnostics);
                },
                "active" => {},
                _ => {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05063, &[child.tag_name().name()]) {
                        diagnostics.push(Diagnostic {
                            range: Range { start: Position::new(child.range().start as u32, 0), end: Position::new(child.range().end as u32, 0) },
                            ..diagnostic.clone()
                        });
                    }
                }
            }
        }
        if !has_bundle
            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05064, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        if !has_path
            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05065, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range { start: Position::new(node.range().start as u32, 0), end: Position::new(node.range().end as u32, 0) },
                    ..diagnostic.clone()
                });
            }
        self.on_operation_creation(session, found_id, None, node, asset.into(), diagnostics);
        true
    }

    fn register_ir_model_record(&self, session: &mut SessionInfo, record: XmlRecordKey) {
        let xml_record_sym = &session.st()[record];
        if xml_record_sym.model.0 != "ir.model" {
            return;
        }

        let Some(model_name) = xml_record_sym
            .get_field_text(XmlFieldName::Model, session.st())
            .map(|name| Sy!(name))
        else {
            return;
        };
        let model = session
            .sync_odoo
            .models
            .entry(model_name.clone())
            .or_insert_with(|| Rc::new(RefCell::new(Model::new(model_name.clone()))))
            .clone();
        model.borrow_mut().add_symbol(session, record);
        session
            .sync_odoo
            .get_main_entry()
            .borrow_mut()
            .search_rebuild_for_models(session, model_name);
    }

    fn register_ir_model_fields_record(&self, session: &mut SessionInfo, record: XmlRecordKey) {
        let xml_record_sym = &session.st()[record];
        if xml_record_sym.model.0 != "ir.model.fields" {
            return;
        }

        let model_name: OYarn = if let Some(&field_sym_key) =
            xml_record_sym.fields().get(XmlFieldName::ModelId.as_str())
        {
            let field_sym = &session.st()[field_sym_key];
            let Some((ref_key, ref_range)) = field_sym.ref_key.clone() else {
                return;
            };
            let xml_ids = SyncOdoo::get_xml_ids(
                session,
                self.xml_symbol.into(),
                &ref_key,
                &utils::text_range_to_range(&ref_range),
                &mut vec![],
            );
            let model_name_option =
                xml_ids
                    .iter_valid(session.st())
                    .find_map(|xml_id| match xml_id {
                        XmlId::PythonClass(key) => {
                            let sym = &session.st()[key];
                            sym._model.as_ref().map(|model| model.name.clone())
                        }
                        XmlId::XmlRecord(key) => {
                            let st = session.st();
                            let sym = &st[key];
                            if sym.model.0 != "ir.model" {
                                return None;
                            }
                            sym.fields().iter().find_map(|(name, &field_key)| {
                                if name == "model" {
                                    st[field_key].text.as_ref().map(|t| oyarn!("{}", t))
                                } else {
                                    None
                                }
                            })
                        }
                        _ => None,
                    });
            match model_name_option {
                Some(model_name) => model_name,
                None => return,
            }
        } else if let Some(model_name) =
            xml_record_sym.get_field_text(XmlFieldName::Model, session.st())
        {
            Sy!(model_name)
        } else {
            return;
        };
        if let Some(model) = session.sync_odoo.models.get(&model_name).cloned() {
            model.borrow_mut().add_xml_field_symbol(session, record);
        }
    }
}
