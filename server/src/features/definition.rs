use lsp_types::{GotoDefinitionResponse, Location, LocationLink, Range};
use std::{cell::RefCell, rc::Rc};
use roxmltree;

use crate::core::file_mgr::{AstKind, FileInfo, FileMgr};
use crate::core::tsserver_bridge;
use crate::features::owl_component_utils;
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::features::goto_utils::{GotoRequest, GotoSource, GotoSourceType, GotoUtils};
use crate::features::owl_virtual;
use crate::features::owl_xml_utils::TEMPLATE_NAME_ATTRS;
use crate::threads::SessionInfo;

pub struct DefinitionFeature {}

impl DefinitionFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: SourceFileKey,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let ast_kind = file_info.borrow().file_info_ast.borrow().ast.kind();
        let definitions_sources = match ast_kind {
            AstKind::PythonAst => GotoUtils::get_symbols(session, GotoRequest::Definition, file_symbol, file_info, line, character),
            AstKind::XmlAst => GotoUtils::get_symbols_xml(session, file_symbol, file_info, line, character),
            AstKind::CsvAst => GotoUtils::get_symbols_csv(session, file_symbol, file_info, line, character),
            AstKind::JsAst => {return DefinitionFeature::get_js_definition(session, file_info, line, character);},
        };
        let links: Vec<LocationLink> = definitions_sources
            .iter()
            .flat_map(|def| GotoUtils::goto_source_to_location(session, def))
            .collect();
        if ast_kind == AstKind::XmlAst && links.is_empty()
        && let Some(response) = Self::get_owl_js_definition(session, file_info, line, character) {
            return Some(response);
        }
        Some(GotoDefinitionResponse::Link(links))
    }

    fn get_js_definition(session: &mut SessionInfo, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<GotoDefinitionResponse> {
        // Check if cursor is over a template reference (e.g. `static template = "module.xml_id"`)
        let encoding = session.sync_odoo.encoding;
        let template_refs = file_info.borrow().file_info_ast.borrow().ast.as_js_ast().js_template_refs.clone();
        for template_ref in &template_refs {
            // @todo: this is a change from the previous (fda's) call. Check if equivalent, and why it changed.
            let range = file_info.borrow().text_range_to_range(template_ref.range, encoding);
            if Self::position_in_range(line, character, &range) {
                let Some(templates) = session.sync_odoo.js_templates.get(&template_ref.t_name) else { continue; };
                let mut locations = vec![];
                for template in templates.iter_valid(&session.sync_odoo.symbol_table) {
                    locations.extend(GotoUtils::goto_source_to_location(session, &GotoSource {
                        source: GotoSourceType::SymbolKey(template.into()),
                        origin_selection_range: Some(range),
                    }));
                }
                return Some(GotoDefinitionResponse::Link(locations));
            }
        }

        let file_path = &file_info.borrow().uri;
        let locs: Vec<Location> = if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
            bridge.get_definition(file_path, line, character)
                .iter()
                .map(tsserver_bridge::ts_to_lsp_location)
                .collect()
        } else {
            vec![]
        };
        if locs.is_empty() {
            return None;
        }
        Some(GotoDefinitionResponse::Array(locs))
    }

    // @todo-ref: move the methods below to a dedicated owl_js_definition file
    /// Resolve an OWL cursor in an XML template: template-name attribute values navigate
    /// through the in-house indexes; everything else (directive expressions, component
    /// props, tag names) is delegated to the self-locating `owl_virtual::definition_xml_owl`.
    fn get_owl_js_definition(
        session: &mut SessionInfo,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32,
    ) -> Option<GotoDefinitionResponse> {
        let encoding = session.sync_odoo.encoding;
        let data = file_info.borrow().file_info_ast.borrow()
            .text_document.as_ref()?.contents().to_string();
        let offset = file_info.borrow().position_to_offset(line, character, encoding);
        let document = roxmltree::Document::parse(&data).ok()?;

        // A template-name value (an xml_id): `t-name` → the component class; `t-call` /
        // `t-inherit` → the `<t t-name>` declaration. A dynamic name (`t-call="{{tpl}}"`)
        // is a JS expression and falls through to the virtual doc instead.
        if let Some((attr_name, template_name, value_range)) =
            Self::find_template_name_attr(document.root_element(), offset)
        {
            match attr_name.as_str() {
                "t-name" => {
                    return Self::goto_component_from_template(
                        session, file_info, &template_name, value_range,
                    );
                }
                "t-call" | "t-inherit"
                    if !template_name.contains("{{") && !template_name.contains("#{") =>
                {
                    return Self::goto_template_declaration(
                        session, file_info, &template_name, value_range,
                    );
                }
                _ => {}
            }
        }

        owl_virtual::definition_xml_owl(session, file_info, line, character)
    }

    /// Find the template-name attribute (`t-name` / `t-call` / `t-inherit`) whose value
    /// contains `offset`, as `(attr_name, template_name, value_byte_range_in_file)`.
    pub(crate) fn find_template_name_attr(
        node: roxmltree::Node,
        offset: usize,
    ) -> Option<(String, String, std::ops::Range<usize>)> {
        if !node.is_element() {
            return None;
        }
        for attr in node.attributes() {
            if !TEMPLATE_NAME_ATTRS.contains(&attr.name()) {
                continue;
            }
            // `range_value()` excludes the surrounding quotes (see `owl_virtual`), so the
            // range maps 1:1 to the attribute value string.
            let r = attr.range_value();
            if r.start <= offset && offset <= r.end {
                return Some((attr.name().to_string(), attr.value().to_string(), r.start..r.end));
            }
        }
        node.children().find_map(|child| Self::find_template_name_attr(child, offset))
    }

    /// Navigate from a template declaration (`<t t-name>`) to the component class whose
    /// `static template` references it (via `js_component_by_template` → descriptors).
    fn goto_component_from_template(
        session: &mut SessionInfo,
        file_info: &Rc<RefCell<FileInfo>>,
        template_name: &str,
        value_range: std::ops::Range<usize>,
    ) -> Option<GotoDefinitionResponse> {
        let encoding = session.sync_odoo.encoding;
        let class_name = owl_component_utils::component_for_template(session, template_name)?;
        let (file_path, name_byte, name_len) = {
            let descriptor = session.sync_odoo.component_descriptors.get(&class_name)?;
            (
                descriptor.file_path.clone(),
                descriptor.class_name_byte as usize,
                descriptor.class_name.len(),
            )
        };

        // The component's `FileInfo` may not have a text document loaded; read the source
        // directly (falls back to disk) and convert byte offsets ourselves.
        let content = owl_virtual::read_real_js(session, &file_path)?;
        let target_range = owl_virtual::byte_range_to_lsp_range(
            &content,
            name_byte..name_byte + name_len,
            encoding,
        );

        let origin_lsp_range = file_info.borrow().std_range_to_range(&value_range, encoding);

        Some(GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(origin_lsp_range),
            target_uri: FileMgr::pathname2uri(&file_path),
            target_range,
            target_selection_range: target_range,
        }]))
    }

    /// Navigate from a template *reference* (`t-call` / `t-inherit`) to the `<t t-name>`
    /// declaration(s) of the named template, via `js_templates`.
    fn goto_template_declaration(
        session: &mut SessionInfo,
        file_info: &Rc<RefCell<FileInfo>>,
        template_name: &str,
        value_range: std::ops::Range<usize>,
    ) -> Option<GotoDefinitionResponse> {
        let encoding = session.sync_odoo.encoding;
        let origin = file_info.borrow().std_range_to_range(&value_range, encoding);

        let locations = session.sync_odoo.js_templates.get(template_name)?
            .iter_valid(&session.sync_odoo.symbol_table)
            .flat_map(|template| {
                GotoUtils::goto_source_to_location(
                    session,
                    &GotoSource {
                        source: GotoSourceType::SymbolKey(template.into()),
                        origin_selection_range: Some(origin),
                    },
                )
            })
            .collect::<Vec<_>>();
        if locations.is_empty() {
            return None;
        }
        Some(GotoDefinitionResponse::Link(locations))
    }

    fn position_in_range(line: u32, character: u32, range: &Range) -> bool {
        let after_start = line > range.start.line
            || (line == range.start.line && character >= range.start.character);
        let before_end = line < range.end.line
            || (line == range.end.line && character <= range.end.character);
        after_start && before_end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_template_name_attr_locates_t_name_value() {
        let xml = r#"<templates><t t-name="mod.MyComp"><div t-if="this.ok"/></t></templates>"#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let root = document.root_element();
        let name_start = xml.find("mod.MyComp").unwrap();

        // Cursor inside the `t-name` value → returns the attr name, template name, and the
        // value byte range (quotes excluded).
        let (attr, name, range) =
            DefinitionFeature::find_template_name_attr(root, name_start + 2).unwrap();
        assert_eq!(attr, "t-name");
        assert_eq!(name, "mod.MyComp");
        assert_eq!(range, name_start..name_start + "mod.MyComp".len());

        // A directive expression value (`t-if`) is not a template-name attribute.
        let ok_start = xml.find("this.ok").unwrap();
        assert!(DefinitionFeature::find_template_name_attr(root, ok_start).is_none());
    }

    #[test]
    fn find_template_name_attr_locates_t_call_and_t_inherit() {
        let xml = r#"<templates>
            <t t-name="mod.Child"><t t-call="mod.Base"/></t>
            <t t-name="mod.Override" t-inherit="mod.Base"><div/></t>
        </templates>"#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let root = document.root_element();

        let call_start = xml.find("mod.Base\"/>").unwrap();
        let (attr, name, range) =
            DefinitionFeature::find_template_name_attr(root, call_start + 1).unwrap();
        assert_eq!(attr, "t-call");
        assert_eq!(name, "mod.Base");
        assert_eq!(range, call_start..call_start + "mod.Base".len());

        // On the same element, `t-name` and `t-inherit` are disambiguated by cursor position.
        let inherit_start = xml.find(r#"t-inherit="mod.Base""#).unwrap() + r#"t-inherit=""#.len();
        let (attr, name, _) =
            DefinitionFeature::find_template_name_attr(root, inherit_start + 1).unwrap();
        assert_eq!(attr, "t-inherit");
        assert_eq!(name, "mod.Base");

        let override_name_start = xml.find("mod.Override").unwrap();
        let (attr, name, _) =
            DefinitionFeature::find_template_name_attr(root, override_name_start + 1).unwrap();
        assert_eq!(attr, "t-name");
        assert_eq!(name, "mod.Override");
    }
}
