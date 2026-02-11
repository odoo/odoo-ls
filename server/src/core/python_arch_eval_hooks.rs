use std::cmp::Ordering;
use std::collections::HashMap;
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
use crate::core::evaluation::Context;
use crate::constants::*;
use crate::core::symbols::symbol_keys::FunctionKey;
use crate::core::symbols::symbol_keys::SymbolKey;
use crate::core::symbols::symbol_keys::Weak;
use crate::core::symbols::symbol_table::SymbolTable;
use crate::oyarn;
use crate::threads::SessionInfo;
use crate::utils::compare_semver;
use crate::Sy;
use crate::S;

use super::entry_point::EntryPoint;
use super::evaluation::{ContextValue, Evaluation, EvaluationSymbolPtr, EvaluationSymbol, EvaluationSymbolWeak};
use super::file_mgr::FileMgr;
use super::python_arch_eval::PythonArchEval;

type PythonArchEvalHookFile = fn (odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey);

fn get_base_model_symbol(odoo: &mut SyncOdoo) -> Option<SymbolKey> {
    let base_model_tree = if compare_semver(odoo.full_version.as_str(), "18.1") >= Ordering::Equal {
        (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel")])
    } else {
        (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel")])
    };
    let base_model_symbol = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &base_model_tree, u32::MAX);
    base_model_symbol.first().copied()
}
pub struct PythonArchEvalFileHook {
    pub odoo_entry: bool,
    pub trees: Vec<(OYarn, OYarn, (Vec<OYarn>, Vec<OYarn>))>, //if tree content is set, will provide symbol in file content instead of the file symbol to func
    pub if_exist_only: bool,
    pub func: PythonArchEvalHookFile
}

#[allow(non_upper_case_globals)]
static arch_eval_file_hooks: Lazy<Vec<PythonArchEvalFileHook>> = Lazy::new(|| {vec![
    PythonArchEvalFileHook {odoo_entry: true,
                        trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("env")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("env")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey| {
        let env_file = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("api")], vec![]), u32::MAX);
        let env_class = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment")]), u32::MAX);
        if !env_class.is_empty() {
            let env = &mut odoo.symbol_table[symbol.unwrap_variable_key()];
            let env_class = *env_class.last().unwrap();
            let context = HashMap::new();
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
            odoo.symbol_table.add_dependency(file_symbol, *env_file.last().unwrap(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                        trees: vec![(Sy!("0.0"), Sy!("15.3"), (vec![Sy!("odoo"), Sy!("http")], vec![Sy!("request")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey| {
        // --------- request: WebRequest (before 15.3) ---------
        let web_request_class_syms = odoo.symbol_table.get_symbol(file_symbol, &(vec![], vec![Sy!("WebRequest")]), u32::MAX);
        let Some(&web_request_class) = web_request_class_syms.last() else {
            return;
        };
        let evaluations = vec![Evaluation::eval_from_symbol(&odoo.symbol_table, web_request_class, Some(true))];
        odoo.symbol_table.set_evaluations(symbol, evaluations);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                        trees: vec![
                            (Sy!("15.3"), Sy!("19.2"), (vec![Sy!("odoo"), Sy!("http")], vec![Sy!("request")])),
                            (Sy!("19.2"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("http"), Sy!("requestlib")], vec![Sy!("request")]))
                        ],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey| {
        // --------- request: Request (15.3+) ---------
        let request_class_syms = odoo.symbol_table.get_symbol(file_symbol, &(vec![], vec![Sy!("Request")]), u32::MAX);
        let Some(&request_class) = request_class_syms.last() else {
            return;
        };
        let evaluations = vec![Evaluation::eval_from_symbol(&odoo.symbol_table, request_class, Some(true))];
        odoo.symbol_table.set_evaluations(symbol, evaluations);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                        trees: vec![
                            (Sy!("0.0"), Sy!("15.3"), (vec![Sy!("odoo"), Sy!("http")], vec![Sy!("WebRequest"), Sy!("env")])),
                            (Sy!("15.3"), Sy!("19.2"), (vec![Sy!("odoo"), Sy!("http")], vec![Sy!("Request"), Sy!("env")])),
                            (Sy!("19.2"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("http"), Sy!("requestlib")], vec![Sy!("Request"), Sy!("env")]))
                        ],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey| {
        // --------- (Web)Request.env: Environment | None ---------
        let env_file_syms = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("api")], vec![]), u32::MAX);
        let Some(&env_file) = env_file_syms.last() else {
            return;
        };
        let env_class_syms = odoo.symbol_table.get_symbol(env_file, &(vec![], vec![Sy!("Environment")]), u32::MAX);
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
                        trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("ids")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("ids")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
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
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment"), Sy!("registry")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("environments")], vec![Sy!("Environment"), Sy!("registry")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        let registry_sym = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("modules"), Sy!("registry")], vec![Sy!("Registry")]), u32::MAX);
        if !registry_sym.is_empty() {
            odoo.symbol_table.set_evaluations(symbol, vec![Evaluation {
                symbol: EvaluationSymbol::new_with_symbol(
                    (*registry_sym.last().unwrap()).into(),
                    Some(true),
                    HashMap::new(),
                    None
                ),
                value: None,
                range: None,
            }]);
        }
    }},
    /* As __get__ doesn't exists in each class, the validator will not trigger hooks for them at function level, so we put it at file level. */
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Boolean")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_misc")], vec![Sy!("Boolean")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("bool")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Integer")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_numeric")], vec![Sy!("Integer")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("int")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Float")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_numeric")], vec![Sy!("Float")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("float")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Monetary")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_numeric")], vec![Sy!("Monetary")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("float")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Char")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_textual")], vec![Sy!("Char")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Text")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_textual")], vec![Sy!("Text")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Html")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_textual")], vec![Sy!("Html")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("markupsafe")], vec![Sy!("Markup")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Date")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_temporal")], vec![Sy!("Date")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("datetime")], vec![Sy!("date")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Datetime")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_temporal")], vec![Sy!("Datetime")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("datetime")], vec![Sy!("datetime")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Binary")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_binary")], vec![Sy!("Binary")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("bytes")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Image")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_binary")], vec![Sy!("Image")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("bytes")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Selection")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_selection")], vec![Sy!("Selection")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Reference")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_reference")], vec![Sy!("Reference")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Json")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_misc")], vec![Sy!("Json")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("object")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Properties")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_properties")], vec![Sy!("Properties")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("object")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("PropertiesDefinition")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_properties")], vec![Sy!("PropertiesDefinition")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol, (vec![Sy!("builtins")], vec![Sy!("object")]));
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, None);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2one")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("Many2one")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval_relational(&mut odoo.symbol_table, symbol);
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, Some(oyarn!("Many2one")));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("One2many")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("One2many")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
                PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, Some(oyarn!("One2many")));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2many")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("Many2many")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: SymbolKey, symbol: SymbolKey| {
        PythonArchEvalHooks::_update_get_eval_relational(&mut odoo.symbol_table, symbol);
        PythonArchEvalHooks::_update_field_init(&mut odoo.symbol_table, symbol, Some(oyarn!("Many2many")));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("init")], vec![Sy!("_")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey| {
        let odoo_underscore = odoo.get_symbol(odoo.symbol_table.file_path(file_symbol), &(vec![Sy!("odoo")], vec![Sy!("_")]), u32::MAX);
        if let Some(&eval_1) = odoo_underscore.first() {
            odoo.symbol_table.set_evaluations(eval_1, vec![Evaluation::eval_from_symbol(&odoo.symbol_table, symbol, Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("init")], vec![Sy!("SUPERUSER_ID")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey| {
        let odoo_superuser_id = odoo.get_symbol(odoo.symbol_table.file_path(file_symbol), &(vec![Sy!("odoo")], vec![Sy!("SUPERUSER_ID")]), u32::MAX);
        if let Some(&eval_1) = odoo_superuser_id.first() {
            odoo.symbol_table.set_evaluations(eval_1,vec![Evaluation::eval_from_symbol(&odoo.symbol_table, symbol, Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("init")], vec![Sy!("_lt")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey| {
        let odoo_lt = odoo.get_symbol(odoo.symbol_table.file_path(file_symbol), &(vec![Sy!("odoo")], vec![Sy!("_lt")]), u32::MAX);
        if let Some(&eval_1) = odoo_lt.first() {
            odoo.symbol_table.set_evaluations(eval_1, vec![Evaluation::eval_from_symbol(&odoo.symbol_table, symbol, Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("init")], vec![Sy!("Command")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey| {
        let odoo_command = odoo.get_symbol(odoo.symbol_table.file_path(file_symbol), &(vec![Sy!("odoo")], vec![Sy!("Command")]), u32::MAX);
        if let Some(&eval_1) = odoo_command.first() {
            odoo.symbol_table.set_evaluations(eval_1, vec![Evaluation::eval_from_symbol(&odoo.symbol_table, symbol, Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("15.0"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("addons"), Sy!("base"), Sy!("models"), Sy!("ir_rule")], vec![Sy!("IrRule"), Sy!("global")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey| {
        let file_path = odoo.symbol_table.file_path(file_symbol);
        let boolean_field = if compare_semver(odoo.full_version.as_str(), "18.1") >= Ordering::Equal {
            odoo.get_symbol(file_path, &(vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_misc")], vec![Sy!("Boolean")]), u32::MAX)
        } else {
            odoo.get_symbol(file_path, &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Boolean")]), u32::MAX)
        };
        if let Some(&boolean) = boolean_field.first() {
            let mut eval = Evaluation::eval_from_symbol(&odoo.symbol_table, boolean, Some(true));
            let weak = eval.symbol.get_mut_symbol_ptr().get_mut_weak();
            weak.context.insert(S!("compute"), ContextValue::STRING(S!("_compute_global")));
            odoo.symbol_table.set_evaluations(symbol, vec![eval]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("_monkeypatches"), Sy!("werkzeug")], vec![]))],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: SymbolKey, symbol: SymbolKey| {
        let file_path = odoo.symbol_table.file_path(file_symbol).to_string();
        let url_decode = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("url_decode")]), u32::MAX);
        let werkzeug_url_decode = odoo.get_symbol(&file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_decode")]), u32::MAX);
        if let Some(&werkzeug_url_decode) = werkzeug_url_decode.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_decode { //if not variable, no need to patch it
                if let Some(&eval_1) = url_decode.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_encode = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("url_encode")]), u32::MAX);
        let werkzeug_url_encode = odoo.get_symbol(&file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_encode")]), u32::MAX);
        if let Some(&werkzeug_url_encode) = werkzeug_url_encode.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_encode { //if not variable, no need to patch it
                if let Some(&eval_1) = url_encode.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_join = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("url_join")]), u32::MAX);
        let werkzeug_url_join = odoo.get_symbol(&file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_join")]), u32::MAX);
        if let Some(&werkzeug_url_join) = werkzeug_url_join.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_join { //if not variable, no need to patch it
                if let Some(&eval_1) = url_join.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_parse = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("url_parse")]), u32::MAX);
        let werkzeug_url_parse = odoo.get_symbol(&file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_parse")]), u32::MAX);
        if let Some(&werkzeug_url_parse) = werkzeug_url_parse.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_parse { //if not variable, no need to patch it
                if let Some(&eval_1) = url_parse.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_quote = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("url_quote")]), u32::MAX);
        let werkzeug_url_quote = odoo.get_symbol(&file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_quote")]), u32::MAX);
        if let Some(&werkzeug_url_quote) = werkzeug_url_quote.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_quote { //if not variable, no need to patch it
                if let Some(&eval_1) = url_quote.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_unquote = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("url_unquote")]), u32::MAX);
        let werkzeug_url_unquote = odoo.get_symbol(&file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_unquote")]), u32::MAX);
        if let Some(&werkzeug_url_unquote) = werkzeug_url_unquote.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_unquote { //if not variable, no need to patch it
                if let Some(&eval_1) = url_unquote.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_quote_plus = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("url_quote_plus")]), u32::MAX);
        let werkzeug_url_quote_plus = odoo.get_symbol(&file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_quote_plus")]), u32::MAX);
        if let Some(&werkzeug_url_quote_plus) = werkzeug_url_quote_plus.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_quote_plus { //if not variable, no need to patch it
                if let Some(&eval_1) = url_quote_plus.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_unquote_plus = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("url_unquote_plus")]), u32::MAX);
        let werkzeug_url_unquote_plus = odoo.get_symbol(&file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_unquote_plus")]), u32::MAX);
        if let Some(&werkzeug_url_unquote_plus) = werkzeug_url_unquote_plus.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_unquote_plus { //if not variable, no need to patch it
                if let Some(&eval_1) = url_unquote_plus.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url_unparse = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("url_unparse")]), u32::MAX);
        let werkzeug_url_unparse = odoo.get_symbol(&file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_unparse")]), u32::MAX);
        if let Some(&werkzeug_url_unparse) = werkzeug_url_unparse.first() {
            if let SymbolKey::Variable(v) = werkzeug_url_unparse { //if not variable, no need to patch it
                if let Some(&eval_1) = url_unparse.first() {
                    odoo.symbol_table[v].evaluations = vec![
                        Evaluation::eval_from_symbol(&odoo.symbol_table, eval_1, Some(false))
                    ];
                }
            }
        }
        let url = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("URL")]), u32::MAX);
        let werkzeug_url_syms = odoo.get_symbol(&file_path, &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("URL")]), u32::MAX);
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
    pub tree: Vec<(OYarn, OYarn, Tree)>, //min_version, max_version, tree
    pub if_exist_only: bool,
    pub func: PythonArchEvalHookFunc
}

#[allow(non_upper_case_globals)]
static arch_eval_function_hooks: Lazy<Vec<PythonArchEvalFunctionHook>> = Lazy::new(|| {vec![
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment"), Sy!("__getitem__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("environments")], vec![Sy!("Environment"), Sy!("__getitem__")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
       odoo.symbol_table[symbol].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(Weak::null(),
                Some(true),
                HashMap::new(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_env_get_item, name: S!("eval_env_get_item")})
            ),
            value: None,
            range: None
        }];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("modules"), Sy!("registry")], vec![Sy!("Registry"), Sy!("__getitem__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("registry")], vec![Sy!("Registry"), Sy!("__getitem__")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        odoo.symbol_table[symbol].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(Weak::null(),
                Some(true),
                HashMap::new(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_registry_get_item, name: S!("eval_registry_get_item")})
            ),
            value: None,
            range: None
        }];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("__iter__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("__iter__")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("__getitem__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("__getitem__")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_env")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_env")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("sudo")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("sudo")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("create")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("create")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("filtered")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("filtered")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("filtered_domain")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("filtered_domain")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("search")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("search")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
        let func = &odoo.symbol_table[symbol];
        if func.args.len() > 1 {
            // @arena: skipped an upgrade here (args.symbol used to be weak, now it can be trusted as strong?)
            let arg_symbol = func.args.get(1).unwrap().symbol;
            if odoo.symbol_table.name(arg_symbol) == "domain" {
                let evaluations = vec![Evaluation::new_domain(odoo)];
                odoo.symbol_table.set_evaluations(arg_symbol, evaluations);
            } else {
                warn!("domain not found on search signature")
            }
        }
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("browse")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("browse")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_company")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_company")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_context")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_context")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_prefetch")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_prefetch")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_user")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_user")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("exists")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("exists")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let base_model = get_base_model_symbol(odoo);
        odoo.symbol_table[symbol].evaluations = vec![Evaluation::new_self(base_model.unwrap(), Some(true))];
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Id"), Sy!("__get__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_misc")], vec![Sy!("Id"), Sy!("__get__")]))], //We have to put it at function level hook to remove evaluation from existing code
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        PythonArchEvalHooks::_update_get_eval_func_level(odoo, &entry_point, symbol, (vec![Sy!("builtins")], vec![Sy!("int")]));
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("One2many"), Sy!("__get__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("One2many"), Sy!("__get__")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        PythonArchEvalHooks::_update_get_eval_func_relational(&mut odoo.symbol_table, symbol);
    }},
    PythonArchEvalFunctionHook {
                        odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment"), Sy!("ref")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("environments")], vec![Sy!("Environment"), Sy!("ref")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        PythonArchEvalHooks::validation_env_ref(&mut odoo.symbol_table, symbol);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Field"), Sy!("__init__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields")], vec![Sy!("Field"), Sy!("__init__")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: FunctionKey| {
        let Some(fields_class_sym) = odoo.symbol_table.get_in_parents(symbol.into(), &[SymType::CLASS], true) else {
            return;
        };
        odoo.symbol_table[symbol].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                fields_class_sym.into(),
                Some(true),
                HashMap::new(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_init, name: S!("eval_init")})
            ),
            value: None,
            range: None,
        }];
    }},
]});


