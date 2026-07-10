use oxc::ast::ast::{
    Argument, ArrayExpressionElement, AssignmentExpression, AssignmentTarget, Class, Expression, MethodDefinition, MethodDefinitionKind, ObjectPropertyKind, Program, PropertyDefinition, PropertyKey
};
use oxc::ast_visit::{Visit, walk};
use oxc::span::Span;
use ruff_text_size::{TextRange, TextSize};

use crate::threads::SessionInfo;

/// A span (byte offsets) plus the template name string value found in a template assignment.
#[derive(Debug, Clone)]
pub struct JsTemplateRef {
    /// range of the closing quote of the string literal in the JS source.
    pub range: TextRange,
    /// The template name string value (e.g. `"sale.form_view"`).
    pub t_name: String,
    /// The name of the enclosing class, if any.
    pub class_name: Option<String>,
}

fn span_to_textrange(span: Span) -> TextRange {
    TextRange::new(
        TextSize::new(span.start),
        TextSize::new(span.end),
    )
}

fn key_span(key: &PropertyKey<'_>) -> Option<Span> {
    match key {
        PropertyKey::StaticIdentifier(ident) => Some(ident.span),
        PropertyKey::StringLiteral(lit) => Some(lit.span),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub enum InferredType {
    Unknown,
    Object(Vec<(String, InferredType)>),
    Primitive,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MemberKind {
    ReactiveState,
    Field,
    Getter,
    Method,
    Prop,
    Env,
}

#[derive(Debug, Clone)]
pub struct ComponentMember {
    pub name: String,
    pub kind: MemberKind,
    pub typ: InferredType,
    pub range: TextRange,
}

#[derive(Debug, Clone)]
pub struct ComponentDescriptor {
    pub class_name: String,
    pub members: Vec<ComponentMember>,
    pub file_path: String,
}

impl ComponentDescriptor {
    pub fn find_member(&self, name: &str) -> Option<&ComponentMember> {
        self.members.iter().find(|m| m.name == name)
    }
}

fn get_key_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(ident) => Some(ident.name.to_string()),
        PropertyKey::StringLiteral(lit) => Some(lit.value.to_string()),
        _ => None,
    }
}

fn extract_shape_from_object(obj: &oxc::ast::ast::ObjectExpression<'_>) -> InferredType {
    let mut fields = vec![];
    for prop_kind in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = prop_kind else { continue };
        let Some(key) = get_key_name(&prop.key) else { continue };
        let typ = match &prop.value {
            Expression::ObjectExpression(nested) => extract_shape_from_object(nested),
            Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_) => InferredType::Primitive,
            Expression::ArrayExpression(_) => InferredType::Unknown,
            _ => InferredType::Unknown,
        };
        fields.push((key, typ));
    }
    InferredType::Object(fields)
}

/// Walks an OXC AST and collects every OWL `template` string assignment.
///
/// Detected patterns:
/// - `static template = "module.name"` (class property definition)
struct JSArchBuilderVisitor {
    file_path: String,
    refs: Vec<JsTemplateRef>,
    class_stack: Vec<String>,
    in_setup: bool,
    descriptor_stack: Vec<ComponentDescriptor>,
    descriptors: Vec<ComponentDescriptor>,
}

impl JSArchBuilderVisitor {
    fn new(file_path: String) -> Self {
        Self {
            file_path,
            refs: vec![],
            class_stack: vec![],
            in_setup: false,
            descriptor_stack: vec![],
            descriptors: vec![],
        }
    }
}

impl<'a> Visit<'a> for JSArchBuilderVisitor {
    fn visit_class(&mut self, it: &Class<'a>) {
        let class_name = it.id.as_ref().map(|id| id.name.to_string());
        if let Some(ref name) = class_name {
            self.class_stack.push(name.clone());
            let mut desc = ComponentDescriptor {
                class_name: name.clone(),
                members: vec![ComponentMember {
                    name: "env".to_string(),
                    kind: MemberKind::Env,
                    typ: InferredType::Unknown,
                    range: TextRange::default(),
                }],
                file_path: self.file_path.clone(),
            };
            // Add `props` as a member (always present)
            desc.members.push(ComponentMember {
                name: "props".to_string(),
                kind: MemberKind::Prop,
                typ: InferredType::Unknown,
                range: TextRange::default(),
            });
            self.descriptor_stack.push(desc);
        }
        walk::walk_class(self, it);
        if class_name.is_some() {
            self.class_stack.pop();
        }
        if let Some(desc) = self.descriptor_stack.pop() {
            if !desc.class_name.is_empty() {
                self.descriptors.push(desc);
            }
        }
    }

