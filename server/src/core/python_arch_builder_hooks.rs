use std::path::PathBuf;
use once_cell::sync::Lazy;
use ruff_text_size::TextRange;
use tracing::{info, warn};
use crate::core::import_resolver::manual_import;
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::symbol_keys::{ClassKey, SourceFileKey, SymbolKey};
use crate::threads::SessionInfo;
use crate::S;
use crate::tree::TreeStrSlice;

use super::odoo::SyncOdoo;

type PythonArchClassHookFn = fn (symbol_table: &mut SymbolTable, class: ClassKey);
type Version = (u32, u32); // major, minor

pub struct PythonArchClassHook {
    pub odoo_entry: bool,
    pub trees: Vec<(Version, Version, TreeStrSlice<'static>)>,
    pub func: PythonArchClassHookFn
}

#[allow(non_upper_case_globals)]
static arch_class_hooks: Lazy<Vec<PythonArchClassHook>> = Lazy::new(|| {vec![
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            ((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel"]))
        ],
        func: |symbol_table: &mut SymbolTable, class: ClassKey| {
            // ----------- env ------------
            let symbol_key: SymbolKey = class.into();
            let env = symbol_table.get_symbol(symbol_key, (&[], &["env"]), u32::MAX);
            if env.is_empty() {
                let mut range = symbol_table[class].range.clone();
                let slots = symbol_table.get_symbol(symbol_key, (&[], &["__slots__"]), u32::MAX);
                if slots.len() == 1 {
                    range = symbol_table.range(slots[0]).clone();
                }
                symbol_table.add_new_variable(symbol_key, "env", range);
            }
        }
    },
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            ((15, 3), (19, 2), (&["odoo", "http"], &["Request"])),
            ((19, 2), (999, 0), (&["odoo", "http", "requestlib"], &["Request"]))
        ],
        func: |symbol_table: &mut SymbolTable, class: ClassKey| {
            // ----------- Request.env ------------
            let has_env = !symbol_table.get_content_symbol(class.into(), "env", u32::MAX).symbols.is_empty();
            if has_env {
                return;
            }
            let range = symbol_table[class].range.clone();
            symbol_table.add_new_variable(class, "env", range);
        }
    },
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            ((0, 0), (18, 1), (&["odoo", "api"], &["Environment"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "environments"], &["Environment"]))
        ],
        func: |symbol_table: &mut SymbolTable, class: ClassKey| {
            let new_sym = symbol_table.get_symbol(class.into(), (&[], &["__new__"]), u32::MAX);
            let mut range = symbol_table[class].range.clone();
            if new_sym.len() == 1 {
                range = symbol_table.range(new_sym[0]).clone();
            }
            // ----------- env.cr ------------
            symbol_table.add_new_variable(class, "cr", range);
            // ----------- env.uid ------------
            let uid_sym = symbol_table.add_new_variable(class, "uid", range);
            symbol_table[uid_sym].doc_string = Some(S!("The current user id (for access rights checks)"));
            // ----------- env.context ------------
            let context_sym = symbol_table.add_new_variable(class, "context", range);
            symbol_table[context_sym].doc_string = Some(S!("The current context"));
            // ----------- env.su ------------
            let su_sym = symbol_table.add_new_variable(class, "su", range);
            symbol_table[su_sym].doc_string = Some(S!("whether in superuser mode"));
            // ----------- env.registry -----------
            let _ = symbol_table.add_new_variable(class, "registry", range);
        }
    },
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            ((15, 0), (999, 0), (&["odoo", "addons", "base", "models", "ir_rule"], &["IrRule"])),
        ],
        func: |symbol_table: &mut SymbolTable, class: ClassKey| {
            let range = symbol_table[class].range.clone();
            // ----------- global ------------
            symbol_table.add_new_variable(class, "global", range);
        }
    },
    PythonArchClassHook {
        odoo_entry: true,
        trees: vec![
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Boolean"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Integer"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Float"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Monetary"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Char"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Text"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Html"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Date"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Datetime"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Binary"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Image"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Selection"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Reference"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Many2one"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Many2oneReference"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Json"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Properties"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["PropertiesDefinition"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["One2many"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Many2many"])),
            ((0, 0), (18, 1), (&["odoo", "fields"], &["Id"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_misc"], &["Boolean"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_numeric"], &["Integer"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_numeric"], &["Float"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_numeric"], &["Monetary"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_textual"], &["Char"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_textual"], &["Text"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_textual"], &["Html"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_temporal"], &["Date"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_temporal"], &["Datetime"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_binary"], &["Binary"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_binary"], &["Image"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_selection"], &["Selection"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_reference"], &["Reference"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_relational"], &["Many2one"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_reference"], &["Many2oneReference"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_misc"], &["Json"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_properties"], &["Properties"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_properties"], &["PropertiesDefinition"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_relational"], &["One2many"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_relational"], &["Many2many"])),
            ((18, 1), (999, 0), (&["odoo", "orm", "fields_misc"], &["Id"])),
        ],
        func: |symbol_table: &mut SymbolTable, class: ClassKey| {
            let symbol_key: SymbolKey = class.into();
            let range = symbol_table[class].range.clone();
            // ----------- __get__ ------------
            let get_sym = symbol_table.get_symbol(symbol_key, (&[], &["__get__"]), u32::MAX);
            if get_sym.is_empty() {
                symbol_table.add_new_function(symbol_key, &S!("__get__"), range, &range.end());
            } else {
                let name = &symbol_table[class].name;
                if !["Id", "One2many"].contains(&name.as_str()) {
                    warn!("Found __get__ function for field of name ({})", name);
                }
            }
            // ----------- __init__ ------------
            let get_sym = symbol_table.get_symbol(symbol_key, (&[], &["__init__"]), u32::MAX);
            if get_sym.is_empty() {
                symbol_table.add_new_function(symbol_key, &S!("__init__"), range, &range.end());
            }
        }
    },
]});

