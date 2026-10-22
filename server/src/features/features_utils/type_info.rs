use std::fmt::Display;

use itertools::Itertools;
use ruff_text_size::Ranged;
use crate::core::symbols::function_symbol::Argument;
use crate::core::symbols::storage::xml::xml_field_symbol::XmlFieldName;
use crate::core::symbols::symbol_keys::{SymbolKey, Wk};
use crate::core::symbols::storage::SymbolTable;

use crate::constants::SymType;
use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr};
use crate::core::evaluation_context::{Context, ContextKey, ContextValue};
use crate::threads::SessionInfo;
use crate::S;

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct CallableSignature {
    pub arguments: String,
    pub return_types: String,
}
#[derive(Clone, Eq, PartialEq, Hash)]
pub enum TypeInfo {
    CALLABLE(CallableSignature),
    VALUE(String),
}

impl Display for TypeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            TypeInfo::CALLABLE(CallableSignature { arguments, return_types }) => format!("(({}) -> {})", arguments, return_types),
            TypeInfo::VALUE(value) => value.clone(),
        };
        write!(f, "{}", value)
    }
}

/// Given an Argument return its String representation
/// Attempts to fetch the type hint to return `_arg: _arg_type`
/// Otherwise just returns the argument name
fn argument_presentation(session: &mut SessionInfo, arg: &Argument) -> String {
    let arg_symbol = arg.symbol.upgrade(session.st()).unwrap();
    let arg_name = session.st()[arg_symbol].name.clone();
    match arg.annotation.as_ref() {
        Some(anno_expr) => {
            let Some(type_symbol) = session.st().parent(arg_symbol)
            .and_then(|parent| Evaluation::eval_from_ast(session, anno_expr, parent, &anno_expr.range().start(), false, &mut vec![]).0.first().cloned())
            .and_then(|type_evaluation| type_evaluation.symbol.get_symbol_as_weak(session, None, &mut vec![], None).weak.upgrade(session.st()))
                else {
                return arg_name.to_string()
            };
            let type_name = session.st().name(type_symbol);
            format!("{}: {}", arg_name, type_name)
        },
        None => arg_name.to_string()
    }
}

/// Return return type representation of evaluation
/// for a function evaluation it is typically (_arg: _arg_type, ...) -> (_result_type)
/// for variable it just shows the type, or Any if it fails to find it
pub fn get_inferred_types(session: &mut SessionInfo, eval: &EvaluationSymbolPtr, context: Option<&Context>, symbol_type: &SymType) -> TypeInfo {
    if *symbol_type == SymType::CLASS{
        return TypeInfo::VALUE(S!(""));
    }
    match eval {
        EvaluationSymbolPtr::WEAK(eval_weak) | EvaluationSymbolPtr::SELF(eval_weak)  => {
            if let Some(inferred_type) = eval.upgrade_weak(session.st()) {
                // let inferred_type = inferred_type.borrow();
                match inferred_type {
                    SymbolKey::Function(function_key) if !session.st()[function_key].is_property => {
                        let base_attr_ctx_ref = eval.get_weak().context.get(ContextKey::BaseAttr).or(context.and_then(|ctx| ctx.get(ContextKey::BaseAttr)));
                        let call_parent = match base_attr_ctx_ref {
                            Some(ContextValue::SYMBOL(s)) => *s,
                            _ => {
                                let parent_option = session.st().parent(inferred_type);
                                if let Some(parent) = parent_option && parent.typ() == SymType::CLASS {
                                    Wk::from(parent)
                                } else {
                                    Wk::null()
                                }
                            }
                        };
                        let mut ctx = context.cloned();
                        if let Some(ctx) = ctx.as_mut() {
                            ctx.insert(ContextKey::BaseCall, ContextValue::SYMBOL(call_parent));
                        }
                        // context.as_mut().map(|ctx| ctx.insert(ContextKey::BaseCall, ContextValue::SYMBOL(call_parent)));
                        let return_type = {
                            let func_eval = session.st()[function_key].evaluations.clone();
                            let type_names: Vec<_> = func_eval.iter().flat_map(|eval|{
                                let eval_symbol = eval.symbol.get_symbol_weak_transformed(session, ctx.as_ref(), &mut vec![], None);
                                let weak_eval_symbols = SymbolTable::follow_ref(&eval_symbol, session, ctx.as_ref(), false, false, None, None);
                                weak_eval_symbols.iter().map(|weak_eval_symbol| match weak_eval_symbol.upgrade_weak(session.st()) {
                                    //if fct is a variable, it means that evaluation is None.
                                    Some(s_type) if s_type.typ() != SymType::VARIABLE => session.st().repr(s_type).to_string(),
                                    _ => "Any".to_string()
                                }).collect::<Vec<_>>()
                            }).unique().collect();
                            if !type_names.is_empty() {represent_return_types(type_names)} else {S!("None")}
                        };
                        let argument_names = session.st()[function_key].args.clone().iter().map(|arg| argument_presentation(session, arg)).join(", ");
                        TypeInfo::CALLABLE(CallableSignature { arguments: argument_names, return_types: return_type })
                    },
                    SymbolKey::File(_) => TypeInfo::VALUE(S!("File")),
                    SymbolKey::PythonPackage(_) | SymbolKey::Module(_) => TypeInfo::VALUE(S!("Module")),
                    SymbolKey::Namespace(_) => TypeInfo::VALUE(S!("Namespace")),
                    SymbolKey::Class(class_key) => TypeInfo::VALUE(if eval_weak.is_super {format!("super[{}]", session.st()[class_key].name)} else {session.st()[class_key].name.to_string()}), // TODO: Maybe do something special if it is a descriptor
                    SymbolKey::XmlRecord(xml_field_record_key) => {
                        let xml_record = &session.st()[xml_field_record_key];
                        match xml_record.get_field_text(XmlFieldName::Type, session.st()) {
                            Some(type_name) => TypeInfo::VALUE(type_name),
                            None => TypeInfo::VALUE(S!("Any"))
                        }
                    },
                    _ => TypeInfo::VALUE(S!("Any"))
                }
            } else {
                TypeInfo::VALUE(S!("Any"))
            }
        }
        EvaluationSymbolPtr::ANY => TypeInfo::VALUE(S!("Any")),
        EvaluationSymbolPtr::NONE => TypeInfo::VALUE(S!("None")),
        _ => TypeInfo::VALUE(S!("Any")),
    }
}

/// Given a list of return types from evaluations
/// Combine them into one string, surround with parenthesis
///   if there is more than one type.
/// Always puts the Any type at the end. e.g. `(int | dict | Any)``
pub(super) fn represent_return_types(return_types: Vec<String>) -> String {
    let mut return_types = return_types.clone();
    if let Some(pos) = return_types.iter().position(|n| n == "Any") {
        let last_index = return_types.len() - 1;
        return_types.swap(pos, last_index);
    }
    let return_types_string = return_types.iter().join(" | ");
    if return_types.len() > 1 {
        format!("({})", return_types_string)
    } else {
        return_types_string
    }

}
