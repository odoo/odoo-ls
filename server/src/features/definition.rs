use lsp_types::{GotoDefinitionResponse, Location, LocationLink, Position, Range};
use std::{cell::RefCell, rc::Rc};
use roxmltree;

use crate::core::file_mgr::{AstType, FileInfo, FileMgr};
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::features::goto_utils::{GotoRequest, GotoSource, GotoUtils};
use crate::threads::SessionInfo;

pub struct DefinitionFeature {}

/// A variable declared by a `t-set` or `t-as` directive in an OWL template.
#[derive(Clone)]
struct TemplateVarDecl {
    name: String,
    /// Byte offset in the source file where the variable name starts (inside the attribute value).
    file_offset: usize,
}

/// Context returned when the cursor lands inside an OWL directive attribute value.
struct OWLAttrCtx {
    attr_name: String,
    attr_value: String,
    /// Byte offset of the opening quote of the attribute value in the source file.
    attr_value_start: usize,
    template_name: String,
    /// Byte range within `attr_value` of the word under the cursor.
    word_range: std::ops::Range<usize>,
    /// Template variables in scope at the cursor position.
    template_vars: Vec<TemplateVarDecl>,
}

impl DefinitionFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: SourceFileKey,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let ast_type = file_info.borrow().file_info_ast.borrow().ast_type.clone();
        let definitions_sources = match ast_type {
            AstType::Python => GotoUtils::get_symbols(session, GotoRequest::Definition, file_symbol, file_info, line, character),
            AstType::Xml => GotoUtils::get_symbols_xml(session, file_symbol, file_info, line, character),
            AstType::Csv => GotoUtils::get_symbols_csv(session, file_symbol, file_info, line, character),
            AstType::Js => {vec![]}, // Fill later directly with links from tsserver
        };
        let mut links = vec![];
        for def in definitions_sources.iter() {
            links.extend(GotoUtils::goto_source_to_location(session, def));
        }
        if matches!(ast_type, AstType::Xml) && links.is_empty() {
            if let Some(response) = Self::get_owl_js_definition(session, file_info, line, character) {
                return Some(response);
            }
        }
        if matches!(ast_type, AstType::Js) {
            // Check if cursor is over a template reference (e.g. `static template = "module.xml_id"`)
            let template_refs = file_info.borrow().file_info_ast.borrow().js_template_refs.clone();
            for (range, template_name, _) in &template_refs {
                if Self::position_in_range(line, character, range) {
                    let Some(templates) = session.sync_odoo.js_templates.get(template_name) else { continue; };
                    let mut locations = vec![];
                    for template in templates.iter_valid(&session.sync_odoo.symbol_table) {
                        locations.extend(GotoUtils::goto_source_to_location(session, &GotoSource {
                            source: template.into(),
                            origin_selection_range: Some(*range),
                        }));
                    }
                    return Some(GotoDefinitionResponse::Link(locations));
                }
            }

            let file_path = &file_info.borrow().uri;
            let locs: Vec<Location> = if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
                bridge.get_definition(&file_path, line, character)
                    .into_iter()
                    .filter_map(|(target_file, sl, sc, el, ec)| {
                        let uri = FileMgr::pathname2uri(&target_file);
                        Some(Location {
                            uri,
                            range: Range {
                                start: Position { line: sl, character: sc },
                                end:   Position { line: el, character: ec },
                            },
                        })
                    })
                    .collect()
            } else {
                vec![]
            };
            if locs.is_empty() {
                return None;
            }
            return Some(GotoDefinitionResponse::Array(locs));
        }
        Some(GotoDefinitionResponse::Link(links))
    }

    /// Resolve an OWL directive attribute expression in an XML template.
    ///
    /// First tries to find a matching JS class member (`this.xxx`); if that fails,
    /// looks for a template-local variable declared by `t-set` or `t-as`/`t-foreach`.
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
        let root = document.root_element();

        let ctx = Self::find_owl_attr_at_offset(root, offset)?;

        // Skip declaration sites and non-directive attributes.
        if !ctx.attr_name.starts_with("t-")
            || ctx.attr_name == "t-name"
            || ctx.attr_name.starts_with("t-inherit")
            || ctx.attr_name == "t-set"
            || ctx.attr_name == "t-as"
        {
            return None;
        }

        let word = &ctx.attr_value[ctx.word_range.clone()];
        if word.is_empty() {
            return None;
        }

        let origin_start = ctx.attr_value_start + ctx.word_range.start;
        let origin_end   = ctx.attr_value_start + ctx.word_range.end;
        let origin_lsp_range = file_info.borrow()
            .std_range_to_range(&(origin_start..origin_end), encoding);

        // --- Try JS class member ---
        if let Some(class_name) = session.sync_odoo.js_component_by_template.get(&ctx.template_name).cloned() {
            if let Some(descriptor) = session.sync_odoo.component_descriptors.get(&class_name).cloned() {
                if let Some(member) = descriptor.find_member(word) {
                    let target_uri = FileMgr::pathname2uri(&descriptor.file_path);
                    let target_range = member.range;
                    return Some(GotoDefinitionResponse::Link(vec![LocationLink {
                        origin_selection_range: Some(origin_lsp_range),
                        target_uri,
                        target_range,
                        target_selection_range: target_range,
                    }]));
                }
            }
        }

        // --- Try template-local variable (t-set / t-as) ---
        let var_decl = ctx.template_vars.iter().find(|v| v.name == word)?;
        let decl_start = var_decl.file_offset;
        let decl_end   = decl_start + var_decl.name.len();
        let target_range = file_info.borrow()
            .std_range_to_range(&(decl_start..decl_end), encoding);
        let target_uri = FileMgr::pathname2uri(&file_info.borrow().uri);

        Some(GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(origin_lsp_range),
            target_uri,
            target_range,
            target_selection_range: target_range,
        }]))
    }

    /// Walk the XML tree to find which OWL directive attribute value contains `offset`.
    fn find_owl_attr_at_offset(
        node: roxmltree::Node,
        offset: usize,
    ) -> Option<OWLAttrCtx> {
        Self::walk_xml_for_owl(node, offset, None, vec![])
    }

    fn walk_xml_for_owl<'d>(
        node: roxmltree::Node<'d, 'd>,
        offset: usize,
        parent_template: Option<String>,
        inherited_vars: Vec<TemplateVarDecl>,
    ) -> Option<OWLAttrCtx> {
        if !node.is_element() {
            return None;
        }

        let current_template = node.attribute("t-name")
            .map(|s| s.to_string())
            .or(parent_template.clone());

        // Check if cursor is in one of this element's own attribute values.
        for attr in node.attributes() {
            let attr_range = attr.range_value();
            if attr_range.start <= offset && offset <= attr_range.end {
                let attr_name = attr.name().to_string();
                let Some(ref tmpl) = current_template else { return None; };
                let attr_value = attr.value().to_string();
                let pos_in_val = offset.saturating_sub(attr_range.start + 1);
                let word_range = Self::word_range_at(&attr_value, pos_in_val)?;
                return Some(OWLAttrCtx {
                    attr_name,
                    attr_value,
                    attr_value_start: attr_range.start,
                    template_name: tmpl.clone(),
                    word_range,
                    template_vars: inherited_vars,
                });
            }
        }

        // Variables available to this element's children:
        // inherit all vars from ancestors, then add any t-foreach/t-as on this element.
        let mut child_vars: Vec<TemplateVarDecl> = inherited_vars;
        if let Some(t_as_val) = node.attribute("t-as") {
            if node.attribute("t-foreach").is_some() {
                if let Some(t_as_attr) = node.attributes().find(|a| a.name() == "t-as") {
                    child_vars.push(TemplateVarDecl {
                        name: t_as_val.to_string(),
                        file_offset: t_as_attr.range_value().start + 1,
                    });
                }
            }
        }

        // Recurse into children, collecting t-set declarations from preceding siblings.
        let mut sibling_vars = child_vars;
        for child in node.children() {
            if !child.is_element() {
                continue;
            }
            if let Some(result) = Self::walk_xml_for_owl(child, offset, current_template.clone(), sibling_vars.clone()) {
                return Some(result);
            }
            // cursor was not in this child; if the child is entirely before the cursor,
            // its t-set declaration is now in scope for subsequent siblings.
            if child.range().end <= offset {
                if let Some(t_set_val) = child.attribute("t-set") {
                    if let Some(t_set_attr) = child.attributes().find(|a| a.name() == "t-set") {
                        sibling_vars.push(TemplateVarDecl {
                            name: t_set_val.to_string(),
                            file_offset: t_set_attr.range_value().start + 1,
                        });
                    }
                }
            }
        }
        None
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

    fn position_in_range(line: u32, character: u32, range: &Range) -> bool {
        let after_start = line > range.start.line
            || (line == range.start.line && character >= range.start.character);
        let before_end = line < range.end.line
            || (line == range.end.line && character <= range.end.character);
        after_start && before_end
    }
}
