use crate::core::evaluation_context::ContextKey;
use crate::core::evaluation::HookName;
use crate::utils::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use lsp_types::Diagnostic;
use once_cell::sync::Lazy;
use ruff_python_ast::Arguments;
use ruff_python_ast::Expr;
use ruff_python_ast::StmtFunctionDef;
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;
use tracing::warn;
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::evaluation::GetSymbolHook;
use crate::core::odoo::SyncOdoo;
use crate::core::evaluation_context::Context;
use crate::constants::*;
use crate::tree::OYarnExt;
use crate::tree::Tree;
use crate::core::symbols::symbol_keys::FunctionKey;
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::core::symbols::symbol_keys::SymbolKey;
use crate::core::symbols::symbol_keys::Wk;
use crate::core::symbols::storage::SymbolTable;
use crate::oyarn;
use crate::threads::SessionInfo;
use crate::Sy;
use crate::S;
use crate::tree::TreeStrSlice;

use super::entry_point::EntryPoint;
use super::evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationSymbol, EvaluationSymbolWeak};
use super::evaluation_context::ContextValue;
use super::file_mgr::FileMgr;
use super::python_arch_eval::PythonArchEval;

type PythonArchEvalHookFile = fn (odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey);
type Version = (u32, u32); // (major, minor)