type PythonArchEvalHookDecorator = fn (session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments) -> Vec<Diagnostic>;

pub struct PythonArchEvalDecoratorHook {
    pub trees: Vec<(OYarn, OYarn, Tree)>, //min_version, max_version, tree
    pub func: PythonArchEvalHookDecorator
}

#[allow(non_upper_case_globals)]
static arch_eval_decorator_hooks: Lazy<Vec<PythonArchEvalDecoratorHook>> = Lazy::new(|| {vec![
    PythonArchEvalDecoratorHook {trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("returns")]))], //disappear in 18.1
                        func: |session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_returns_decorator(session, func_sym, arguments)
    }},
    PythonArchEvalDecoratorHook {trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("onchange")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("decorators")], vec![Sy!("onchange")]))],
                        func: |session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_simple_field_decorator(session, func_sym, arguments)
    }},
    PythonArchEvalDecoratorHook {trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("constrains")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("decorators")], vec![Sy!("constrains")]))],
                        func: |session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_simple_field_decorator(session, func_sym, arguments)
    }},
    PythonArchEvalDecoratorHook {trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("depends")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("decorators")], vec![Sy!("depends")]))],
                        func: |session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_nested_field_decorator(session, func_sym, arguments)
    }},
]});
pub struct PythonArchEvalHooks {
}

