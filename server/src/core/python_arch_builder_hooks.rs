use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use crate::core::symbols::symbol::MainSymbol;
use crate::constants::*;
use crate::threads::SessionInfo;
use crate::S;

use super::odoo::SyncOdoo;

pub struct PythonArchBuilderHooks {}

impl PythonArchBuilderHooks {

    pub fn on_class_def(session: &mut SessionInfo, localized_sym: Rc<RefCell<MainSymbol>>) {
        let mut loc = localized_sym.borrow_mut();
        let symbol = loc.symbol.upgrade().unwrap();
        let mut symbol = symbol.borrow_mut();
        let name = &symbol.name;
        match name.as_str() {
            "BaseModel" => {
                if symbol.get_tree() == (vec![S!("odoo"), S!("models")], vec![S!("BaseModel")]) {
                    // ----------- env ------------
                    let env = symbol.get_symbol(&(vec![], vec![S!("env")]));
                    if env.is_none() {
                        let mut range = loc.range.clone();
                        let slots = symbol.get_symbol(&(vec![], vec![S!("__slots__")]));
                        if let Some(slots) = slots {
                            let slots = slots.borrow();
                            let loc_slots = slots.get_loc_sym(u32::MAX);
                            if loc_slots.len() == 1 {
                                range = loc_slots[0].borrow().range;
                            }
                        }
                        let mut env = symbol.create_or_get_symbol(session, "env", SymType::CONTENT);
                        env.borrow_mut().new_localized_symbol(SymType::VARIABLE, range);
                    }
                }
            },
            "Environment" => {
                if symbol.get_tree() == (vec![S!("odoo"), S!("api")], vec![S!("Environment")]) {
                    let new_sym = symbol.get_symbol(&(vec![], vec![S!("__new__")]));
                    let mut range = loc.range.clone();
                    if let Some(new_sym) = new_sym {
                        let new_sym_borrowed = new_sym.borrow();
                        let new_sym_loc = new_sym_borrowed.get_loc_sym(u32::MAX);
                        if new_sym_loc.len() == 1 {
                            range = new_sym_loc[0].borrow().range.clone();
                        }
                    }
                    // ----------- env.cr ------------
                    let mut cr_sym = symbol.create_or_get_symbol(session, "cr", SymType::CONTENT);
                    cr_sym.borrow_mut().new_localized_symbol(SymType::VARIABLE, range);
                    // ----------- env.uid ------------
                    let mut uid_sym = symbol.create_or_get_symbol(session, "uid", SymType::CONTENT);
                    let uid_loc = uid_sym.borrow_mut().new_localized_symbol(SymType::VARIABLE, range);
                    uid_loc.borrow_mut().doc_string = Some(S!("The current user id (for access rights checks)"));
                    // ----------- env.context ------------
                    let mut context_sym = symbol.create_or_get_symbol(session, "context", SymType::CONTENT);
                    let context_loc = context_sym.borrow_mut().new_localized_symbol(SymType::VARIABLE, range);
                    context_loc.borrow_mut().doc_string = Some(S!("The current context"));
                    // ----------- env.su ------------
                    let mut su_sym = symbol.create_or_get_symbol(session, "su", SymType::CONTENT);
                    let su_loc = su_sym.borrow_mut().new_localized_symbol(SymType::VARIABLE, range);
                    su_loc.borrow_mut().doc_string = Some(S!("whether in superuser mode"));
                }
            },
            "Boolean" | "Integer" | "Float" | "Monetary" | "Char" | "Text" | "Html" | "Date" | "Datetime" |
            "Binary" | "Image" | "Selection" | "Reference" | "Many2one" | "Many2oneReference" | "Json" |
            "Properties" | "PropertiesDefinition" | "One2many" | "Many2many" | "Id" => {
                if symbol.get_tree().0 == vec![S!("odoo"), S!("fields")] {
                    if vec![S!("Many2one"), S!("Many2many"), S!("One2many")].contains(&symbol.name) {
                        //TODO how to do this?
                    }
                    // ----------- __get__ ------------
                    let get_sym = symbol.get_symbol(&(vec![], vec![S!("__get__")]));
                    if get_sym.is_none() {
                        let mut get_sym = symbol.create_or_get_symbol(session, "__get__", SymType::CONTENT);
                        let get_loc = get_sym.borrow_mut().new_localized_symbol(SymType::VARIABLE, loc.range.clone());
                    }
                }
            }
            _ => {}
        }
    }

    pub fn on_done(session: &mut SessionInfo, symbol: &Rc<RefCell<MainSymbol>>) {
        let name = symbol.borrow().name.clone();
        if name == "release" {
            if symbol.borrow().get_tree() == (vec![S!("odoo"), S!("release")], vec![]) {
                let (maj, min, mic) = SyncOdoo::read_version(session, PathBuf::from(symbol.borrow().paths[0].clone()));
                if maj != session.sync_odoo.version_major || min != session.sync_odoo.version_minor || mic != session.sync_odoo.version_micro {
                    session.sync_odoo.need_rebuild = true;
                }
            }
        }
    }
}