use std::cmp::Ordering;
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use once_cell::sync::Lazy;
use ruff_text_size::{TextRange, TextSize};
use tracing::{info, warn};
use crate::core::entry_point::EntryPoint;
use crate::core::import_resolver::manual_import;
use crate::core::symbols::symbol_table::SymbolTable;
use crate::core::symbols::symbol_keys::{ClassKey, SymbolKey};
use crate::threads::SessionInfo;
use crate::utils::compare_semver;
use crate::{Sy, S};
use crate::constants::OYarn;

use super::odoo::SyncOdoo;

// @arena: currently, none of the class hooks use entry_point
type PythonArchClassHookFn = fn (symbol_table: &mut SymbolTable, entry: &Rc<RefCell<EntryPoint>>, class: ClassKey);

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
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel")]))
        ],
        func: |symbol_table: &mut SymbolTable, _entry_point: &Rc<RefCell<EntryPoint>>, class: ClassKey| {
            // ----------- env ------------
            let symbol_key: SymbolKey = class.into();
            let env = symbol_table.get_symbol(symbol_key, &(vec![], vec![Sy!("env")]), u32::MAX);
            if env.is_empty() {
                let mut range = symbol_table[class].range.clone();
                let slots = symbol_table.get_symbol(symbol_key, &(vec![], vec![Sy!("__slots__")]), u32::MAX);
                if slots.len() == 1 {
                    range = symbol_table.range(slots[0]).clone();
                }
                symbol_table.add_new_variable(symbol_key, Sy!("env"), &range);
            }
        }
    },
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment")])),
            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("environments")], vec![Sy!("Environment")]))
        ],
        func: |symbol_table: &mut SymbolTable, _entry_point: &Rc<RefCell<EntryPoint>>, class: ClassKey| {
            let new_sym = symbol_table.get_symbol(class.into(), &(vec![], vec![Sy!("__new__")]), u32::MAX);
            let mut range = symbol_table[class].range.clone();
            if new_sym.len() == 1 {
                range = symbol_table.range(new_sym[0]).clone();
            }
            // ----------- env.cr ------------
            symbol_table.add_new_variable(class, Sy!("cr"), &range);
            // ----------- env.uid ------------
            let uid_sym = symbol_table.add_new_variable(class, Sy!("uid"), &range);
            symbol_table[uid_sym].doc_string = Some(S!("The current user id (for access rights checks)"));
            // ----------- env.context ------------
            let context_sym = symbol_table.add_new_variable(class, Sy!("context"), &range);
            symbol_table[context_sym].doc_string = Some(S!("The current context"));
            // ----------- env.su ------------
            let su_sym = symbol_table.add_new_variable(class, Sy!("su"), &range);
            symbol_table[su_sym].doc_string = Some(S!("whether in superuser mode"));
            // ----------- env.registry -----------
            let _ = symbol_table.add_new_variable(class, Sy!("registry"), &range);
        }
    },
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            (Sy!("15.0"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("addons"), Sy!("base"), Sy!("models"), Sy!("ir_rule")], vec![Sy!("IrRule")])),
        ],
        func: |symbol_table: &mut SymbolTable, _entry_point: &Rc<RefCell<EntryPoint>>, class: ClassKey| {
            let range = symbol_table[class].range.clone();
            // ----------- global ------------
            symbol_table.add_new_variable(class, Sy!("global"), &range);
        }
    },
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Boolean")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Integer")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Float")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Monetary")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Char")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Text")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Html")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Date")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Datetime")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Binary")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Image")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Selection")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Reference")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2one")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2oneReference")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Json")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Properties")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("PropertiesDefinition")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("One2many")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2many")])),
            (Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Id")])),
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
        func: |symbol_table: &mut SymbolTable, _entry_point: &Rc<RefCell<EntryPoint>>, class: ClassKey| {
            let symbol_key: SymbolKey = class.into();
            let range = symbol_table[class].range.clone();
            // ----------- __get__ ------------
            let get_sym = symbol_table.get_symbol(symbol_key, &(vec![], vec![Sy!("__get__")]), u32::MAX);
            if get_sym.is_empty() {
                symbol_table.add_new_function(symbol_key, &S!("__get__"), &range, &range.end());
            } else {
                let name = &symbol_table[class].name;
                if !["Id", "One2many"].contains(&name.as_str()) {
                    warn!("Found __get__ function for field of name ({})", name);
                }
            }
            // ----------- __init__ ------------
            let get_sym = symbol_table.get_symbol(symbol_key, &(vec![], vec![Sy!("__init__")]), u32::MAX);
            if get_sym.is_empty() {
                symbol_table.add_new_function(symbol_key, &S!("__init__"), &range, &range.end());
            }
        }
    },
]});

pub struct PythonArchBuilderHooks {}

impl PythonArchBuilderHooks {