impl PythonArchEvalHooks {

    pub fn on_file_eval(session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: SymbolKey) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let tree = st!().get_tree(symbol);
        let odoo_tree = SymbolTable::get_main_entry_tree(session, symbol);
        let name = st!().name(symbol).clone();
        for hook in arch_eval_file_hooks.iter() {
            for (min_version, max_version, hook_tree) in hook.trees.iter() {
                if compare_semver(min_version, &session.sync_odoo.full_version) == Ordering::Greater ||
                    compare_semver(max_version, &session.sync_odoo.full_version) <= Ordering::Equal {
                    continue; //skip if version not in range
                }
                if name.eq(hook_tree.0.last().unwrap()) &&
                ((hook.odoo_entry && session.sync_odoo.has_main_entry && odoo_tree.0 == hook_tree.0) || (!hook.odoo_entry && tree.0 == hook_tree.0)) {
                    if hook_tree.1.is_empty() {
                        (hook.func)(session.sync_odoo, entry_point, symbol, symbol);
                    } else {
                        let sub_symbol = st!().get_symbol(symbol, &(vec![], hook_tree.1.clone()), u32::MAX);
                        if !sub_symbol.is_empty() {
                            (hook.func)(session.sync_odoo, entry_point, symbol, *sub_symbol.last().unwrap());
                        }
                    }
                }
            }
        }
    }

    pub fn on_function_eval(session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, function: FunctionKey) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let symbol_key: SymbolKey = function.into();
        let tree = st!().get_tree(symbol_key);
        let odoo_tree = SymbolTable::get_main_entry_tree(session, symbol_key);
        let name = st!().name(symbol_key).clone();
        for hook in arch_eval_function_hooks.iter() {
            for hook_tree in hook.tree.iter() {
                if compare_semver(hook_tree.0.as_str(), session.sync_odoo.full_version.as_str()) == Ordering::Greater ||
                    compare_semver(hook_tree.1.as_str(), session.sync_odoo.full_version.as_str()) <= Ordering::Equal {
                    continue; //skip if version not in range
                }
                if name.eq(hook_tree.2.1.last().unwrap()) {
                    if (hook.odoo_entry && session.sync_odoo.has_main_entry && odoo_tree == hook_tree.2) || (!hook.odoo_entry && tree == hook_tree.2) {
                        (hook.func)(session.sync_odoo, entry_point, function);
                    }
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
        file: SymbolKey,
        current_step: BuildSteps,
    ) -> Vec<Diagnostic>{
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
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
            let parent = st!()[func_sym].parent();
            let mut deps = vec![vec![], vec![], vec![]];
            let (dec_evals, diags) = Evaluation::eval_from_ast(session, &decorator_base, parent, &func_stmt.range.start(), false, &mut deps);
            st!().insert_dependencies(file, &mut deps, current_step);
            diagnostics.extend(diags);
            let mut followed_evals = vec![];
            for eval in dec_evals {
                followed_evals.extend(SymbolTable::follow_ref(&eval.symbol.get_symbol(session, &mut None, &mut vec![], None), session, &mut None, true, false, None, None));
            }
            for decorator_eval in followed_evals {
                let EvaluationSymbolPtr::WEAK(decorator_eval_sym_weak) = decorator_eval else {
                    continue;
                };
                let Some(dec_sym) = decorator_eval_sym_weak.weak.upgrade(&st!()) else {
                    continue;
                };
                let dec_sym_tree = st!().get_tree(dec_sym);
                for hook in arch_eval_decorator_hooks.iter() {
                    for (min_version, max_version, hook_tree) in hook.trees.iter() {
                        if compare_semver(min_version, &session.sync_odoo.full_version) == Ordering::Greater ||
                            compare_semver(max_version,  &session.sync_odoo.full_version.as_str()) <= Ordering::Equal {
                            continue; //skip if version not in range
                        }
                        if !dec_sym_tree.0.ends_with(&hook_tree.0) || !dec_sym_tree.1.ends_with(&hook_tree.1) || !SyncOdoo::is_in_main_entry(session, &dec_sym_tree.0) {
                            continue;
                        }
                        diagnostics.extend((hook.func)(session, func_sym, decorator_args));
                    }
                }
            }
        }
        diagnostics
    }

    pub fn eval_env_get_item(session: &mut SessionInfo, _evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>
    {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let res = Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(Weak::null(), Some(true), false)));
        let Some(context) = context else {
            return res
        };
        let in_validation = context.get(&S!("is_in_validation")).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool();
        let Some(ContextValue::STRING(s)) = context.get(&S!("args")) else {
            return res
        };
        let maybe_model = session.sync_odoo.models.get(&oyarn!("{}", s)).cloned();
        let has_class_in_parents = scope.as_ref().map(|&scope| st!().get_in_parents(scope, &[SymType::CLASS], true).is_some()).unwrap_or(false);
        if maybe_model.as_ref().map(|m| m.borrow_mut().has_symbols(&st!())).unwrap_or(false) {
            let Some(model) = maybe_model else {unreachable!()};
            let module = context.get(&S!("module"));
            let from_module = if let Some(ContextValue::MODULE(m)) = module {
                m.upgrade(&st!())
            } else {
                None
            };
            if let Some(scope_file) = scope.and_then(|s| st!().get_file(s)) {
                //exclude orm files
                if compare_semver(session.sync_odoo.full_version.as_str(), "18.1") < Ordering::Equal {
                    let env_files = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("api")], vec![]), u32::MAX);
                    let env_file = *env_files.last().unwrap();
                    if env_file != scope_file {
                        st!().add_model_dependencies(scope_file, &model);
                    }
                } else {
                    let tree = SymbolTable::get_main_entry_tree(session, scope_file);
                    if !tree.0.starts_with(&[Sy!("odoo"), Sy!("orm")]) {
                        st!().add_model_dependencies(scope_file, &model);
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
                                range: FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range()),
                                ..diagnostic_base.clone()
                            });
                        }
                    } else {
                        // Model exists but not in dependencies
                        let valid_modules: Vec<OYarn> = symbols.iter().map(|&s| match st!().find_module(s) {
                            Some(sym) => st!().name(sym).clone(),
                            None => Sy!("Unknown")
                        }).collect();
                        if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03001, &[&format!("{:?}", valid_modules)]) {
                            diagnostics.push(Diagnostic {
                                range: FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range()),
                                ..diagnostic_base.clone()
                            });
                        }
                    }
                } else {
                    // Model exists, but has no main symbols
                    if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03005, &[]) {
                            diagnostics.push(Diagnostic {
                                range: FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range()),
                                ..diagnostic_base
                            });
                    }
                }
            }
        } else if in_validation && has_class_in_parents {
            // Model Unknown
            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03002, &[]) {
                diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range()),
                    ..diagnostic_base
                });
            }
            let Some(file_symbol) = scope.and_then(|scope| st!().get_file(scope)) else {
              return res
            };
            let f = file_symbol.unwrap_file_key();
            st!()[f].not_found_models.insert(Sy!(s.clone()), BuildSteps::VALIDATION);
            session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(file_symbol);
        }
        res
    }

    pub fn eval_registry_get_item(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>
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

    fn eval_get(_session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, _diagnostics: &mut Vec<Diagnostic>, _scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>
    {
        if context.is_some() {
            let parent_instance = context.as_ref().unwrap().get(&S!("parent_instance"));
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

    fn _update_get_eval_func_level(odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, function: FunctionKey, tree: Tree) {
        let return_sym = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &tree, u32::MAX);
        let Some(&return_sym) = return_sym.last() else {
            let file = odoo.symbol_table.get_file(function.into());
            odoo.symbol_table.not_found_paths_mut(file.unwrap()).push((BuildSteps::ARCH_EVAL, flatten_tree(&tree)));
            entry_point.borrow_mut().not_found_symbols.insert(odoo.symbol_table[function].parent());
            return;
        };
        odoo.symbol_table[function].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                return_sym.into(),
                Some(true),
                HashMap::new(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_get, name: S!("eval_get")})
            ),
            value: None,
            range: None
        }];
    }

    fn _update_get_eval(odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: SymbolKey, tree: Tree) {
        let get_syms = odoo.symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("__get__")]), u32::MAX);
        let Some(&get_sym) = get_syms.last() else {
            return;
        };
        let return_syms = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &tree, u32::MAX);
        let Some(&return_sym) = return_syms.last() else {
            let file = odoo.symbol_table.get_file(symbol);
            odoo.symbol_table.not_found_paths_mut(file.unwrap()).push((BuildSteps::ARCH_EVAL, flatten_tree(&tree)));
            entry_point.borrow_mut().not_found_symbols.insert(symbol);
            return;
        };
        odoo.symbol_table.set_evaluations(get_sym, vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                return_sym.into(),
                Some(true),
                HashMap::new(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_get, name: S!("eval_get")})
            ),
            value: None,
            range: None
        }]);

        let tree = if compare_semver(odoo.full_version.as_str(), "18.1.0") == Ordering::Less {
            (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Field"), Sy!("__get__")])
        } else {
            (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields")], vec![Sy!("Field"), Sy!("__get__")])
        };
        let Some(field_get) = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(),  &tree, u32::MAX).first().copied()
        else {
            return;
        };
        // @arena: when these keys are obtained above, the matching on Some could include the function key type (e.g. Some(SymbolKey::Function(get_sym)))
        let field_get_args = odoo.symbol_table[field_get.unwrap_function_key()].args.clone();
        odoo.symbol_table[get_sym.unwrap_function_key()].args = field_get_args;
    }

    // @arena todo: double check if class_sym can safely be unwrapped as ClassKey
    fn eval_relational_with_related(session: &mut SessionInfo, related_field: &ContextValue, context: &Context) -> Option<EvaluationSymbolPtr>{
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let Some(ContextValue::SYMBOL(class_sym_weak)) = context.get(&S!("field_parent")) else {return None};
        let Some(class_sym) = class_sym_weak.upgrade(&st!()) else {return None};
        let related_field_name = related_field.as_string();
        let from_module = st!().find_module(class_sym);
        let syms = PythonArchEval::get_nested_sub_field(session, &related_field_name, class_sym.unwrap_class_key(), from_module);
        if let Some(&symbol) = syms.first() {
            return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: symbol.into(), context: HashMap::new(), instance: Some(true), is_super: false}))
        }
        None
    }

    fn eval_relational_with_comodel(session: &mut SessionInfo, comodel: &ContextValue, context: &Context, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>{
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let comodel = oyarn!("{}", comodel.as_string());
        let comodel_sym = session.sync_odoo.models.get(&comodel).cloned();
        if let Some(comodel_sym) = comodel_sym {
            // Add dependency
            if let Some(scope) = scope.and_then(|s| st!().get_file(s)) {
                st!().add_model_dependencies(scope, &comodel_sym);
            }
            let module = context.get(&S!("module"));
            let mut from_module = None;
            if let Some(ContextValue::MODULE(m)) = module {
                if let Some(m) = m.upgrade(&st!()) {
                    from_module = Some(m);
                }
            }
            let main_symbol = comodel_sym.borrow().get_main_symbols(session, from_module);
            if main_symbol.len() == 1 {
                return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: main_symbol[0].into(), context: HashMap::new(), instance: Some(true), is_super: false}))
            }
        }
        Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: Weak::null(), context: HashMap::new(), instance: Some(true), is_super: false}))
    }

    fn eval_relational(session: &mut SessionInfo, _evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, _diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>
    {
        let Some(context) = context else {
            return None;
        };
        if let Some(comodel) = context.get(&S!("comodel_name")) {
            return PythonArchEvalHooks::eval_relational_with_comodel(session, comodel, context, scope);
        }
        if let Some(related_field) = context.get(&S!("related")) {
            return PythonArchEvalHooks::eval_relational_with_related(session, related_field, context);
        }
        None
    }

    fn _update_get_eval_relational(symbol_table: &mut SymbolTable, symbol: SymbolKey) {
        let get_sym = symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("__get__")]), u32::MAX);
        if get_sym.is_empty() {
            return;
        }
        symbol_table.set_evaluations(*get_sym.last().unwrap(), vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Weak::null(),
                Some(true),
                HashMap::new(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_relational, name: S!("eval_relational")})
            ),
            value: None,
            range: None,
        }]);
    }

    fn _update_get_eval_func_relational(symbol_table: &mut SymbolTable, get_symbol: FunctionKey) {
        symbol_table[get_symbol].evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Weak::null(),
                Some(true),
                HashMap::new(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_relational, name: S!("eval_relational")})
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

    fn eval_init_common(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: &mut Option<Context>, _diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<SymbolKey>, relational: bool, one2many: bool) -> Option<EvaluationSymbolPtr>
    {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let Some(context) = maybe_context else {return None};

        let Some(parameters) = context.get(&S!("parameters")).map(|ps| ps.as_arguments()) else {return None};

        let parent = st!().get_scope_symbol(
            file_symbol.unwrap(),
            context.get(&S!("range")).unwrap().as_text_range().start().to_u32(),
            false
        );
        let mut result_context = HashMap::new();

        let mut contexts_to_add = HashMap::new();
        if relational {
            if let Some(first_param) = parameters.args.get(0) {
                contexts_to_add.insert("comodel_name", (first_param, first_param.range(), "str"));
            }
            if one2many {
                if let Some(second_param) = parameters.args.get(1) {
                    contexts_to_add.insert("inverse_name", (second_param, second_param.range(), "str"));
                }
            }
        }

        // Keyword Arguments for fields that we would like to keep in the context
        let context_arguments = [
            ("comodel_name", "str"),
            ("related", "str"),
            ("compute", "str"),
            ("inverse", "str"),
            ("search", "str"),
            ("inverse_name", "str"),
            ("delegate", "bool"),
            ("required", "bool"),
            ("default", "bool"),
        ];
        contexts_to_add.extend(
            context_arguments.into_iter()
            .filter_map(|(arg_name, only_str)|
                PythonArchEvalHooks::find_special_arguments(&parameters, arg_name)
                .map(|(field_name_expr, arg_range)| (arg_name, (field_name_expr, arg_range, only_str)))
            )
        );

        for (arg_name, (field_name_expr, arg_range, bool_or_str)) in contexts_to_add {
            match bool_or_str {
                "str" => if let Some(related_string) = Evaluation::expr_to_str(session, field_name_expr, parent, &parameters.range.start(), false, &mut vec![]).0 {
                    result_context.insert(S!(arg_name), ContextValue::STRING(related_string.to_string()));
                    result_context.insert(format!("{arg_name}_arg_range"), ContextValue::RANGE(arg_range.clone()));
                },
                "bool" => {
                    let maybe_boolean = Evaluation::expr_to_bool(session, field_name_expr, parent, &parameters.range.start(), false, &mut vec![]).0;
                    if let Some(boolean) = maybe_boolean {
                        result_context.insert(S!(arg_name), ContextValue::BOOLEAN(boolean));
                    }
                    if arg_name == "default" {
                        result_context.insert(S!("default"), ContextValue::BOOLEAN(true)); //set to True as the value is not really useful for now, but we want the key in context if one default is set
                    }
                },
                _ => {}
            }
        }

        result_context.extend([
            (S!("field_parent"), ContextValue::SYMBOL(parent.into())),
        ]);
        let weak_eval = match context.get(&S!("constructing_class")) {
            Some(ContextValue::SYMBOL(weak)) if !weak.is_expired(&st!()) => *weak,
            _ => evaluation_sym.get_weak().weak,
        };
        return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak {
            weak: weak_eval,
            context: result_context,
            instance: Some(true),
            is_super: false
        }));
    }

    fn eval_init(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<SymbolKey>) -> Option<EvaluationSymbolPtr> {
        return PythonArchEvalHooks::eval_init_common(session, evaluation_sym, maybe_context, diagnostics, file_symbol, false, false)
    }

    fn eval_init_relational(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<SymbolKey>) -> Option<EvaluationSymbolPtr> {
        return PythonArchEvalHooks::eval_init_common(session, evaluation_sym, maybe_context, diagnostics, file_symbol, true, false)
    }

    fn eval_init_relational_one2many(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<SymbolKey>) -> Option<EvaluationSymbolPtr> {
        return PythonArchEvalHooks::eval_init_common(session, evaluation_sym, maybe_context, diagnostics, file_symbol, true, true)
    }

    fn _update_field_init(symbol_table: &mut SymbolTable, symbol: SymbolKey, relational: Option<OYarn>) {
        let init_sym = symbol_table.get_symbol(symbol, &(vec![], vec![Sy!("__init__")]), u32::MAX);
        if init_sym.is_empty() {
            return;
        }
        symbol_table.set_evaluations(*init_sym.last().unwrap(), vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Weak::from(symbol), //use the weak to keep reference to the class for the hook.
                Some(true),
                HashMap::new(),
                Some(match relational {
                    Some(oyarn) if oyarn == oyarn!("One2many") => GetSymbolHook{callable: PythonArchEvalHooks::eval_init_relational_one2many, name: S!("eval_init_relational_one2many")},
                    Some(_) => GetSymbolHook{callable: PythonArchEvalHooks::eval_init_relational, name: S!("eval_init_relational")},
                    None => GetSymbolHook{callable: PythonArchEvalHooks::eval_init, name: S!("eval_init")},
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
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let mut diagnostics = vec![];
        let Some(Expr::StringLiteral(expr)) = arguments.args.first() else {return diagnostics};
        let returns_str = expr.value.to_string();
        if returns_str == S!("self"){
            if let Some(base) = st!().get_in_parents(func_sym.into(), &vec![SymType::CLASS], true) {
                let is_class_method = st!()[func_sym].is_class_method;
                st!()[func_sym].evaluations = vec![Evaluation::new_self(base, Some(!is_class_method))];
            }
            return diagnostics;
        }
        let Some(model) = session.sync_odoo.models.get(&oyarn!("{}", returns_str)).cloned() else {
            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS03002, &[]) {
                diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&expr.range()),
                    ..diagnostic_base
                });
            };
            return diagnostics;
        };
        let Some(&main_model_sym) = model.borrow().get_main_symbols(session, st!().find_module(func_sym)).first() else {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03001, &[]) {
                diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&expr.range()),
                    ..diagnostic
                });
            }
            return diagnostics
        };
        st!()[func_sym].evaluations = vec![Evaluation::eval_from_symbol(&st!(), main_model_sym, Some(false))];
        diagnostics
    }

    /// For @api.constrains and @api.onchange, both can only take a simple field name
    fn handle_api_simple_field_decorator(session: &mut SessionInfo, func_sym: FunctionKey, arguments: &Arguments) -> Vec<Diagnostic>{
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let mut diagnostics = vec![];
        let from_module = st!().find_module(func_sym);

        let Some(class_sym) = st!().get_in_parents(func_sym.into(), &vec![SymType::CLASS], true) else {
            return diagnostics;
        };

        let class_key = class_sym.unwrap_class_key();
        let Some(model_name) = st!()[class_key]._model.as_ref().map(|model| &model.name).cloned() else {
            return diagnostics;
        };

        for arg in arguments.args.iter() {
            let Expr::StringLiteral(expr) = arg else {return diagnostics};
            let field_name = expr.value.to_string();
            let (syms, _) = SymbolTable::get_member_symbol(session, class_sym, &field_name, from_module, false, true, false, true, false);
            if syms.is_empty() {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03014, &[&field_name, &model_name]) {
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
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let mut diagnostics = vec![];
        let from_module = st!().find_module(func_sym);

        let Some(class_sym) = st!().get_in_parents(func_sym.into(), &vec![SymType::CLASS], true) else {
            return diagnostics;
        };

        let class_key = class_sym.unwrap_class_key();
        let Some(model_name) = st!()[class_key]._model.as_ref().map(|model| &model.name).cloned() else {
            return diagnostics;
        };

        for arg in arguments.args.iter() {
            let Expr::StringLiteral(expr) = arg else {return diagnostics};
            let field_name = expr.value.to_string();
            let syms = PythonArchEval::get_nested_sub_field(session, &field_name, class_key, from_module);
            if syms.is_empty() {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03014, &[&field_name, &model_name]) {
                    diagnostics.push(Diagnostic {
                        range: FileMgr::textRange_to_temporary_Range(&expr.range()),
                        ..diagnostic
                    });
                }
            }
        }
        diagnostics
    }

    fn eval_env_ref(session: &mut SessionInfo, _evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let Some(context) = context else {return None};
        let in_validation = context.get(&S!("is_in_validation")).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool();
        let Some(parameters) = context.get(&S!("parameters")).map(|ps| ps.as_arguments()) else {return None};
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
        let module_key = module.unwrap().upgrade(&st!())?;
        if let Some(scope) = scope && let Some(file) = st!().get_file(scope) {
            st!().add_dependency(file, module_key.into(), BuildSteps::VALIDATION, BuildSteps::ARCH);
        }
        let Some(_symbol) = st!()[module_key].xml_id_locations.get(xml_id.as_str()) else {
            if in_validation {
                /*if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05001, &[]) {
                    diagnostics.push(Diagnostic {
                        range: FileMgr::textRange_to_temporary_Range(&xml_id_expr.range()),
                        ..diagnostic
                    });
                }*/ //removed, because there is too many valid place where we can't evaluate it correctly (see stock tests)
            }
            return None;
        };
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
                HashMap::new(),
                Some(GetSymbolHook{callable: PythonArchEvalHooks::eval_env_ref, name: S!("eval_env_ref")})
            ),
            value: None,
            range: None
        }];

        diagnostics
    }

}