pub struct PythonArchBuilderHooks {}

impl PythonArchBuilderHooks {

    pub fn on_class_def(session: &mut SessionInfo, class_key: ClassKey) {
        let has_main_entry = session.sync_odoo.has_main_entry;
        let mut lazy_tree = None;
        let mut lazy_odoo_tree = None;
        let name = session.st()[class_key].name.clone();
        for hook in arch_class_hooks.iter() {
            if hook.odoo_entry && !has_main_entry {
                continue;
            }
            for (min_version, max_version, hook_tree) in hook.trees.iter() {
                if !name.eq(hook_tree.1.last().unwrap()) {
                    continue; // skip if class name doesn't match
                }
                if session.sync_odoo.version < *min_version || session.sync_odoo.version >= *max_version {
                    continue; //skip if version not in range
                }
                let tree = if hook.odoo_entry {
                    lazy_odoo_tree.get_or_insert_with(|| session.sync_odoo.get_main_entry_tree(class_key))
                } else {
                    lazy_tree.get_or_insert_with(|| session.st().get_tree(class_key))
                };
                if tree == hook_tree {
                    (hook.func)(&mut session.sync_odoo.symbol_table, class_key);
                }
            }
        }
    }

    pub fn on_file_done(session: &mut SessionInfo, symbol: SourceFileKey) {
        let name = session.st().name(symbol).clone();
        if name == "release" {
            if session.sync_odoo.get_main_entry_tree(symbol) == (&["odoo", "release"], &[]) {
                let file_path = session.st().path(symbol);
                let new_version = SyncOdoo::read_version(session, PathBuf::from(file_path));
                if new_version != session.sync_odoo.version {
                    session.sync_odoo.need_rebuild = true;
                }
            }
        } else if name == "init" {
            if session.sync_odoo.version >= (18, 1) {
                if session.sync_odoo.get_main_entry_tree(symbol) == (&["odoo", "init"], &[]) {
                    let file_path = session.st().path(symbol);
                    let odoo_namespace = session.sync_odoo.get_symbol(file_path, (&["odoo"], &[]), u32::MAX);
                    if let Some(&odoo_namespace) = odoo_namespace.get(0) {
                        // create _ and Command as ext_symbols
                        let owner = symbol.into();
                        session.st_mut().add_new_ext_symbol(odoo_namespace, "SUPERUSER_ID", TextRange::default(), owner);
                        session.st_mut().add_new_ext_symbol(odoo_namespace, "_", TextRange::default(), owner);
                        session.st_mut().add_new_ext_symbol(odoo_namespace, "_lt", TextRange::default(), owner);
                        session.st_mut().add_new_ext_symbol(odoo_namespace, "Command", TextRange::default(), owner);
                    }
                }
            }
        } else if name == "werkzeug" {
            if session.sync_odoo.get_main_entry_tree(symbol) == (&["odoo", "_monkeypatches", "werkzeug"], &[]) {
                //doing this patch like this imply that an odoo project will make these functions available for all entrypoints, but heh
                let file_path = session.st().path(symbol);
                let werkzeug_url = session.sync_odoo.get_symbol(file_path, (&["werkzeug", "urls"], &[]), u32::MAX);
                if let Some(&werkzeug_url) = werkzeug_url.first() {
                    let url_join = session.st().get_symbol(werkzeug_url, (&[], &["url_join"]), u32::MAX);
                    if url_join.is_empty() { //else, installed version is already patched
                        //fake variable, as ext_symbols are not seen through get_symbol, etc...
                        session.st_mut().add_new_variable(werkzeug_url, "url_decode", TextRange::default());
                        session.st_mut().add_new_variable(werkzeug_url, "url_encode", TextRange::default());
                        session.st_mut().add_new_variable(werkzeug_url, "url_join", TextRange::default());
                        session.st_mut().add_new_variable(werkzeug_url, "url_parse", TextRange::default());
                        session.st_mut().add_new_variable(werkzeug_url, "url_quote", TextRange::default());
                        session.st_mut().add_new_variable(werkzeug_url, "url_unquote", TextRange::default());
                        session.st_mut().add_new_variable(werkzeug_url, "url_quote_plus", TextRange::default());
                        session.st_mut().add_new_variable(werkzeug_url, "url_unquote_plus", TextRange::default());
                        session.st_mut().add_new_variable(werkzeug_url, "url_unparse", TextRange::default());
                        session.st_mut().add_new_variable(werkzeug_url, "URL", TextRange::default());
                    }
                } else {
                    warn!("Unable to find werkzeug.urls to monkeypatch it");
                }
            }
        } else if name == "urls" {
            if session.st().get_local_tree(symbol.into()) == (&["werkzeug", "urls"], &[]) {
                //manually load patch, as a manual dependency
                let full_path_monkeypatches = S!("odoo._monkeypatches");
                let mut main_odoo_symbol = None;
                if let Some(main_ep) = session.sync_odoo.entry_point_mgr.borrow().main_entry_point.as_ref() {
                    //To import from main entry point, we have to import 'from' a symbol coming from main entry point.
                    //We then use the main symbol of the main entry point to achieve that, instead of the werkzeug symbol
                    main_odoo_symbol = main_ep.borrow().get_symbol(session.st());
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
