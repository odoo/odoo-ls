use std::{cell::RefCell, rc::Weak};

use serde_json::json;
use weak_table::PtrWeakHashSet;

use crate::core::symbols::symbol::Symbol;


pub fn dependencies_to_json(dependencies: &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>) -> serde_json::Value {
    if dependencies.is_empty() {
        return json!(null);
    }
    json!({
    "arch": json!({
        "arch": json!(dependencies[0][0].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        }),
    "arch_eval": json!({
        "arch": json!(dependencies[1][0].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        }),
    "odoo": json!({
        "arch": json!(dependencies[2][0].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        "arch_eval": json!(dependencies[2][1].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        "odoo": json!(dependencies[2][2].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        }),
    "validation": json!({
        "arch": json!(dependencies[3][0].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        "arch_eval": json!(dependencies[3][1].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        "odoo": json!(dependencies[3][2].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        }),
    })
}
pub fn dependents_to_json(dependents: &Vec<Vec<Option<PtrWeakHashSet<Weak<RefCell<Symbol>>>>>>) -> serde_json::Value {
    if dependents.is_empty() {
        return json!(null);
    }
    json!({
    "arch": json!({
        "arch": json!(dependents[0][0].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        "arch_eval": json!(dependents[0][1].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        "odoo": json!(dependents[0][2].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        "validation": json!(dependents[0][3].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        }),
    "arch_eval": json!({
        "odoo": json!(dependents[1][0].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        "validation": json!(dependents[1][1].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        }),
    "odoo": json!({
        "odoo": json!(dependents[2][0].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        "validation": json!(dependents[2][1].as_ref().map(|set| set.iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()).unwrap_or(json!(null))),
        }),
    })
}