    /// Catch `static template = "..."` inside a class body.
    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        if it.r#static {
            let key_name = get_key_name(&it.key);
            if key_name.as_deref() == Some("template") {
                if let Some(Expression::StringLiteral(lit)) = &it.value {
                    let content_start = lit.span.start + 1;
                    let content_end = lit.span.end.saturating_sub(1);
                    self.refs.push(JsTemplateRef {
                        range: TextRange::new(
                            TextSize::new(content_start),
                            TextSize::new(content_end),
                        ),
                        t_name: lit.value.to_string(),
                        class_name: self.class_stack.last().cloned(),
                    });
                    return; // no need to recurse into value
                }
            } else if key_name.as_deref() == Some("props") {
                if let Some(desc) = self.descriptor_stack.last_mut() {
                    match &it.value {
                        Some(Expression::ObjectExpression(obj)) => {
                            for prop_kind in &obj.properties {
                                let ObjectPropertyKind::ObjectProperty(p) = prop_kind else {
                                    continue //TODO implement SpreadProperty
                                };
                                if let Some(prop_name) = get_key_name(&p.key) {
                                    let range = key_span(&p.key)
                                        .map(|s| span_to_textrange(s))
                                        .unwrap_or_default();
                                    desc.members.push(ComponentMember {
                                        name: prop_name,
                                        kind: MemberKind::Prop,
                                        typ: InferredType::Unknown,
                                        range,
                                    });
                                }
                            }
                        }
                        Some(Expression::ArrayExpression(arr)) => {
                            for elem in &arr.elements {
                                if let ArrayExpressionElement::StringLiteral(s) = elem {
                                    let range = span_to_textrange(s.span);
                                    desc.members.push(ComponentMember {
                                        name: s.value.to_string(),
                                        kind: MemberKind::Prop,
                                        typ: InferredType::Unknown,
                                        range,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
                return;
            }
        } else if let Some(desc) = self.descriptor_stack.last_mut() {
            // Instance field: `myField = something`
            if let Some(name) = get_key_name(&it.key) {
                if !name.starts_with('#') {
                    let range = key_span(&it.key)
                        .map(|s| span_to_textrange(s))
                        .unwrap_or_default();
                    desc.members.push(ComponentMember {
                        name,
                        kind: MemberKind::Field,
                        typ: InferredType::Unknown,
                        range,
                    });
                }
            }
        }
        walk::walk_property_definition(self, it);
    }

    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        let name = get_key_name(&it.key);
        let was_in_setup = self.in_setup;

        if name.as_deref() == Some("setup") && !it.r#static {
            self.in_setup = true;
        }

        if let Some(ref name) = name {
            if let Some(desc) = self.descriptor_stack.last_mut() {
                let range = key_span(&it.key)
                    .map(|s| span_to_textrange(s))
                    .unwrap_or_default();
                match it.kind {
                    MethodDefinitionKind::Get if !it.r#static => {
                        desc.members.push(ComponentMember {
                            name: name.clone(),
                            kind: MemberKind::Getter,
                            typ: InferredType::Unknown,
                            range,
                        });
                    }
                    MethodDefinitionKind::Method
                        if !it.r#static && name != "setup" && name != "constructor" =>
                    {
                        desc.members.push(ComponentMember {
                            name: name.clone(),
                            kind: MemberKind::Method,
                            typ: InferredType::Unknown,
                            range,
                        });
                    }
                    _ => {}
                }
            }
        }

        walk::walk_method_definition(self, it);
        self.in_setup = was_in_setup;
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if self.in_setup {
            if let AssignmentTarget::StaticMemberExpression(member) = &it.left {
                if matches!(member.object, Expression::ThisExpression(_)) {
                    let field_name = member.property.name.to_string();
                    let field_range = span_to_textrange(member.property.span);
                    let typ = match &it.right {
                        Expression::CallExpression(call) => {
                            let callee_name = match &call.callee {
                                Expression::Identifier(id) => Some(id.name.as_str()),
                                _ => None,
                            };
                            if callee_name == Some("useState") {
                                if let Some(first_arg) = call.arguments.first() {
                                    match first_arg {
                                        Argument::ObjectExpression(obj) => {
                                            extract_shape_from_object(obj)
                                        }
                                        _ => InferredType::Unknown,
                                    }
                                } else {
                                    InferredType::Unknown
                                }
                            } else {
                                InferredType::Unknown
                            }
                        }
                        _ => InferredType::Unknown,
                    };
                    if let Some(desc) = self.descriptor_stack.last_mut() {
                        // Replace existing field with more specific ReactiveState info
                        if let Some(existing) = desc.members.iter_mut().find(|m| m.name == field_name) {
                            existing.kind = MemberKind::ReactiveState;
                            existing.typ = typ;
                            existing.range = field_range;
                        } else {
                            desc.members.push(ComponentMember {
                                name: field_name,
                                kind: MemberKind::ReactiveState,
                                typ,
                                range: field_range,
                            });
                        }
                    }
                    return;
                }
            }
        }
        walk::walk_assignment_expression(self, it);
    }
}

pub fn visit_file(program: &Program<'_>, file_path: &str) -> (Vec<JsTemplateRef>, Vec<ComponentDescriptor>) {
    let mut visitor = JSArchBuilderVisitor::new(file_path.to_string());
    visitor.visit_program(program);
    (
        visitor
        .refs
        .into_iter()
        .collect(),

        visitor.descriptors,
    )
}

pub fn build(session: &mut SessionInfo, templates_ref: &Vec<JsTemplateRef>, components: &Vec<ComponentDescriptor>) {
    // Populate template→class_name mapping
    for tr in templates_ref.iter() {
        if let Some(cn) = &tr.class_name {
            session.sync_odoo.js_component_by_template.insert(tr.t_name.clone(), cn.clone());
        }
    }

    // Populate component descriptors from this file
    for descriptor in components.iter() {
        session.sync_odoo.component_descriptors.insert(descriptor.class_name.clone(), descriptor.clone());
    }
}