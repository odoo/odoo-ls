use std::rc::Rc;
use std::cell::RefCell;
use crate::core::symbol::Symbol;
use crate::constants::*;
use crate::threads::SessionInfo;
use crate::S;

pub struct PythonArchBuilderHooks {}

impl PythonArchBuilderHooks {

    pub fn on_class_def(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>) {
        let mut sym = symbol.borrow_mut();
        let name = &sym.name;
        match name.as_str() {
            "BaseModel" => {
                if sym.get_tree() == (vec![S!("odoo"), S!("models")], vec![S!("BaseModel")]) {
                    // ----------- env ------------
                    let env = sym.get_symbol(&(vec![], vec![S!("env")]));
                    if env.is_none() {
                        let mut env = Symbol::new(S!("env"), SymType::VARIABLE);
                        let slots = sym.get_symbol(&(vec![], vec![S!("__slots__")]));
                        if slots.is_some() {
                            env.range = slots.unwrap().borrow().range.clone();
                        } else {
                            env.range = sym.range.clone();
                        }
                        let env = sym.add_symbol(session, env);
                    }
                }
            },
            "Environment" => {
                if sym.get_tree() == (vec![S!("odoo"), S!("api")], vec![S!("Environment")]) {
                    let new_sym = sym.get_symbol(&(vec![], vec![S!("__new__")]));
                    let mut range = sym.range.clone();
                    if new_sym.is_some() {
                        range = new_sym.unwrap().borrow().range.clone();
                    }
                    // ----------- env.cr ------------
                    let mut cr_sym = Symbol::new(S!("cr"), SymType::VARIABLE);
                    cr_sym.range = range.clone();
                    sym.add_symbol(session, cr_sym);
                    // ----------- env.uid ------------
                    let mut uid_sym = Symbol::new(S!("uid"), SymType::VARIABLE);
                    uid_sym.range = range.clone();
                    uid_sym.doc_string = Some(S!("The current user id (for access rights checks)"));
                    sym.add_symbol(session, uid_sym);
                    // ----------- env.context ------------
                    let mut context_sym = Symbol::new(S!("context"), SymType::VARIABLE);
                    context_sym.range = range.clone();
                    context_sym.doc_string = Some(S!("The current context"));
                    sym.add_symbol(session, context_sym);
                    // ----------- env.su ------------
                    let mut su_sym = Symbol::new(S!("su"), SymType::VARIABLE);
                    su_sym.range = range.clone();
                    su_sym.doc_string = Some(S!("whether in superuser mode"));
                    sym.add_symbol(session, su_sym);
                }
            },
            "Boolean" | "Integer" | "Float" | "Monetary" | "Char" | "Text" | "Html" | "Date" | "Datetime" |
            "Binary" | "Image" | "Selection" | "Reference" | "Many2one" | "Many2oneReference" | "Json" |
            "Properties" | "PropertiesDefinition" | "One2many" | "Many2many" | "Id" => {
                if sym.get_tree().0 == vec![S!("odoo"), S!("fields")] {
                    if vec![S!("Many2one"), S!("Many2many"), S!("One2many")].contains(&sym.name) {
                        //TODO how to do this?
                    }
                    // ----------- __get__ ------------
                    let get_sym = sym.get_symbol(&(vec![], vec![S!("__get__")]));
                    if get_sym.is_none() {
                        let mut get_sym = Symbol::new(S!("__get__"), SymType::FUNCTION);
                        get_sym.range = sym.range.clone();
                        sym.add_symbol(session, get_sym);
                    }
                }
            }
            _ => {}
        }
    }
}