fn get_base_model_symbol(odoo: &mut SyncOdoo) -> Option<SymbolKey> {
    let base_model_tree: TreeStrSlice = if odoo.version >= (18, 1) {
        (&["odoo", "orm", "models"], &["BaseModel"])
    } else {
        (&["odoo", "models"], &["BaseModel"])
    };
    let base_model_symbol = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), base_model_tree, u32::MAX);
    base_model_symbol.first().copied()
}
pub struct PythonArchEvalFileHook {
    pub odoo_entry: bool,
    pub trees: Vec<(Version, Version, TreeStrSlice<'static>)>, //if tree content is set, will provide symbol in file content instead of the file symbol to func
    pub if_exist_only: bool,
    pub func: PythonArchEvalHookFile
}

#[allow(non_upper_case_globals)]
static arch_eval_file_hooks: Lazy<Vec<PythonArchEvalFileHook>> = Lazy::new(|| {vec![
    PythonArchEvalFileHook {odoo_entry: true,
                        trees: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "env"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "env"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey| {
        let env_files = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), (&["odoo", "api"], &[]), u32::MAX);
        let Some(env_file) = env_files.last().and_then(|&f| f.as_source_file_key()) else {
            return;
        };
        let env_classes = odoo.symbol_table.get_symbol(env_file.into(), (&[], &["Environment"]), u32::MAX);
        let Some(&env_class) = env_classes.last() else {
            return;
        };
        let env = &mut odoo.symbol_table[symbol.unwrap_variable_key()];
        let context = Context::default();
        env.evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                env_class.into(),
                Some(true),
                context,
                None,
            ),
            value: None,
            range: None,
        }];
        env.doc_string = Some(S!(""));
        odoo.symbol_table.add_dependency(file_symbol, env_file, BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                        trees: vec![((0, 0), (15, 3), (&["odoo", "http"], &["request"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey| {
        // --------- request: WebRequest (before 15.3) ---------
        let web_request_class_syms = odoo.symbol_table.get_symbol(file_symbol.into(), (&[], &["WebRequest"]), u32::MAX);
        let Some(&web_request_class) = web_request_class_syms.last() else {
            return;
        };
        let evaluations = vec![Evaluation::eval_from_symbol(&odoo.symbol_table, web_request_class, Some(true))];
        odoo.symbol_table.set_evaluations(symbol, evaluations);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                        trees: vec![
                            ((15, 3), (19, 2), (&["odoo", "http"], &["request"])),
                            ((19, 2), (999, 0), (&["odoo", "http", "requestlib"], &["request"]))
                        ],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey| {
        // --------- request: Request (15.3+) ---------
        let request_class_syms = odoo.symbol_table.get_symbol(file_symbol.into(), (&[], &["Request"]), u32::MAX);
        let Some(&request_class) = request_class_syms.last() else {
            return;
        };
        let evaluations = vec![Evaluation::eval_from_symbol(&odoo.symbol_table, request_class, Some(true))];
        odoo.symbol_table.set_evaluations(symbol, evaluations);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                        trees: vec![
                            ((0, 0), (15, 3), (&["odoo", "http"], &["WebRequest", "env"])),
                            ((15, 3), (19, 2), (&["odoo", "http"], &["Request", "env"])),
                            ((19, 2), (999, 0), (&["odoo", "http", "requestlib"], &["Request", "env"]))
                        ],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey| {
        // --------- (Web)Request.env: Environment | None ---------
        let env_file_syms = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), (&["odoo", "api"], &[]), u32::MAX);
        let Some(env_file) = env_file_syms.last().and_then(|&f| f.as_source_file_key()) else {
            return;
        };
        let env_class_syms = odoo.symbol_table.get_symbol(env_file.into(), (&[], &["Environment"]), u32::MAX);
        let Some(&env_class) = env_class_syms.last() else {
            return;
        };
        // env is a property (function) before 15.3, and an instance variable in 15.3+.
        // In both cases the evaluation is Environment | None.
        odoo.symbol_table.set_evaluations(symbol, vec![
            Evaluation::eval_from_symbol(&odoo.symbol_table, env_class, Some(true)),
            Evaluation::new_none()
        ]);
        odoo.symbol_table.add_dependency(file_symbol, env_file, BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                        trees: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "ids"])),
                            ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "ids"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        let values: Vec<ruff_python_ast::Expr> = Vec::new();
        let range = odoo.symbol_table.range(symbol).clone();
        let evaluations = vec![Evaluation::new_list(odoo, Some(values), range)];
        odoo.symbol_table.set_evaluations(symbol, evaluations);
    }},
    /*PythonArchEvalFileHook {file_tree: vec![Sy!("odoo"), Sy!("models")],
                        content_tree: vec![Sy!("BaseModel"), Sy!("search_count")],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _file_symbol: SymbolKey, symbol: SymbolKey| {
        let values: Vec<ruff_python_ast::Expr> = Vec::new();
        let mut id = symbol.borrow_mut();
        let range = id.range().clone();
        id.set_evaluations(vec![Evaluation::eval_from_symbol(odoo, values, range.clone())]);
    }},*/
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "api"], &["Environment", "registry"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "environments"], &["Environment", "registry"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        let registry_sym = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), (&["odoo", "modules", "registry"], &["Registry"]), u32::MAX);
        if !registry_sym.is_empty() {
            odoo.symbol_table.set_evaluations(symbol, vec![Evaluation {
                symbol: EvaluationSymbol::new_with_symbol(
                    (*registry_sym.last().unwrap()).into(),
                    Some(true),
                    Context::default(),
                    None
                ),
                value: None,
                range: None,
            }]);
        }
    }},
    /* As __get__ doesn't exists in each class, the validator will not trigger hooks for them at function level, so we put it at file level. */
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Boolean"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_misc"], &["Boolean"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["bool"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Integer"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_numeric"], &["Integer"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["int"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Float"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_numeric"], &["Float"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["float"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Monetary"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_numeric"], &["Monetary"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["float"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Char"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_textual"], &["Char"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["str"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Text"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_textual"], &["Text"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["str"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Html"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_textual"], &["Html"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["markupsafe"], &["Markup"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Date"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_temporal"], &["Date"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["datetime"], &["date"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Datetime"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_temporal"], &["Datetime"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["datetime"], &["datetime"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Binary"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_binary"], &["Binary"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["bytes"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Image"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_binary"], &["Image"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["bytes"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Selection"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_selection"], &["Selection"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["str"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Reference"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_reference"], &["Reference"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["str"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Json"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_misc"], &["Json"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["object"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Properties"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_properties"], &["Properties"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["object"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["PropertiesDefinition"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_properties"], &["PropertiesDefinition"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (&["builtins"], &["object"]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Many2one"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_relational"], &["Many2one"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval_relational(&mut odoo.symbol_table, symbol);
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, Some(oyarn!("Many2one")));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["One2many"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_relational"], &["One2many"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
                PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, Some(oyarn!("One2many")));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Many2many"])),
                                ((18, 1), (999, 0), (&["odoo", "orm", "fields_relational"], &["Many2many"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SourceFileKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval_relational(&mut odoo.symbol_table, symbol);
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, Some(oyarn!("Many2many")));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((18, 1), (999, 0), (&["odoo", "init"], &["_"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey| {
        let odoo_underscore = odoo.get_symbol(odoo.symbol_table.path(file_symbol), (&["odoo"], &["_"]), u32::MAX);
        if let Some(&eval_1) = odoo_underscore.first() {
            odoo.symbol_table.set_evaluations(eval_1, vec![Evaluation::eval_from_symbol(&odoo.symbol_table, symbol, Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((18, 1), (999, 0), (&["odoo", "init"], &["SUPERUSER_ID"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey| {
        let odoo_superuser_id = odoo.get_symbol(odoo.symbol_table.path(file_symbol), (&["odoo"], &["SUPERUSER_ID"]), u32::MAX);
        if let Some(&eval_1) = odoo_superuser_id.first() {
            odoo.symbol_table.set_evaluations(eval_1,vec![Evaluation::eval_from_symbol(&odoo.symbol_table, symbol, Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((18, 1), (999, 0), (&["odoo", "init"], &["_lt"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey| {
        let odoo_lt = odoo.get_symbol(odoo.symbol_table.path(file_symbol), (&["odoo"], &["_lt"]), u32::MAX);
        if let Some(&eval_1) = odoo_lt.first() {
            odoo.symbol_table.set_evaluations(eval_1, vec![Evaluation::eval_from_symbol(&odoo.symbol_table, symbol, Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((18, 1), (999, 0), (&["odoo", "init"], &["Command"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey| {
        let odoo_command = odoo.get_symbol(odoo.symbol_table.path(file_symbol), (&["odoo"], &["Command"]), u32::MAX);
        if let Some(&eval_1) = odoo_command.first() {
            odoo.symbol_table.set_evaluations(eval_1, vec![Evaluation::eval_from_symbol(&odoo.symbol_table, symbol, Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((15, 0), (999, 0), (&["odoo", "addons", "base", "models", "ir_rule"], &["IrRule", "global"]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey| {
        let file_path = odoo.symbol_table.path(file_symbol);
        let boolean_field = if odoo.version >= (18, 1) {
            odoo.get_symbol(file_path, (&["odoo", "orm", "fields_misc"], &["Boolean"]), u32::MAX)
        } else {
            odoo.get_symbol(file_path, (&["odoo", "fields"], &["Boolean"]), u32::MAX)
        };
        if let Some(&boolean) = boolean_field.first() {
            let mut eval = Evaluation::eval_from_symbol(&odoo.symbol_table, boolean, Some(true));
            let weak = eval.symbol.get_mut_symbol_ptr().get_mut_weak();
            weak.context.insert(ContextKey::Compute, ContextValue::STRING(S!("_compute_global")));
            odoo.symbol_table.set_evaluations(symbol, vec![eval]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![((0, 0), (999, 0), (&["odoo", "_monkeypatches", "werkzeug"], &[]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SourceFileKey, symbol: SymbolKey| {
        let file_path = odoo.symbol_table.path(file_symbol).to_string();
        let url_decode = odoo.symbol_table.get_symbol(symbol, (&[], &["url_decode"]), u32::MAX);
        let werkzeug_url_decode = odoo.get_symbol(&file_path, (&["werkzeug", "urls"], &["url_decode"]), u32::MAX);
        if let Some(&werkzeug_url_decode) = werkzeug_url_decode.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_decode { //if not variable, no need to patch it
                if let Some(&eval_1) = url_decode.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_encode = odoo.symbol_table.get_symbol(symbol, (&[], &["url_encode"]), u32::MAX);
        let werkzeug_url_encode = odoo.get_symbol(&file_path, (&["werkzeug", "urls"], &["url_encode"]), u32::MAX);
        if let Some(&werkzeug_url_encode) = werkzeug_url_encode.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_encode { //if not variable, no need to patch it
                if let Some(&eval_1) = url_encode.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_join = odoo.symbol_table.get_symbol(symbol, (&[], &["url_join"]), u32::MAX);
        let werkzeug_url_join = odoo.get_symbol(&file_path, (&["werkzeug", "urls"], &["url_join"]), u32::MAX);
        if let Some(&werkzeug_url_join) = werkzeug_url_join.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_join { //if not variable, no need to patch it
                if let Some(&eval_1) = url_join.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_parse = odoo.symbol_table.get_symbol(symbol, (&[], &["url_parse"]), u32::MAX);
        let werkzeug_url_parse = odoo.get_symbol(&file_path, (&["werkzeug", "urls"], &["url_parse"]), u32::MAX);
        if let Some(&werkzeug_url_parse) = werkzeug_url_parse.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_parse { //if not variable, no need to patch it
                if let Some(&eval_1) = url_parse.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_quote = odoo.symbol_table.get_symbol(symbol, (&[], &["url_quote"]), u32::MAX);
        let werkzeug_url_quote = odoo.get_symbol(&file_path, (&["werkzeug", "urls"], &["url_quote"]), u32::MAX);
        if let Some(&werkzeug_url_quote) = werkzeug_url_quote.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_quote { //if not variable, no need to patch it
                if let Some(&eval_1) = url_quote.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_unquote = odoo.symbol_table.get_symbol(symbol, (&[], &["url_unquote"]), u32::MAX);
        let werkzeug_url_unquote = odoo.get_symbol(&file_path, (&["werkzeug", "urls"], &["url_unquote"]), u32::MAX);
        if let Some(&werkzeug_url_unquote) = werkzeug_url_unquote.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_unquote { //if not variable, no need to patch it
                if let Some(&eval_1) = url_unquote.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_quote_plus = odoo.symbol_table.get_symbol(symbol, (&[], &["url_quote_plus"]), u32::MAX);
        let werkzeug_url_quote_plus = odoo.get_symbol(&file_path, (&["werkzeug", "urls"], &["url_quote_plus"]), u32::MAX);
        if let Some(&werkzeug_url_quote_plus) = werkzeug_url_quote_plus.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_quote_plus { //if not variable, no need to patch it
                if let Some(&eval_1) = url_quote_plus.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_unquote_plus = odoo.symbol_table.get_symbol(symbol, (&[], &["url_unquote_plus"]), u32::MAX);
        let werkzeug_url_unquote_plus = odoo.get_symbol(&file_path, (&["werkzeug", "urls"], &["url_unquote_plus"]), u32::MAX);
        if let Some(&werkzeug_url_unquote_plus) = werkzeug_url_unquote_plus.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_unquote_plus { //if not variable, no need to patch it
                if let Some(&eval_1) = url_unquote_plus.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_unparse = odoo.symbol_table.get_symbol(symbol, (&[], &["url_unparse"]), u32::MAX);
        let werkzeug_url_unparse = odoo.get_symbol(&file_path, (&["werkzeug", "urls"], &["url_unparse"]), u32::MAX);
        if let Some(&werkzeug_url_unparse) = werkzeug_url_unparse.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_unparse { //if not variable, no need to patch it
                if let Some(&eval_1) = url_unparse.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url = odoo.symbol_table.get_symbol(symbol, (&[], &["URL"]), u32::MAX);
        let werkzeug_url_syms = odoo.get_symbol(&file_path, (&["werkzeug", "urls"], &["URL"]), u32::MAX);
        if let Some(&werkzeug_url) = werkzeug_url_syms.first() {
            if let SymbolKey::Variable(v) = werkzeug_url { //if not variable, no need to patch it
                if let Some(&eval_1) = url.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
    }},
]});

type PythonArchEvalHookFunc = fn (odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, function: FunctionKey);

pub struct PythonArchEvalFunctionHook {
    pub odoo_entry: bool,
    pub tree: Vec<(Version, Version, TreeStrSlice<'static>)>, //min_version, max_version, tree
    pub if_exist_only: bool,
    pub func: PythonArchEvalHookFunc
}

#[allow(non_upper_case_globals)]
static arch_eval_function_hooks: Lazy<Vec<PythonArchEvalFunctionHook>> = Lazy::new(|| {vec![
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "api"], &["Environment", "__getitem__"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "environments"], &["Environment", "__getitem__"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
       odoo.symbol_table[symbol].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(Wk::null(),
                Some(true),
                Context::default(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_env_get_item, name: HookName::EvalEnvGetItem})
            ),
            value: None,
            range: None
        }];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "modules", "registry"], &["Registry", "__getitem__"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "registry"], &["Registry", "__getitem__"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        odoo.symbol_table[symbol].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(Wk::null(),
                Some(true),
                Context::default(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_registry_get_item, name: HookName::EvalRegistryGetItem})
            ),
            value: None,
            range: None
        }];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "__iter__"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "__iter__"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "__getitem__"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "__getitem__"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "with_env"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "with_env"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "sudo"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "sudo"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "create"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "create"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "filtered"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "filtered"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "filtered_domain"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "filtered_domain"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "search"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "search"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
        let func = &odoo.symbol_table[symbol];
        if func.args.len() > 1 {
            if let Some(arg_symbol) = func.args.get(1).unwrap().symbol.upgrade(&odoo.symbol_table) {
                if odoo.symbol_table.name(arg_symbol) == "domain" {
                    odoo.symbol_table[arg_symbol].evaluations = vec![Evaluation::new_domain(odoo)];
                } else {
                    warn!("domain not found on search signature")
                }
            }
        }
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "browse"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "browse"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "with_company"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "with_company"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "with_context"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "with_context"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "with_prefetch"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "with_prefetch"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "with_user"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "with_user"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "models"], &["BaseModel", "exists"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "models"], &["BaseModel", "exists"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Id", "__get__"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "fields_misc"], &["Id", "__get__"]))], //We have to put it at function level hook to remove evaluation from existing code
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        PythonArchEvalHooks::_update_get_eval_func_level(odoo, &entry_point, symbol, (&["builtins"], &["int"]));
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["One2many", "__get__"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "fields_relational"], &["One2many", "__get__"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        PythonArchEvalHooks::_update_get_eval_func_relational(&mut odoo.symbol_table, symbol);
    }},
    PythonArchEvalFunctionHook {
                        odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "api"], &["Environment", "ref"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "environments"], &["Environment", "ref"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        PythonArchEvalHooks::validation_env_ref(&mut odoo.symbol_table, symbol);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![((0, 0), (18, 1), (&["odoo", "fields"], &["Field", "__init__"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "fields"], &["Field", "__init__"]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let Some(fields_class_sym) = odoo.symbol_table.get_in_parents(symbol.into(), &[SymType::CLASS], true) else {
            return;
        };
        odoo.symbol_table[symbol].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                fields_class_sym.into(),
                Some(true),
                Context::default(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_init, name: HookName::EvalInit})
            ),
            value: None,
            range: None,
        }];
    }},
]});


type PythonArchEvalHookDecorator = fn (session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments) -> Vec<Diagnostic>;

pub struct PythonArchEvalDecoratorHook {
    pub trees: Vec<(Version, Version, TreeStrSlice<'static>)>, //min_version, max_version, tree
    pub func: PythonArchEvalHookDecorator
}

#[allow(non_upper_case_globals)]
static arch_eval_decorator_hooks: Lazy<Vec<PythonArchEvalDecoratorHook>> = Lazy::new(|| {vec![
    PythonArchEvalDecoratorHook {trees: vec![((0, 0), (18, 1), (&["odoo", "api"], &["returns"]))], //disappear in 18.1
                        func: |session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_returns_decorator(session, func_sym, arguments)
    }},
    PythonArchEvalDecoratorHook {trees: vec![((0, 0), (18, 1), (&["odoo", "api"], &["onchange"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "decorators"], &["onchange"]))],
                        func: |session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_simple_field_decorator(session, func_sym, arguments)
    }},
    PythonArchEvalDecoratorHook {trees: vec![((0, 0), (18, 1), (&["odoo", "api"], &["constrains"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "decorators"], &["constrains"]))],
                        func: |session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_simple_field_decorator(session, func_sym, arguments)
    }},
    PythonArchEvalDecoratorHook {trees: vec![((0, 0), (18, 1), (&["odoo", "api"], &["depends"])),
                        ((18, 1), (999, 0), (&["odoo", "orm", "decorators"], &["depends"]))],
                        func: |session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_nested_field_decorator(session, func_sym, arguments)
    }},
]});
pub struct PythonArchEvalHooks {
}

impl PythonArchEvalHooks {

    pub fn on_file_eval(session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: SourceFileKey) {
        let has_main_entry = session.sync_odoo.has_main_entry;
        let mut lazy_tree: Option<Tree> = None;
        let mut lazy_odoo_tree: Option<Tree> = None;
        let name = session.st().name(symbol).clone();
        for hook in arch_eval_file_hooks.iter() {
            if hook.odoo_entry && !has_main_entry {
                continue;
            }
            for (min_version, max_version, hook_tree) in hook.trees.iter() {
                if !name.eq(hook_tree.0.last().unwrap()) {
                    continue; // skip if file name not matched
                }
                if session.sync_odoo.version < *min_version || session.sync_odoo.version >= *max_version {
                    continue; //skip if version not in range
                }
                let file_tree_matches = if hook.odoo_entry {
                    let odoo_tree = lazy_odoo_tree.get_or_insert_with(|| session.sync_odoo.get_main_entry_tree(symbol));
                    odoo_tree.0 == hook_tree.0
                } else {
                    let tree = lazy_tree.get_or_insert_with(|| session.st().get_tree(symbol));
                    tree.0 == hook_tree.0
                };
                if !file_tree_matches {
                    continue;
                }
                if hook_tree.1.is_empty() {
                    (hook.func)(session.sync_odoo, entry_point, symbol, symbol.into());
                } else {
                    let sub_symbol = session.st().get_symbol(symbol.into(), (&[], hook_tree.1), u32::MAX);
                    if !sub_symbol.is_empty() {
                        (hook.func)(session.sync_odoo, entry_point, symbol, *sub_symbol.last().unwrap());
                    }
                }
            }
        }
    }

    pub fn on_function_eval(session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, function: FunctionKey) {
        let symbol_key: SymbolKey = function.into();
        let has_main_entry = session.sync_odoo.has_main_entry;
        let mut lazy_tree: Option<Tree> = None;
        let mut lazy_odoo_tree: Option<Tree> = None;
        let name = session.st().name(symbol_key).clone();
        for hook in arch_eval_function_hooks.iter() {
            if hook.odoo_entry && !has_main_entry {
                continue;
            }
            for (min_version, max_version, hook_tree) in hook.tree.iter() {
                if !name.eq(hook_tree.1.last().unwrap()) {
                    continue; // skip if function name not matched
                }
                if session.sync_odoo.version < *min_version || session.sync_odoo.version >= *max_version {
                    continue; //skip if version not in range
                }
                let tree = if hook.odoo_entry {
                    lazy_odoo_tree.get_or_insert_with(|| session.sync_odoo.get_main_entry_tree(symbol_key))
                } else {
                    lazy_tree.get_or_insert_with(|| session.st().get_tree(symbol_key))
                };
                if tree == hook_tree {
                    (hook.func)(session.sync_odoo, entry_point, function);
                }
            }
        }
    }

    /// Read function decorators and set evaluations where applicable
    /// - api.returns -> self -> Self, string -> model name if exists + validate
    /// - validates api.depends/onchange/constrains
    pub fn handle_func_decorators(
        session: &mut SessionInfo,
        func_stmt: &StmtFunctionDef,
        func_sym: FunctionKey,
        file: SourceFileKey,
        current_step: BuildSteps,
    ) -> Vec<Diagnostic>{

        let mut diagnostics = vec![];
        for decorator in func_stmt.decorator_list.iter() {
            let (decorator_base, decorator_args) = match &decorator.expression {
                Expr::Call(call_expr) => {
                    (&call_expr.func, &call_expr.arguments)
                },
                _ => {continue;}
            };
            if decorator_args.args.is_empty(){
                continue; // All the decorators we handle have at least one arg for now
            }
            let parent = session.st()[func_sym].parent();
            let mut deps = vec![vec![], vec![], vec![]];
            let (dec_evals, diags) = Evaluation::eval_from_ast(session, &decorator_base, parent, &func_stmt.range.start(), false, &mut deps);
            session.st_mut().insert_dependencies(file, &deps, current_step);
            diagnostics.extend(diags);
            let mut followed_evals = vec![];
            for eval in dec_evals {
                followed_evals.extend(SymbolTable::follow_ref(&eval.symbol.get_symbol(session, None, &mut vec![], None), session, None, true, false, None, None));
            }
            for decorator_eval in followed_evals {
                let EvaluationSymbolPtr::WEAK(decorator_eval_sym_weak) = decorator_eval else {
                    continue;
                };
                let Some(dec_sym) = decorator_eval_sym_weak.weak.upgrade(session.st()) else {
                    continue;
                };
                let dec_sym_tree = session.st().get_tree(dec_sym);
                for hook in arch_eval_decorator_hooks.iter() {
                    for (min_version, max_version, hook_tree) in hook.trees.iter() {
                        if session.sync_odoo.version < *min_version || session.sync_odoo.version >= *max_version {
                            continue; //skip if version not in range
                        }

                        if dec_sym_tree.0.ends_with_strs(hook_tree.0)
                        && dec_sym_tree.1.ends_with_strs(hook_tree.1)
                        && SyncOdoo::is_in_main_entry(session, &dec_sym_tree.0) {
                            diagnostics.extend((hook.func)(session, func_sym, decorator_args));
                        }
                    }
                }
            }
        }
        diagnostics
    }

    pub fn eval_env_get_item(session: &mut SessionInfo, _evaluation_sym: &EvaluationSymbol, context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>
    {
        let res = Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(Wk::null(), Some(true), false)));
        let Some(context) = context else {
            return res
        };
        let in_validation = context.get(ContextKey::IsInValidation).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool();
        let Some(ContextValue::STRING(s)) = context.get(ContextKey::Args) else {
            return res
        };
        let maybe_model = session.sync_odoo.models.get(s.as_str()).cloned();
        let has_class_in_parents = scope.as_ref().map(|&scope| session.st().get_in_parents(scope, &[SymType::CLASS], true).is_some()).unwrap_or(false);
        if maybe_model.as_ref().map(|m| m.borrow_mut().has_symbols(session.st())).unwrap_or(false) {
            let Some(model) = maybe_model else {unreachable!()};
            let module = context.get(ContextKey::Module);
            let from_module = if let Some(ContextValue::MODULE(m)) = module {
                m.upgrade(session.st())
            } else {
                None
            };
            if let Some(scope_file) = scope.and_then(|s| session.st().get_file(s)) {
                //exclude orm files
                if session.sync_odoo.version < (18, 1) {
                    let env_files = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path.as_ref().unwrap(), (&["odoo", "api"], &[]), u32::MAX);
                    let env_file = *env_files.last().unwrap();
                    if env_file != scope_file {
                        session.st_mut().add_model_dependencies(scope_file, &model);
                    }
                } else {
                    let tree = session.sync_odoo.get_main_entry_tree(scope_file);
                    if !tree.0.starts_with_strs(&["odoo", "orm"]) {
                        session.st_mut().add_model_dependencies(scope_file, &model);
                    }
                }
            }
            let model = model.clone();
            let model = model.borrow();
            let symbols = model.get_main_symbols(session, from_module);
            if let Some(&first_symbol) = symbols.first() {
                return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(first_symbol, Some(true), false)));
            }
            if in_validation && has_class_in_parents { //we don't want to show error for functions outside of a model body
                if from_module.is_some() {
                    //retry without from_module to see if model exists elsewhere
                    let symbols = model.get_main_symbols(session, None);
                    if symbols.is_empty() {
                        // Model exists, but has no main symbols
                        if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03005, &[]) { // Is this error code correct?
                            diagnostics.push(Diagnostic {
                                range: FileMgr::textRange_to_temporary_Range(&context.get(ContextKey::Range).unwrap().as_text_range()),
                                ..diagnostic_base.clone()
                            });
                        }
                    } else {
                        // Model exists but not in dependencies
                        let valid_modules: Vec<OYarn> = symbols.iter().map(|&s| match session.st().find_module(s) {
                            Some(sym) => session.st().name(sym).clone(),
                            None => Sy!("Unknown")
                        }).collect();
                        if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03001, &[&format!("{:?}", valid_modules)]) {
                            diagnostics.push(Diagnostic {
                                range: FileMgr::textRange_to_temporary_Range(&context.get(ContextKey::Range).unwrap().as_text_range()),
                                ..diagnostic_base.clone()
                            });
                        }
                    }
                } else {
                    // Model exists, but has no main symbols
                    if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03005, &[]) {
                            diagnostics.push(Diagnostic {
                                range: FileMgr::textRange_to_temporary_Range(&context.get(ContextKey::Range).unwrap().as_text_range()),
                                ..diagnostic_base
                            });
                    }
                }
            }
        } else if in_validation && has_class_in_parents {
            // Model Unknown
            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03002, &[]) {
                diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&context.get(ContextKey::Range).unwrap().as_text_range()),
                    ..diagnostic_base
                });
            }
            let Some(file_symbol) = scope.and_then(|scope| session.st().get_file(scope)) else {
              return res
            };
            let f = file_symbol.unwrap_file_key();
            session.st_mut()[f].not_found_models.insert(Sy!(s.clone()), BuildSteps::VALIDATION);
            session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(file_symbol);
        }
        res
    }

    pub fn eval_registry_get_item(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>
    {
        let mut result = PythonArchEvalHooks::eval_env_get_item(session, evaluation_sym, context, diagnostics, scope);
        match result.as_mut().unwrap() {
            EvaluationSymbolPtr::WEAK(weak) => {
                weak.instance = Some(false);
            },
            _ => {}
        }
        result
    }

    fn eval_get(_session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: Option<&Context>, _diagnostics: &mut Vec<Diagnostic>, _scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>
    {
        if context.is_some() {
            let parent_instance = context.unwrap().get(ContextKey::ParentInstance);
            if parent_instance.is_some() {
                match parent_instance.unwrap() {
                    ContextValue::BOOLEAN(b) => {
                        if !*b {
                            todo!();//TODO
                        }
                    },
                    _ => {}
                }
            }
        }
        Some(EvaluationSymbolPtr::WEAK(evaluation_sym.get_weak().clone()))
    }

    fn _update_get_eval_func_level(odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, function: FunctionKey, tree: TreeStrSlice<'static>) {
        let return_sym = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), tree, u32::MAX);
        let Some(&return_sym) = return_sym.last() else {
            let file = odoo.symbol_table.get_file(function.into()).unwrap();
            odoo.symbol_table.not_found_paths_mut(file).push((BuildSteps::ARCH_EVAL, Tree::from(tree).flatten()));
            entry_point.borrow_mut().not_found_symbols.insert(file);
            return;
        };
        odoo.symbol_table[function].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                return_sym.into(),
                Some(true),
                Context::default(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_get, name: HookName::EvalGet})
            ),
            value: None,
            range: None
        }];
    }

    fn _update_get_eval(odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: SymbolKey, tree: TreeStrSlice<'static>) {
        let get_syms = odoo.symbol_table.get_symbol(symbol, (&[], &["__get__"]), u32::MAX);
        let Some(&get_sym) = get_syms.last() else {
            return;
        };
        let return_syms = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), tree, u32::MAX);
        let Some(&return_sym) = return_syms.last() else {
            let file = odoo.symbol_table.get_file(symbol).unwrap();
            odoo.symbol_table.not_found_paths_mut(file).push((BuildSteps::ARCH_EVAL, Tree::from(tree).flatten()));
            entry_point.borrow_mut().not_found_symbols.insert(file);
            return;
        };
        odoo.symbol_table.set_evaluations(get_sym, vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                return_sym.into(),
                Some(true),
                Context::default(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_get, name: HookName::EvalGet})
            ),
            value: None,
            range: None
        }]);

        let tree: TreeStrSlice = if odoo.version < (18, 1) {
            (&["odoo", "fields"], &["Field", "__get__"])
        } else {
            (&["odoo", "orm", "fields"], &["Field", "__get__"])
        };
        let Some(SymbolKey::Function(field_get)) = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(),  tree, u32::MAX).first().copied()
        else {
            return;
        };
        let field_get_args = odoo.symbol_table[field_get].args.clone();
        odoo.symbol_table[get_sym.unwrap_function_key()].args = field_get_args;
    }

    fn eval_relational_with_related(session: &mut SessionInfo, related_field: &ContextValue, context: &Context) -> Option<EvaluationSymbolPtr>{
        let Some(ContextValue::SYMBOL(class_sym_weak)) = context.get(ContextKey::FieldParent) else {return None};
        let Some(SymbolKey::Class(class_sym)) = class_sym_weak.upgrade(session.st()) else {return None};
        let related_field_name = related_field.as_str();
        let from_module = session.st().find_module(class_sym);
        let syms = PythonArchEval::get_nested_sub_field(session, related_field_name, class_sym, from_module);
        if let Some(&symbol) = syms.first() {
            return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: symbol.into(), context: Context::default(), instance: Some(true), is_super: false}))
        }
        None
    }

    fn eval_relational_with_comodel(session: &mut SessionInfo, comodel: &ContextValue, context: &Context, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>{
        let comodel = comodel.as_str();
        let comodel_sym = session.sync_odoo.models.get(comodel).cloned();
        if let Some(comodel_sym) = comodel_sym {
            // Add dependency
            if let Some(scope) = scope.and_then(|s| session.st().get_file(s)) {
                session.st_mut().add_model_dependencies(scope, &comodel_sym);
            }
            let module = context.get(ContextKey::Module);
            let mut from_module = None;
            if let Some(ContextValue::MODULE(m)) = module {
                if let Some(m) = m.upgrade(session.st()) {
                    from_module = Some(m);
                }
            }
            let main_symbol = comodel_sym.borrow().get_main_symbols(session, from_module);
            if main_symbol.len() == 1 {
                return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: main_symbol[0].into(), context: Context::default(), instance: Some(true), is_super: false}))
            }
        }
        Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: Wk::null(), context: Context::default(), instance: Some(true), is_super: false}))
    }

    fn eval_relational(session: &mut SessionInfo, _evaluation_sym: &EvaluationSymbol, context: Option<&Context>, _diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>
    {
        let Some(context) = context else {
            return None;
        };
        if let Some(comodel) = context.get(ContextKey::ComodelName) {
            return PythonArchEvalHooks::eval_relational_with_comodel(session, comodel, context, scope);
        }
        if let Some(related_field) = context.get(ContextKey::Related) {
            return PythonArchEvalHooks::eval_relational_with_related(session, related_field, context);
        }
        None
    }

    fn _update_get_eval_relational(symbol_table: &mut SymbolTable, symbol: SymbolKey) {
        let get_sym = symbol_table.get_symbol(symbol, (&[], &["__get__"]), u32::MAX);
        if get_sym.is_empty() {
            return;
        }
        symbol_table.set_evaluations(*get_sym.last().unwrap(), vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Wk::null(),
                Some(true),
                Context::default(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_relational, name: HookName::EvalRelational})
            ),
            value: None,
            range: None,
        }]);
    }

    fn _update_get_eval_func_relational(symbol_table: &mut SymbolTable, get_symbol: FunctionKey) {
        symbol_table[get_symbol].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Wk::null(),
                Some(true),
                Context::default(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_relational, name: HookName::EvalRelational})
            ),
            value: None,
            range: None,
        }];
    }

    fn find_special_arguments<'a>(parameters: &'a Arguments, arg_name: &str) -> Option<(&'a Expr, TextRange)> {
        parameters.keywords.iter().find_map(|keyword| {
            keyword.arg
                .as_ref().filter(|kw_arg| kw_arg.id == arg_name)
                .map(|_| (&keyword.value, keyword.range()))
        })
    }

    fn eval_init_common(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: Option<&Context>, _diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<SymbolKey>, relational: bool, one2many: bool) -> Option<EvaluationSymbolPtr>
    {
        let Some(context) = maybe_context else {return None};

        let Some(parameters) = context.get(ContextKey::Parameters).map(|ps| ps.as_arguments()) else {return None};

        let parent = session.st().get_scope_symbol(
            file_symbol.unwrap(),
            context.get(ContextKey::Range).unwrap().as_text_range().start().to_u32(),
            false
        );
        let mut result_context = Context::default();

        let mut contexts_to_add = HashMap::default();
        if relational {
            if let Some(first_param) = parameters.args.get(0) {
                contexts_to_add.insert(ContextKey::ComodelName, (first_param, first_param.range(), "str", ContextKey::ComodelNameArgRange));
            }
            if one2many {
                if let Some(second_param) = parameters.args.get(1) {
                    contexts_to_add.insert(ContextKey::InverseName, (second_param, second_param.range(), "str", ContextKey::InverseNameArgRange));
                }
            }
        }

        // Keyword Arguments for fields that we would like to keep in the context
        let context_arguments = [
            ("comodel_name", "str", ContextKey::ComodelName, ContextKey::ComodelNameArgRange),
            ("related", "str", ContextKey::Related, ContextKey::RelatedArgRange),
            ("compute", "str", ContextKey::Compute, ContextKey::ComputeArgRange),
            ("inverse", "str", ContextKey::Inverse, ContextKey::InverseArgRange),
            ("search", "str", ContextKey::Search, ContextKey::SearchArgRange),
            ("inverse_name", "str", ContextKey::InverseName, ContextKey::InverseNameArgRange),
            ("delegate", "bool", ContextKey::Delegate, ContextKey::EMPTY), // No arg range
            ("required", "bool", ContextKey::Required, ContextKey::EMPTY),
            ("default", "bool", ContextKey::Default, ContextKey::EMPTY),
        ];
        contexts_to_add.extend(
            context_arguments.into_iter()
            .filter_map(|(arg_name_str, only_str, arg_name_key, arg_range_key)|
                PythonArchEvalHooks::find_special_arguments(&parameters, arg_name_str)
                .map(|(field_name_expr, arg_range)| (arg_name_key, (field_name_expr, arg_range, only_str, arg_range_key)))
            )
        );

        for (arg_name, (field_name_expr, arg_range, bool_or_str, arg_range_key)) in contexts_to_add {
            match bool_or_str {
                "str" => if let Some(related_string) = Evaluation::expr_to_str(session, field_name_expr, parent, &parameters.range.start(), false, &mut vec![]).0 {
                    result_context.insert(arg_name, ContextValue::STRING(related_string.to_string()));
                    result_context.insert(arg_range_key, ContextValue::RANGE(arg_range));
                },
                "bool" => {
                    let maybe_boolean = Evaluation::expr_to_bool(session, field_name_expr, parent, &parameters.range.start(), false, &mut vec![]).0;
                    if let Some(boolean) = maybe_boolean {
                        result_context.insert(arg_name, ContextValue::BOOLEAN(boolean));
                    }
                    if arg_name == ContextKey::Default {
                        result_context.insert(ContextKey::Default, ContextValue::BOOLEAN(true)); //set to True as the value is not really useful for now, but we want the key in context if one default is set
                    }
                },
                _ => {}
            }
        }

        result_context.insert(ContextKey::FieldParent, ContextValue::SYMBOL(parent.into()));
        let weak_eval = match context.get(ContextKey::ConstructingClass) {
            Some(ContextValue::SYMBOL(weak)) if !weak.is_expired(session.st()) => *weak,
            _ => evaluation_sym.get_weak().weak,
        };
        return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak {
            weak: weak_eval,
            context: result_context,
            instance: Some(true),
            is_super: false
        }));
    }

    fn eval_init(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<SymbolKey>) -> Option<EvaluationSymbolPtr> {
        return PythonArchEvalHooks::eval_init_common(session, evaluation_sym, maybe_context, diagnostics, file_symbol, false, false)
    }

    fn eval_init_relational(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<SymbolKey>) -> Option<EvaluationSymbolPtr> {
        return PythonArchEvalHooks::eval_init_common(session, evaluation_sym, maybe_context, diagnostics, file_symbol, true, false)
    }

    fn eval_init_relational_one2many(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<SymbolKey>) -> Option<EvaluationSymbolPtr> {
        return PythonArchEvalHooks::eval_init_common(session, evaluation_sym, maybe_context, diagnostics, file_symbol, true, true)
    }

    fn _update_field_init(symbol_table: &mut SymbolTable, symbol: SymbolKey, relational: Option<OYarn>) {
        let init_sym = symbol_table.get_symbol(symbol, (&[], &["__init__"]), u32::MAX);
        if init_sym.is_empty() {
            return;
        }
        symbol_table.set_evaluations(*init_sym.last().unwrap(), vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Wk::from(symbol), //use the weak to keep reference to the class for the hook.
                Some(true),
                Context::default(),
                Some(match relational {
                    Some(oyarn) if oyarn == oyarn!("One2many") => GetSymbolHook{callable: PythonArchEvalHooks::eval_init_relational_one2many, name: HookName::EvalInitRelationalOne2many},
                    Some(_) => GetSymbolHook{callable: PythonArchEvalHooks::eval_init_relational, name: HookName::EvalInitRelational},
                    None => GetSymbolHook{callable: PythonArchEvalHooks::eval_init, name: HookName::EvalInit},
                })
            ),
            value: None,
            range: None,
        }]);
    }

    /// For @api.returns decorator, which can take a string or self
    /// - self: self
    /// - string: model name if exists + validate
    /// Adds evaluation to the function symbol
    /// Returns a vector of diagnostics if the model is not found or not in the dependencies of the module
    fn handle_api_returns_decorator(session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments) -> Vec<Diagnostic>{
        let mut diagnostics = vec![];
        let Some(Expr::StringLiteral(expr)) = arguments.args.first() else {return diagnostics};
        let returns_str = expr.value.to_str();
        if returns_str == "self" {
            if let Some(base) = session.st().get_in_parents(func_sym.into(), &[SymType::CLASS], true) {
                let is_class_method = session.st()[func_sym].is_class_method;
                session.st_mut()[func_sym].evaluations = vec![Evaluation::new_self(base, Some(!is_class_method))];
            }
            return diagnostics;
        }
        let Some(model) = session.sync_odoo.models.get(returns_str).cloned() else {
            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03002, &[]) {
                diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&expr.range()),
                    ..diagnostic_base
                });
            };
            return diagnostics;
        };
        let Some(&main_model_sym) = model.borrow().get_main_symbols(session, session.st().find_module(func_sym)).first() else {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03001, &[]) {
                diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&expr.range()),
                    ..diagnostic
                });
            }
            return diagnostics
        };
        session.st_mut()[func_sym].evaluations = vec![Evaluation::eval_from_symbol(session.st(), main_model_sym, Some(false))];
        diagnostics
    }

    /// For @api.constrains and @api.onchange, both can only take a simple field name
    fn handle_api_simple_field_decorator(session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments) -> Vec<Diagnostic>{
        let mut diagnostics = vec![];
        let from_module = session.st().find_module(func_sym);

        let Some(class_sym) = session.st().get_in_parents(func_sym.into(), &[SymType::CLASS], true) else {
            return diagnostics;
        };

        let class_key = class_sym.unwrap_class_key();
        let Some(model_name) = session.st()[class_key]._model.as_ref().map(|model| &model.name).cloned() else {
            return diagnostics;
        };

        for arg in arguments.args.iter() {
            let Expr::StringLiteral(expr) = arg else {return diagnostics};
            let field_name = expr.value.to_str();
            let (syms, _) = SymbolTable::get_member_symbol(session, class_sym, field_name, from_module, false, true, false, true, false);
            if syms.is_empty() {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03014, &[field_name, &model_name]) {
                    diagnostics.push(Diagnostic {
                        range: FileMgr::textRange_to_temporary_Range(&expr.range()),
                        ..diagnostic
                    });
                }
            }
        }
        diagnostics
    }

    /// For @api.depends, which can take a nested simple field name
    fn handle_api_nested_field_decorator(session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments) -> Vec<Diagnostic>{
        let mut diagnostics = vec![];
        let from_module = session.st().find_module(func_sym);

        let Some(class_sym) = session.st().get_in_parents(func_sym.into(), &[SymType::CLASS], true) else {
            return diagnostics;
        };

        let class_key = class_sym.unwrap_class_key();
        let Some(model_name) = session.st()[class_key]._model.as_ref().map(|model| &model.name).cloned() else {
            return diagnostics;
        };

        for arg in arguments.args.iter() {
            let Expr::StringLiteral(expr) = arg else {return diagnostics};
            let field_name = expr.value.to_str();
            let syms = PythonArchEval::get_nested_sub_field(session, field_name, class_key, from_module);
            if syms.is_empty() {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03014, &[field_name, &model_name]) {
                    diagnostics.push(Diagnostic {
                        range: FileMgr::textRange_to_temporary_Range(&expr.range()),
                        ..diagnostic
                    });
                }
            }
        }
        diagnostics
    }

    fn eval_env_ref(session: &mut SessionInfo, _evaluation_sym: &EvaluationSymbol, context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr> {
        let Some(context) = context else {return None};
        let in_validation = context.get(ContextKey::IsInValidation).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool();
        let Some(parameters) = context.get(ContextKey::Parameters).map(|ps| ps.as_arguments()) else {return None};
        if parameters.args.is_empty() {
            return None; // No arguments to process
        }
        if !parameters.args[0].is_string_literal_expr() {
            return None;
        }
        if parameters.keywords.len() == 1
        // read raise_if_not_found keyword argument
        && !parameters.keywords[0].value.as_boolean_literal_expr().map(|b| b.value).unwrap_or(true) {
            return None; // No need to process if the second argument (raise_if_not_found) is false
        }
        let xml_id_expr = parameters.args[0].as_string_literal_expr().unwrap();
        let xml_id_str = xml_id_expr.value.to_str();
        let mut xml_id_split = xml_id_str.split('.');
        let module_name = xml_id_split.next().unwrap();
        let xml_id = xml_id_split.collect::<Vec<&str>>().join(".");
        if in_validation && xml_id.contains(".") { // invalid xml_id format, should not contain any dots, i.e. module.xml_id
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05051, &[]) {
                diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&xml_id_expr.range()),
                    ..diagnostic
                });
            }
        }
        let module = session.sync_odoo.modules.get(module_name);
        if module.is_none() {
            if in_validation {
                if xml_id.len() == 0 {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05002, &[]) {
                        diagnostics.push(Diagnostic {
                            range: FileMgr::textRange_to_temporary_Range(&xml_id_expr.range()),
                            ..diagnostic
                        });
                    }
                } else {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05003, &[]) {
                        diagnostics.push(Diagnostic {
                            range: FileMgr::textRange_to_temporary_Range(&xml_id_expr.range()),
                            ..diagnostic
                        });
                    }
                }
            }
            return None;
        }
        let module_key = module.unwrap().upgrade(session.st())?;
        if let Some(scope) = scope && let Some(file) = session.st_mut().get_file(scope) {
            if file != module_key.into() {
                session.st_mut().add_dependency(file, module_key.into(), BuildSteps::VALIDATION, BuildSteps::ARCH);
            }
        }
        //TODO => csv xml_id
        //TODO check module dependencies
        //TODO in xml ONLY, ref can omit the 'module.' before the xml_id
        //TODO implement base.model_'nameofmodel' - to test
        return None; //TODO implement returned value
    }

    fn validation_env_ref(symbol_table: &mut SymbolTable, func_sym: FunctionKey) -> Vec<Diagnostic> {
        let diagnostics = vec![];
        symbol_table[func_sym].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                func_sym.into(),
                Some(true),
                Context::default(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_env_ref, name: HookName::EvalEnvRef})
            ),
            value: None,
            range: None
        }];

        diagnostics
    }

}
