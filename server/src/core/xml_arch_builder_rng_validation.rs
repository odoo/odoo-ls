use roxmltree::Node;
use ruff_text_size::{TextRange, TextSize};

use crate::{Sy, constants::OYarn, core::{diagnostics::{Diag, DiagnosticCode, create_diag}, entry_point::EntryPoint, model::Model, odoo::SyncOdoo, symbols::{storage::{XmlFieldParent, xml::xml_field_symbol::XmlFieldName}, symbol_keys::{XmlFieldKey, XmlId, XmlRecordKey}}}, features::owl_xml_utils::is_template_extension, oyarn, threads::SessionInfo, utils::TextRanged};

use super::xml_arch_builder::XmlArchBuilder;

/* Contains the RelaxNG Validation part of the XmlArchBuilder */
impl XmlArchBuilder {

    pub fn load_odoo_openerp_data(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diag>) -> bool {
        match node.tag_name().name() {
            "odoo" | "openerp" | "data" => {
                for attr in node.attributes() {
                    match attr.name() {
                        "noupdate" | "auto_sequence" | "uid" | "context" => {},
                        _ => {
                            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05004, &[attr.name(), node.tag_name().name()], attr.text_range()));
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
                    ) {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05005, &[child.tag_name().name(), node.tag_name().name()], child.text_range()));
                    }
                }
                true
            },
            _ => false,
        }
    }

    pub fn load_frontend_data(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diag>) {
        for attr in node.attributes() {
            if attr.name() == "t-name" {
                // even if discouraged, load it so it can be used in features
                self.load_qweb_template(session, node, None, true, diagnostics);
                diagnostics.extend(create_diag(session, DiagnosticCode::OLS05075, &[], attr.text_range()));
                return;
            }
        }
        for child in node.children().filter(|n| n.is_element()) {
            // if not an owl template, we actually do nothing
            self.load_qweb_template(session, &child, None, true, diagnostics);
        }
    }

    fn load_menuitem(&mut self, session: &mut SessionInfo, node: &Node, is_submenu: bool, diagnostics: &mut Vec<Diag>) -> bool {
        if node.tag_name().name() != "menuitem" { return false; }
        let mut found_id = None;
        let has_parent = node.attribute("parent").is_some();
        for attr in node.attributes() {
            let attr_range = attr.text_range();
            match attr.name() {
                "id" => {
                    found_id = Some(attr.value().to_string());
                },
                "sequence" => {
                    if attr.value().parse::<i32>().is_err() {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05008, &[], attr_range));
                    }
                },
                "groups" => {
                    let missing_groups = attr.value().split(",")
                        .filter(|group| self.get_group_ids(session, group.trim_start_matches("-"), &attr, diagnostics).is_empty())
                        .collect::<Vec<&str>>()
                        .join(",");
                    if !missing_groups.is_empty() {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05054, &[&missing_groups], attr_range));
                    }
                },
                "name" | "active" => {},
                "action" => {
                    if (has_parent || is_submenu) && node.has_children() {
                        for sub_menu in node.children().filter(|c| c.is_element() && c.tag_name().name() == "menuitem") {
                            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05009, &[], sub_menu.text_range()));
                        }
                    }
                    //check that action exists
                    if SyncOdoo::get_xml_ids(session, self.xml_symbol.into(), attr.value(), attr_range, diagnostics).is_empty(&session.sync_odoo.symbol_table) {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05053, &[attr.value()], attr_range));
                    }
                }
                "parent" => {
                    if is_submenu {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05012, &[], attr_range));
                    } else {
                        //check that parent exists
                        if SyncOdoo::get_xml_ids(session, self.xml_symbol.into(), attr.value(), attr_range, diagnostics).is_empty(&session.sync_odoo.symbol_table) {
                            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05052, &[attr.value()], attr_range));
                        }
                    }
                },
                "web_icon" => {
                    if has_parent || is_submenu {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05010, &[], attr_range));
                    }
                }
                _ => {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS05007, &[attr.name()], attr_range));
                }
            }
        }
        if found_id.is_none() {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05006, &[], node.text_range()));
        }
        for child in node.children().filter(|n| n.is_element()) {
            if child.tag_name().name() != "menuitem" {
                diagnostics.extend(create_diag(session, DiagnosticCode::OLS05011, &[child.tag_name().name()], child.text_range()));
            }
            else {
                self.load_menuitem(session, &child, true, diagnostics);
            }
        }
        let data = session.st_mut().add_new_xml_menuitem(
            self.xml_symbol,
            found_id.clone().map(OYarn::from),
            node.text_range()
        );
        self.on_operation_creation(session, found_id, None, node, data.into(), diagnostics);
        true
    }

    /// Load a <record> node, returning true if node is a record node
    fn load_record(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diag>) -> bool {
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
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS05013, &[attr.name()], attr.text_range()));
                }
            }
        }

        if !found_model {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05014, &[], node.text_range()));
            return true;
        }
        let record = session.st_mut().add_new_xml_record(
            self.xml_symbol.into(),
            (oyarn!("{}", node.attribute("model").unwrap()), node.attribute_node("model").unwrap().range()),
            found_id.clone().map(|id| oyarn!("{}", id)),
            node.text_range()
        );
        // load fields in reverse order to have last field declaration overwrite previous ones, as Odoo does
        for child in node.children().filter(|n| n.is_element()).rev() {
            self.load_field(session, &child, record.into(), diagnostics);
        }
        self.register_ir_model_record(session, record);
        self.register_ir_model_fields_record(session, record);
        self.on_operation_creation(session, found_id, None, node, record.into(), diagnostics);
        true
    }

    // load a field and add it to the parent symbol. Parent could be either XmlRecordKey or XmlAssetKey
    fn load_field(&mut self, session: &mut SessionInfo, node: &Node, parent: XmlFieldParent, diagnostics: &mut Vec<Diag>) -> Option<XmlFieldKey> {
        if node.tag_name().name() != "field" {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05015, &[node.tag_name().name()], node.text_range()));
            return None;
        }
        let Some(node_name_node) = node.attribute_node("name") else {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05016, &[], node.text_range()));
            return None;
        };

        let has_type = node.attribute("type").is_some();
        let ref_key = node.attribute_node("ref").map(|rk| (rk.value().to_string(), rk.text_range()));
        let has_ref = ref_key.is_some();
        let has_eval = node.attribute("eval").is_some();
        let has_search = node.attribute("search").is_some();
        if [has_type, has_ref, has_eval, has_search].iter().filter(|b| **b).count() > 1 {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05017, &[], node.text_range()));
            return None;
        }
        let mut is_xml_or_html = false;
        let mut iterable_child_node = false;
        if let Some(field_type) = node.attribute("type") {
            match field_type {
                "int" => {
                    let content = node.text().unwrap_or("");
                    if !(content.parse::<i32>().is_ok() || content == "None") {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05018, &[content], node.text_range()));
                    }
                }
                "float" => {
                    let content = node.text().unwrap_or("");
                    if content.parse::<f64>().is_err() {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05019, &[content], node.text_range()));
                    }
                }
                "list" | "tuple" => {
                    iterable_child_node = true;
                    for child in node.children() {
                        if !self.load_value(session, &child, diagnostics) {
                            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05020, &[child.tag_name().name()], child.text_range()));
                        }
                    }
                }
                "html" | "xml" => {
                    is_xml_or_html = true;
                }
                "base64" | "char" | "file" => {
                    if node.has_attribute("file") && node.text().is_some() {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05021, &[], node.text_range()));
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
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05022, &[attr.name()], node.text_range()));
                    }
                },
                "model" => {
                    if !has_eval && !has_search {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05023, &[], attr.text_range()));
                    }
                },
                "use" => {
                    if !has_search {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05024, &[], attr.text_range()));
                    }
                }
                _ => {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS05025, &[attr.name()], attr.text_range()));
                }
            }
        }
        for child in node.children() {
            if !self.load_record(session, &child, diagnostics) && !child.is_text() && !child.is_comment() && !is_xml_or_html && !iterable_child_node {
                diagnostics.extend(create_diag(session, DiagnosticCode::OLS05026, &[], child.text_range()));
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
        match session.st_mut().add_new_xml_field(
            parent,
            node_name_node.value(),
            TextRange::new(TextSize::new(node_name_node.range().start as u32), TextSize::new(node_name_node.range().end as u32)),
            text,
            text_range.map(|r| TextRange::new(TextSize::new(r.start as u32), TextSize::new(r.end as u32))),
            ref_key
        ) {
            Ok(field) => Some(field),
            Err(_) => {
                diagnostics.extend(create_diag(session, DiagnosticCode::OLS05076, &[node_name_node.value()], node_name_node.text_range()));
                None
            },
        }
    }

    fn load_value(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diag>) -> bool {
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
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05027, &[], attr.text_range()));
                    }
                },
                "eval" => {
                    has_eval = true;
                    if has_search || has_type_or_file_or_text {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05028, &[], attr.text_range()));
                    }
                },
                "type" => {
                    has_type_or_file_or_text = true;
                    if has_search || has_eval {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05029, &[], attr.text_range()));
                        continue;
                    }
                    if !node.has_attribute("file") && node.text().is_none() {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05036, &[], attr.text_range()));
                    }
                },
                "file" => {
                    has_type_or_file_or_text = true;
                    if node.text().is_some() {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05030, &[], attr.text_range()));
                        continue;
                    }
                    if has_search || has_eval {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05031, &[], attr.text_range()));
                    }
                }
                _ => {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS05032, &[attr.name()], attr.text_range()));
                }
            }
        }
        if !has_search && !has_eval && !has_type_or_file_or_text {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05037, &[], node.text_range()));
        }
        true
    }

    fn load_template(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diag>) -> bool {
        if node.tag_name().name() != "template" { return false; }
        //no interesting rule to check, as 'any' is valid
        let found_id = node.attribute("id").map(|s| s.to_string());
        self.load_qweb_template(session, node, found_id, false, diagnostics);
        true
    }

    fn collect_t_name(node: &Node) -> Option<(OYarn, TextRange)> {
        let t_name_attr = node.attribute_node("t-name")?;
        let r = t_name_attr.range_value();
        Some((
            oyarn!("{}", t_name_attr.value()),
            TextRange::new(TextSize::new(r.start as u32), TextSize::new(r.end as u32)),
        ))
    }

    fn collect_t_calls(node: &Node) -> Vec<(OYarn, TextRange)> {
        let mut result = vec![];
        for desc in node.descendants() {
            if !desc.is_element() { continue; }
            if let Some(attr) = desc.attribute_node("t-call") {
                let r = attr.range_value();
                result.push((
                    oyarn!("{}", attr.value()),
                    TextRange::new(TextSize::new(r.start as u32), TextSize::new(r.end as u32))
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

    fn load_qweb_template(&mut self, session: &mut SessionInfo, node: &Node, found_id: Option<String>, for_web: bool, diagnostics: &mut Vec<Diag>) -> bool {
        let mut found_t_name = Self::collect_t_name(node);
        // An extension modifies the inherited template in place: Odoo ignores its t-name
        if is_template_extension(*node)
            && let Some(t_name_attr) = node.attribute_node("t-name")
        {
            found_t_name = None;
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05077, &[], t_name_attr.text_range()));
        }
        let found_inherit = node.attribute("t-inherit").is_some();
        if found_id.is_none() && found_t_name.is_none() && !found_inherit {
            return false;
        }
        let data = session.st_mut().add_new_xml_template(
            self.xml_symbol,
            found_id.as_ref().map(|id| oyarn!("{}", id)),
            found_t_name.clone(),
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
        if let Some((t_name, range)) = &found_t_name && !t_name.contains(".") {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05071, &[], *range));
        }
        let t_name_string = found_t_name.map(|(t_name, _)| t_name.to_string());
        self.on_operation_creation(session, found_id, t_name_string, node, data.into(), diagnostics);
        true
    }

    fn load_delete(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diag>) -> bool {
        if node.tag_name().name() != "delete" { return false; }
        let node_range = node.text_range();
        if node.attribute("model").is_none() {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05033, &[], node_range));
            return true;
        }
        let found_id = node.attribute("id").map(|s| s.to_string());
        let has_search = node.attribute("search").is_some();
        if found_id.is_some() && has_search {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05034, &[], node_range));
        }
        if found_id.is_none() && !has_search {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05035, &[], node_range));
        }
        let data = session.st_mut().add_new_xml_delete(
            self.xml_symbol,
            found_id.clone().map(|id| oyarn!("{}", id)),
            node_range,
            Sy!(node.attribute("model").unwrap().to_string())
        );
        self.on_operation_creation(session, found_id, None, node, data.into(), diagnostics);
        true
    }

    fn load_function(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diag>) -> bool {
        if node.tag_name().name() != "function" { return false; }
        for attr in ["model", "name"] {
            if node.attribute(attr).is_none() {
                diagnostics.extend(create_diag(session, DiagnosticCode::OLS05044, &[attr], node.text_range()));
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
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS05046, &[attr.name()], attr.text_range()));
                }
            }
        }
        for child in node.children().filter(|n| n.is_element()) {
            if self.load_value(session, &child, diagnostics) {
                if has_eval {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS05045, &[], child.text_range()));
                }
            } else if self.load_function(session, &child, diagnostics) {
                if has_eval {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS05047, &[], child.text_range()));
                }
            } else {
                diagnostics.extend(create_diag(session, DiagnosticCode::OLS05048, &[child.tag_name().name()], child.text_range()));
            }
        }
        true
    }

    fn load_asset(&mut self, session: &mut SessionInfo, node: &Node, diagnostics: &mut Vec<Diag>) -> bool {
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
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS05058, &[attr.name()], attr.text_range()));
                }
            }
        }
        if found_id.is_none() {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05059, &[], node.text_range()));
        }
        else if !has_name {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05060, &[], node.text_range()));
        }
        let asset = session.st_mut().add_new_xml_asset(
            self.xml_symbol,
            found_id.clone().map(OYarn::from),
            TextRange::new(TextSize::new(node.range().start as u32), TextSize::new(node.range().end as u32)));
        // Validate children: must be bundle, path, or field
        let (mut has_bundle, mut has_path) = (false, false);
        // iterate in reverse order so that last field declaration overwrite previous ones, as Odoo does
        for child in node.children().filter(|n| n.is_element()).rev() {
            match child.tag_name().name() {
                "bundle" => {
                    has_bundle = true;
                    for attr in child.attributes() {
                        if attr.name() != "directive" {
                            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05061, &[attr.name()], attr.text_range()));
                        }
                    }
                    if child.text().is_none() {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05066, &[], child.text_range()));
                    }
                },
                "path" => {
                    has_path = true;
                    if child.attributes().count() > 0 {
                        for attr in child.attributes() {
                            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05062, &[attr.name()], attr.text_range()));
                        }
                    }
                    if child.text().is_none() {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS05067, &[], child.text_range()));
                    }
                },
                "field" => {
                    self.load_field(session, &child, asset.into(), diagnostics);
                },
                "active" => {},
                _ => {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS05063, &[child.tag_name().name()], child.text_range()));
                }
            }
        }
        if !has_bundle {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05064, &[], node.text_range()));
        }
        if !has_path {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS05065, &[], node.text_range()));
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
        let model = session.sync_odoo.model_mgr.add_or_get_model(&model_name);
        Model::add_symbol(session, model, record);
        let main_entry = session.sync_odoo.get_main_entry();
        EntryPoint::search_rebuild_for_models(session, main_entry, model_name);
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
                ref_range,
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
        if let Some(model) = session.model_mgr().get_model_key(&model_name) {
            Model::add_xml_field_symbol(session, model, record);
        }
    }
}
