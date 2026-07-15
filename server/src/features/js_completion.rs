use std::{cell::RefCell, rc::Rc};

use lsp_types::{CompletionItem, CompletionItemKind};
use roxmltree::Node;

use crate::{core::{file_mgr::FileInfo, js_arch_builder::{ComponentDescriptor, InferredType, MemberKind}}, threads::SessionInfo};

const OWL_DIRECTIVE_ATTRS: &[&str] = &[
    "t-if", "t-elif", "t-foreach", "t-out", "t-esc", "t-value", "t-key", "t-model",
];

pub fn owl_completion(
    session: &mut SessionInfo,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
) -> Option<Vec<CompletionItem>> {
    use crate::core::js_arch_builder::MemberKind;

    let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
    let data = file_info
        .borrow()
        .file_info_ast
        .borrow()
        .text_document
        .as_ref()?
        .contents()
        .to_string();
    let document = roxmltree::Document::parse(&data).ok()?;

    let (attr_value, cursor_rel, t_name) =
        find_owl_attr_at_offset(&document.root_element(), offset)?;

    let text_to_cursor = &attr_value[..cursor_rel.min(attr_value.len())];
    let prefix = {
        let idx = text_to_cursor.rfind("this")?;
        &text_to_cursor[idx..]
    };

    let class_name = session.sync_odoo.js_component_by_template.get(&t_name)?.clone();
    let descriptor = session.sync_odoo.component_descriptors.get(&class_name)?;

    let members = complete_members(descriptor, prefix);
    if members.is_empty() {
        return None;
    }

    let items = members
        .into_iter()
        .map(|(name, kind)| CompletionItem {
            label: name,
            kind: Some(match kind {
                MemberKind::Method => CompletionItemKind::METHOD,
                MemberKind::Getter => CompletionItemKind::PROPERTY,
                MemberKind::Prop | MemberKind::Env | MemberKind::ReactiveState => CompletionItemKind::VARIABLE,
                MemberKind::Field => CompletionItemKind::FIELD,
            }),
            ..Default::default()
        })
        .collect();

    Some(items)
}

fn is_owl_directive(attr_name: &str) -> bool {
    OWL_DIRECTIVE_ATTRS.contains(&attr_name)
        || attr_name.starts_with("t-att-")
        || attr_name.starts_with("t-on-")
}

fn find_template_name<'a, 'input>(node: &Node<'a, 'input>) -> Option<String> {
    let mut current = *node;
    loop {
        let tag = current.tag_name().name();
        if (tag == "template" || tag == "t")
            && let Some(t_name) = current.attribute("t-name") {
                return Some(t_name.to_string());
            }
        current = current.parent()?;
    }
}

/**
 * Return the attribute at the given offset if it is an owl directive under the form (attr_name, position of the cursor relative to the start of the attribute, and the t_name of the current template)
 */
fn find_owl_attr_at_offset<'a, 'input>(
    node: &Node<'a, 'input>,
    offset: usize,
) -> Option<(String, usize, String)> {
    if node.is_element() {
        let owl_attr = node.attributes().find(|attr| {
            is_owl_directive(attr.name())
                && attr.range_value().start <= offset
                && attr.range_value().end >= offset
        });
        if let Some(attr) = owl_attr {
            let cursor_rel = offset.saturating_sub(attr.range_value().start);
            if let Some(t_name) = find_template_name(node) {
                return Some((attr.value().to_string(), cursor_rel, t_name));
            }
        }
        for child in node.children() {
            if let Some(result) = find_owl_attr_at_offset(&child, offset) {
                return Some(result);
            }
        }
    }
    None
}

/// Given a `ComponentDescriptor` and the expression prefix typed so far in an
/// Owl directive attribute (e.g. `"this."`, `"this.state."`), return the member
/// names that should be offered as completions.
fn complete_members(descriptor: &ComponentDescriptor, prefix: &str) -> Vec<(String, MemberKind)> {
    if !prefix.starts_with("this") {
        return vec![];
    }
    // Split on dots to determine depth
    let mut current: InferredType;
    let parts: Vec<&str> = prefix.split('.').collect();
    if parts.len() <= 1 {
        return vec![];
    }
    if parts.len() == 2 {
        return descriptor.members.iter().filter_map(
        |m| if m.name.starts_with(parts[1]) {
                Some((m.name.clone(), m.kind.clone()))
            } else {None}).collect();
    }
    //parts.len() is > 2
    if let Some(member) = descriptor.find_member(parts[1]) {
        match &member.typ {
            InferredType::Object(_) => current = member.typ.clone(),
            _ => return vec![], // Can't navigate into non-object types
        }
    } else {
        return vec![]; // No such member
    }
    for part in parts[2..parts.len()-1].iter() { // skip 'this' and string to complete
        if let InferredType::Object(fields) = current {
            if let Some((_, field_typ)) = fields.iter().find(|(k, _)| k == part) {
                match field_typ {
                    InferredType::Object(_) => current = field_typ.clone(),
                    _ => return vec![], // Can't navigate into non-object types
                }
            } else {
                return vec![]; // No such field
            }
        } else {
            return vec![]; // Current type isn't an object, can't navigate further
        }
    }
    // Now complete the last part
    let last_part = parts.last().unwrap();
    if let InferredType::Object(fields) = current {
        // Create a fake descriptor for the current type to find members
        fields
            .iter()
            .filter_map(|(k, _)| {
                if k.starts_with(last_part) {
                    Some((k.clone(), MemberKind::Field))
                } else {
                    None
                }
            })
            .collect()
    } else {
        vec![]// Current type isn't an object, can't have members
    }
}