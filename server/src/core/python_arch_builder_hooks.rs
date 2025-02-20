use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use tracing::warn;
use crate::core::symbols::symbol::Symbol;
use crate::threads::SessionInfo;
use crate::S;

use super::odoo::SyncOdoo;

pub struct PythonArchBuilderHooks {}

impl PythonArchBuilderHooks {

    pub fn on_class_def(session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>) {
        let mut sym = symbol.borrow_mut();
        let name = &sym.name();
        match name.as_str() {
            "BaseModel" => {
                if sym.get_main_entry_tree(session) == (vec![S!("odoo"), S!("models")], vec![S!("BaseModel")]) {
                    // ----------- env ------------
                    let env = sym.get_symbol(&(vec![], vec![S!("env")]), u32::MAX);
                    if env.is_empty() {
                        let mut range = sym.range().clone();
                        let slots = sym.get_symbol(&(vec![], vec![S!("__slots__")]), u32::MAX);
                        if slots.len() == 1 {
                            if slots.len() == 1 {
                                range = slots[0].borrow().range().clone();
                            }
                        }
                        sym.add_new_variable(session, &S!("env"), &range);
                    }
                    
                }
            },
            "Environment" => {
                if sym.get_main_entry_tree(session) == (vec![S!("odoo"), S!("api")], vec![S!("Environment")]) {
                    let new_sym = sym.get_symbol(&(vec![], vec![S!("__new__")]), u32::MAX);
                    let mut range = sym.range().clone();
                    if new_sym.len() == 1 {
                        range = new_sym[0].borrow().range().clone();
                    }
                    // ----------- env.cr ------------
                    sym.add_new_variable(session, &S!("cr"), &range);
                    // ----------- env.uid ------------
                    let uid_sym = sym.add_new_variable(session, &S!("uid"), &range);
                    uid_sym.borrow_mut().as_variable_mut().doc_string = Some(S!("The current user id (for access rights checks)"));
                    // ----------- env.context ------------
                    let context_sym = sym.add_new_variable(session, &S!("context"), &range);
                    context_sym.borrow_mut().as_variable_mut().doc_string = Some(S!("The current context"));
                    // ----------- env.su ------------
                    let su_sym = sym.add_new_variable(session, &S!("su"), &range);
                    su_sym.borrow_mut().as_variable_mut().doc_string = Some(S!("whether in superuser mode"));
                }
            },
            "Boolean" | "Integer" | "Float" | "Monetary" | "Char" | "Text" | "Html" | "Date" | "Datetime" |
            "Binary" | "Image" | "Selection" | "Reference" | "Many2one" | "Many2oneReference" | "Json" |
            "Properties" | "PropertiesDefinition" | "One2many" | "Many2many" | "Id" => {
                if sym.get_main_entry_tree(session).0 == vec![S!("odoo"), S!("fields")] {
                    // ----------- __get__ ------------
                    let get_sym = sym.get_symbol(&(vec![], vec![S!("__get__")]), u32::MAX);
                    if get_sym.is_empty() {
                        let range = sym.range().clone();
                        sym.add_new_function(session, &S!("__get__"), &range, &range.end());
                    } else {
                        if !["Id", "One2many"].contains(&name.as_str()){
                            warn!("Found __get__ function for field of name ({})", name);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn on_done(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>) {
        let name = symbol.borrow().name().clone();
        if name == "release" {
            if symbol.borrow().get_main_entry_tree(session) == (vec![S!("odoo"), S!("release")], vec![]) {
                let (maj, min, mic) = SyncOdoo::read_version(session, PathBuf::from(symbol.borrow().paths()[0].clone()));
                if maj != session.sync_odoo.version_major || min != session.sync_odoo.version_minor || mic != session.sync_odoo.version_micro {
                    session.sync_odoo.need_rebuild = true;
                }
            }
        }
    }
}