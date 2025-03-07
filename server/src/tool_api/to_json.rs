use std::{cell::RefCell, rc::Weak};

use serde_json::json;
use weak_table::PtrWeakHashSet;

use crate::core::symbols::symbol::Symbol;


pub fn dependencies_to_json(dependencies: &[Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 4]) -> serde_json::Value {
    json!({
    "arch": json!({
        "arch": json!(dependencies[0][0].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        }),
    "arch_eval": json!({
        "arch": json!(dependencies[1][0].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        }),
    "odoo": json!({
        "arch": json!(dependencies[2][0].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        "arch_eval": json!(dependencies[2][1].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        "odoo": json!(dependencies[2][2].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        }),
    "validation": json!({
        "arch": json!(dependencies[3][0].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        "arch_eval": json!(dependencies[3][1].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        "odoo": json!(dependencies[3][2].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        }),
    })
}
pub fn dependents_to_json(dependents: &[Vec<PtrWeakHashSet<Weak<RefCell<Symbol>>>>; 3]) -> serde_json::Value {
    json!({
    "arch": json!({
        "arch": json!(dependents[0][0].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        "arch_eval": json!(dependents[0][1].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        "odoo": json!(dependents[0][2].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        "validation": json!(dependents[0][3].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        }),
    "arch_eval": json!({
        "odoo": json!(dependents[1][0].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        "validation": json!(dependents[1][1].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        }),
    "odoo": json!({
        "odoo": json!(dependents[2][0].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        "validation": json!(dependents[2][1].iter().map(|x| x.borrow().paths()).collect::<serde_json::Value>()),
        }),
    })
}