    pub fn on_class_def(session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, class_key: ClassKey) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let symbol_key: SymbolKey = class_key.into();
        let tree = st!().get_tree(symbol_key);
        let odoo_tree = SymbolTable::get_main_entry_tree(session, symbol_key);
        let name = st!().name(symbol_key).clone();
        for hook in arch_class_hooks.iter() {
            for hook_tree in hook.trees.iter() {
                if compare_semver(session.sync_odoo.full_version.as_str(), hook_tree.0.as_str()) >= Ordering::Equal &&
                    compare_semver(session.sync_odoo.full_version.as_str(), hook_tree.1.as_str()) == Ordering::Less {
                    if name.eq(hook_tree.2.1.last().unwrap()) {
                        if (hook.odoo_entry && session.sync_odoo.has_main_entry && odoo_tree == hook_tree.2) || (!hook.odoo_entry && tree == hook_tree.2) {
                            (hook.func)(&mut session.sync_odoo.symbol_table, entry_point, class_key);
                        }
                    }
                }
            }
        }
    }

    // @arena: all if-blocks apply to a file symbol (double check this)
    pub fn on_done(session: &mut SessionInfo, symbol: SymbolKey) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let name = st!().name(symbol).clone();
        if name == "release" {
            if SymbolTable::get_main_entry_tree(session, symbol) == (vec![Sy!("odoo"), Sy!("release")], vec![]) {
                let file_path = &st!()[symbol.unwrap_file_key()].path;
                let (maj, min, mic) = SyncOdoo::read_version(session, PathBuf::from(file_path));
                if maj != session.sync_odoo.version_major || min != session.sync_odoo.version_minor || mic != session.sync_odoo.version_micro {
                    session.sync_odoo.need_rebuild = true;
                }
            }
        } else if name == "init" {
            if compare_semver(session.sync_odoo.full_version.as_str(), "18.1") != Ordering::Less {
                if SymbolTable::get_main_entry_tree(session, symbol) == (vec![Sy!("odoo"), Sy!("init")], vec![]) {
                    let file_path = &st!()[symbol.unwrap_file_key()].path;
                    let odoo_namespace = session.sync_odoo.get_symbol(file_path, &(vec![Sy!("odoo")], vec![]), u32::MAX);
                    if let Some(&odoo_namespace) = odoo_namespace.get(0) {
                        // create _ and Command as ext_symbols
                        let owner = symbol;
                        st!().add_new_ext_symbol(odoo_namespace, "SUPERUSER_ID", &TextRange::new(TextSize::new(0), TextSize::new(0)), owner);
                        st!().add_new_ext_symbol(odoo_namespace, "_", &TextRange::new(TextSize::new(0), TextSize::new(0)), owner);
                        st!().add_new_ext_symbol(odoo_namespace, "_lt", &TextRange::new(TextSize::new(0), TextSize::new(0)), owner);
                        st!().add_new_ext_symbol(odoo_namespace, "Command", &TextRange::new(TextSize::new(0), TextSize::new(0)), owner);
                    }
                }
            }
        } else if name == "werkzeug" {
            if SymbolTable::get_main_entry_tree(session, symbol) == (vec![Sy!("odoo"), Sy!("_monkeypatches"), Sy!("werkzeug")], vec![]) {
                //doing this patch like this imply that an odoo project will make these functions available for all entrypoints, but heh
                let file_path = &st!()[symbol.unwrap_file_key()].path;
                let werkzeug_url = session.sync_odoo.get_symbol(file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![]), u32::MAX);
                if let Some(&werkzeug_url) = werkzeug_url.first() {
                    let url_join = st!().get_symbol(werkzeug_url, &(vec![], vec![Sy!("url_join")]), u32::MAX);
                    if url_join.is_empty() { //else, installed version is already patched
                        //fake variable, as ext_symbols are not seen through get_symbol, etc...
                        st!().add_new_variable(werkzeug_url, Sy!("url_decode"), &TextRange::new(TextSize::new(0), TextSize::new(0)));
                        st!().add_new_variable(werkzeug_url, Sy!("url_encode"), &TextRange::new(TextSize::new(0), TextSize::new(0)));
                        st!().add_new_variable(werkzeug_url, Sy!("url_join"), &TextRange::new(TextSize::new(0), TextSize::new(0)));
                        st!().add_new_variable(werkzeug_url, Sy!("url_parse"), &TextRange::new(TextSize::new(0), TextSize::new(0)));
                        st!().add_new_variable(werkzeug_url, Sy!("url_quote"), &TextRange::new(TextSize::new(0), TextSize::new(0)));
                        st!().add_new_variable(werkzeug_url, Sy!("url_unquote"), &TextRange::new(TextSize::new(0), TextSize::new(0)));
                        st!().add_new_variable(werkzeug_url, Sy!("url_quote_plus"), &TextRange::new(TextSize::new(0), TextSize::new(0)));
                        st!().add_new_variable(werkzeug_url, Sy!("url_unquote_plus"), &TextRange::new(TextSize::new(0), TextSize::new(0)));
                        st!().add_new_variable(werkzeug_url, Sy!("url_unparse"), &TextRange::new(TextSize::new(0), TextSize::new(0)));
                        st!().add_new_variable(werkzeug_url, Sy!("URL"), &TextRange::new(TextSize::new(0), TextSize::new(0)));
                    }
                } else {
                    warn!("Unable to find werkzeug.urls to monkeypatch it");
                }
            }
        } else if name == "urls" {
            if st!().get_local_tree(symbol) == (vec![Sy!("werkzeug"), Sy!("urls")], vec![]) {
                //manually load patch, as a manual dependency
                let full_path_monkeypatches = S!("odoo._monkeypatches");
                let mut main_odoo_symbol = None;
                if let Some(main_ep) = session.sync_odoo.entry_point_mgr.borrow().main_entry_point.as_ref() {
                    //To import from main entry point, we have to import 'from' a symbol coming from main entry point.
                    //We then use the main symbol of the main entry point to achieve that, instead of the werkzeug symbol
                    main_odoo_symbol = main_ep.borrow().get_symbol(&st!());
                }
                if let Some(main_odoo_symbol) = main_odoo_symbol {
                    let werkzeug_patch = manual_import(session, main_odoo_symbol, Some(full_path_monkeypatches), "werkzeug", None, 0, &mut None);
                    for werkzeug_patch in werkzeug_patch {
                        if werkzeug_patch.found {
                            info!("monkeypatch manually found");
                        }
                    }
                }
            }
        }
    }
}
