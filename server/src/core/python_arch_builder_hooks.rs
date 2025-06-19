use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use once_cell::sync::Lazy;
use ruff_text_size::{TextRange, TextSize};
use tracing::warn;
use crate::core::entry_point::EntryPoint;
use crate::core::symbols::symbol::Symbol;
use crate::threads::SessionInfo;
use crate::{Sy, S};
use crate::constants::OYarn;

use super::odoo::SyncOdoo;

type PythonArchClassHookFn = fn (session: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>);

pub struct PythonArchClassHook {
    pub odoo_entry: bool,
    pub trees: Vec<(OYarn, OYarn, (Vec<OYarn>, Vec<OYarn>))>,
    pub func: PythonArchClassHookFn
}

#[allow(non_upper_case_globals)]
static arch_class_hooks: Lazy<Vec<PythonArchClassHook>> = Lazy::new(|| {vec![
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel")]))
        ],
        func: |session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
            // ----------- env ------------
            let env = symbol.borrow().get_symbol(&(vec![], vec![Sy!("env")]), u32::MAX);
            if env.is_empty() {
                let mut range = symbol.borrow().range().clone();
                let slots = symbol.borrow().get_symbol(&(vec![], vec![Sy!("__slots__")]), u32::MAX);
                if slots.len() == 1 {
                    if slots.len() == 1 {
                        range = slots[0].borrow().range().clone();
                    }
                }
                symbol.borrow_mut().add_new_variable(session, Sy!("env"), &range);
            }
        }
    },
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("environments")], vec![Sy!("Environment")]))
        ],
        func: |session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
            let new_sym = symbol.borrow().get_symbol(&(vec![], vec![Sy!("__new__")]), u32::MAX);
            let mut range = symbol.borrow().range().clone();
            if new_sym.len() == 1 {
                range = new_sym[0].borrow().range().clone();
            }
            // ----------- env.cr ------------
            symbol.borrow_mut().add_new_variable(session, Sy!("cr"), &range);
            // ----------- env.uid ------------
            let uid_sym = symbol.borrow_mut().add_new_variable(session, Sy!("uid"), &range);
            uid_sym.borrow_mut().as_variable_mut().doc_string = Some(S!("The current user id (for access rights checks)"));
            // ----------- env.context ------------
            let context_sym = symbol.borrow_mut().add_new_variable(session, Sy!("context"), &range);
            context_sym.borrow_mut().as_variable_mut().doc_string = Some(S!("The current context"));
            // ----------- env.su ------------
            let su_sym = symbol.borrow_mut().add_new_variable(session, Sy!("su"), &range);
            su_sym.borrow_mut().as_variable_mut().doc_string = Some(S!("whether in superuser mode"));
            // ----------- env.registry -----------
            let _ = symbol.borrow_mut().add_new_variable(session, Sy!("registry"), &range);
        }
    },
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Boolean")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Integer")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Float")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Monetary")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Char")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Text")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Html")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Date")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Datetime")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Binary")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Image")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Selection")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Reference")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2one")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2oneReference")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Json")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Properties")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("PropertiesDefinition")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("One2many")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2many")])),
            (Sy!("0.0"), Sy!("18.0"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Id")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_misc")], vec![Sy!("Boolean")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_numeric")], vec![Sy!("Integer")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_numeric")], vec![Sy!("Float")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_numeric")], vec![Sy!("Monetary")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_textual")], vec![Sy!("Char")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_textual")], vec![Sy!("Text")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_textual")], vec![Sy!("Html")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_temporal")], vec![Sy!("Date")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_temporal")], vec![Sy!("Datetime")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_binary")], vec![Sy!("Binary")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_binary")], vec![Sy!("Image")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_selection")], vec![Sy!("Selection")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_reference")], vec![Sy!("Reference")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("Many2one")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_reference")], vec![Sy!("Many2oneReference")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_misc")], vec![Sy!("Json")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_properties")], vec![Sy!("Properties")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_properties")], vec![Sy!("PropertiesDefinition")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("One2many")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("Many2many")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_misc")], vec![Sy!("Id")])),
        ],
        func: |session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
            // ----------- __get__ ------------
            let get_sym = symbol.borrow().get_symbol(&(vec![], vec![Sy!("__get__")]), u32::MAX);
            if get_sym.is_empty() {
                let range = symbol.borrow().range().clone();
                symbol.borrow_mut().add_new_function(session, &S!("__get__"), &range, &range.end());
            } else {
                if !["Id", "One2many"].contains(&symbol.borrow().name().as_str()){
                    warn!("Found __get__ function for field of name ({})", symbol.borrow().name());
                }
            }
            // ----------- __init__ ------------
            let get_sym = symbol.borrow().get_symbol(&(vec![], vec![Sy!("__init__")]), u32::MAX);
            if get_sym.is_empty() {
                let range = symbol.borrow().range().clone();
                symbol.borrow_mut().add_new_function(session, &S!("__init__"), &range, &range.end());
            }
        }
    },
]});

pub struct PythonArchBuilderHooks {}

impl PythonArchBuilderHooks {

    pub fn on_class_def(session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>) {
        let tree = symbol.borrow().get_tree();
        let odoo_tree = symbol.borrow().get_main_entry_tree(session);
        let name = symbol.borrow().name().clone();
        for hook in arch_class_hooks.iter() {
            for hook_tree in hook.trees.iter() {
                if hook_tree.0 >= session.sync_odoo.full_version || 
                    hook_tree.1 <= session.sync_odoo.full_version {
                    continue; //skip if version not in range
                }
                if name.eq(hook_tree.2.1.last().unwrap()) {
                    if (hook.odoo_entry && session.sync_odoo.has_main_entry && odoo_tree == hook_tree.2) || (!hook.odoo_entry && tree == hook_tree.2) {
                        (hook.func)(session, entry_point, symbol.clone());
                    }
                }
            }
        }
    }

    pub fn on_done(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>) {
        let name = symbol.borrow().name().clone();
        if name == "release" {
            if symbol.borrow().get_main_entry_tree(session) == (vec![Sy!("odoo"), Sy!("release")], vec![]) {
                let (maj, min, mic) = SyncOdoo::read_version(session, PathBuf::from(symbol.borrow().paths()[0].clone()));
                if maj != session.sync_odoo.version_major || min != session.sync_odoo.version_minor || mic != session.sync_odoo.version_micro {
                    session.sync_odoo.need_rebuild = true;
                }
            }
        } else if name == "init" {
            if session.sync_odoo.full_version.as_str() >= "18.1" {
                if symbol.borrow().get_main_entry_tree(session) == (vec![Sy!("odoo"), Sy!("init")], vec![]) {
                    let odoo_namespace = session.sync_odoo.get_symbol(symbol.borrow().paths()[0].as_str(), &(vec![Sy!("odoo")], vec![]), u32::MAX);
                    if let Some(odoo_namespace) = odoo_namespace.get(0) {
                        // create _ and Command as ext_symbols
                        let owner = symbol.clone();
                        odoo_namespace.borrow_mut().add_new_ext_symbol(session, Sy!("SUPERUSER_ID"), &TextRange::new(TextSize::new(0), TextSize::new(0)), &owner);
                        odoo_namespace.borrow_mut().add_new_ext_symbol(session, Sy!("_"), &TextRange::new(TextSize::new(0), TextSize::new(0)), &owner);
                        odoo_namespace.borrow_mut().add_new_ext_symbol(session, Sy!("_lt"), &TextRange::new(TextSize::new(0), TextSize::new(0)), &owner);
                        odoo_namespace.borrow_mut().add_new_ext_symbol(session, Sy!("Command"), &TextRange::new(TextSize::new(0), TextSize::new(0)), &owner);
                    }
                }
            }
        }
    }
}