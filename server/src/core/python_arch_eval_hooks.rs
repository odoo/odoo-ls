use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;
use std::rc::Weak;
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
use crate::core::odoo::SyncOdoo;
use crate::core::evaluation::Context;
use crate::core::symbols::symbol::Symbol;
use crate::constants::*;
use crate::oyarn;
use crate::threads::SessionInfo;
use crate::utils::compare_semver;
use crate::Sy;
use crate::S;

use super::entry_point::EntryPoint;
use super::evaluation::{ContextValue, Evaluation, EvaluationSymbolPtr, EvaluationSymbol, EvaluationSymbolWeak};
use super::file_mgr::FileMgr;
use super::python_arch_eval::PythonArchEval;

type PythonArchEvalHookFile = fn (odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>);

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
                        func: |odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let env_file = odoo.sync_odoo.get_symbol(odoo.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("api")], vec![]), u32::MAX);
        let env_class = odoo.sync_odoo.get_symbol(odoo.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment")]), u32::MAX);
        if !env_class.is_empty() {
            let mut env = symbol.borrow_mut();
            let env_class = env_class.last().unwrap();
            let context = HashMap::new();
            env.set_evaluations(vec![Evaluation {
                symbol: EvaluationSymbol::new_with_symbol(
                    Rc::downgrade(env_class),
                    Some(true),
                    context,
                    None,
                ),
                value: None,
                range: None,
            }]);
            file_symbol.borrow_mut().add_dependency(&mut env_file.last().unwrap().borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
            env.set_doc_string(Some(S!("")));
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                        trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("ids")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("ids")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let values: Vec<ruff_python_ast::Expr> = Vec::new();
        let mut id = symbol.borrow_mut();
        let range = id.range().clone();
        id.set_evaluations(vec![Evaluation::new_list(odoo.sync_odoo, values, range)]);
    }},
    /*PythonArchEvalFileHook {file_tree: vec![Sy!("odoo"), Sy!("models")],
                        content_tree: vec![Sy!("BaseModel"), Sy!("search_count")],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let values: Vec<ruff_python_ast::Expr> = Vec::new();
        let mut id = symbol.borrow_mut();
        let range = id.range().clone();
        id.set_evaluations(vec![Evaluation::eval_from_symbol(odoo, values, range.clone())]);
    }},*/
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment"), Sy!("registry")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("environments")], vec![Sy!("Environment"), Sy!("registry")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let registry_sym = odoo.sync_odoo.get_symbol(odoo.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("modules"), Sy!("registry")], vec![Sy!("Registry")]), u32::MAX);
        if !registry_sym.is_empty() {
            symbol.borrow_mut().set_evaluations(vec![Evaluation {
                symbol: EvaluationSymbol::new_with_symbol(
                    Rc::downgrade(registry_sym.last().unwrap()),
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
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("bool")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Integer")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_numeric")], vec![Sy!("Integer")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("int")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Float")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_numeric")], vec![Sy!("Float")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("float")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Monetary")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_numeric")], vec![Sy!("Monetary")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("float")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Char")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_textual")], vec![Sy!("Char")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Text")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_textual")], vec![Sy!("Text")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Html")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_textual")], vec![Sy!("Html")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("markupsafe")], vec![Sy!("Markup")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Date")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_temporal")], vec![Sy!("Date")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("datetime")], vec![Sy!("date")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Datetime")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_temporal")], vec![Sy!("Datetime")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("datetime")], vec![Sy!("datetime")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Binary")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_binary")], vec![Sy!("Binary")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("bytes")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Image")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_binary")], vec![Sy!("Image")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("bytes")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Selection")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_selection")], vec![Sy!("Selection")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Reference")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_reference")], vec![Sy!("Reference")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Json")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_misc")], vec![Sy!("Json")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("object")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Properties")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_properties")], vec![Sy!("Properties")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("object")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("PropertiesDefinition")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_properties")], vec![Sy!("PropertiesDefinition")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo.sync_odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("object")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2one")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("Many2one")]))],
                            if_exist_only: true,
                            func: |_odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_relational(symbol.clone());
        PythonArchEvalHooks::_update_field_init(symbol.clone(), true);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("One2many")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("One2many")]))],
                            if_exist_only: true,
                            func: |_odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
                PythonArchEvalHooks::_update_field_init(symbol.clone(), true);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2many")])),
                            (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("Many2many")]))],
                            if_exist_only: true,
                            func: |_odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_relational(symbol.clone());
        PythonArchEvalHooks::_update_field_init(symbol.clone(), true);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("init")], vec![Sy!("_")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let odoo_underscore = odoo.sync_odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("odoo")], vec![Sy!("_")]), u32::MAX);
        if let Some(eval_1) = odoo_underscore.first() {
            eval_1.borrow_mut().set_evaluations(vec![Evaluation::eval_from_symbol(&Rc::downgrade(&symbol), Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("init")], vec![Sy!("SUPERUSER_ID")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let odoo_superuser_id = odoo.sync_odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("odoo")], vec![Sy!("SUPERUSER_ID")]), u32::MAX);
        if let Some(eval_1) = odoo_superuser_id.first() {
            eval_1.borrow_mut().set_evaluations(vec![Evaluation::eval_from_symbol(&Rc::downgrade(&symbol), Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("init")], vec![Sy!("_lt")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let odoo_lt = odoo.sync_odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("odoo")], vec![Sy!("_lt")]), u32::MAX);
        if let Some(eval_1) = odoo_lt.first() {
            eval_1.borrow_mut().set_evaluations(vec![Evaluation::eval_from_symbol(&Rc::downgrade(&symbol), Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("init")], vec![Sy!("Command")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let odoo_command = odoo.sync_odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("odoo")], vec![Sy!("Command")]), u32::MAX);
        if let Some(eval_1) = odoo_command.first() {
            eval_1.borrow_mut().set_evaluations(vec![Evaluation::eval_from_symbol(&Rc::downgrade(&symbol), Some(false))]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("15.0"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("addons"), Sy!("base"), Sy!("models"), Sy!("ir_rule")], vec![Sy!("IrRule"), Sy!("global")]))],
                            if_exist_only: true,
                            func: |odoo: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let mut boolean_field = odoo.sync_odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Boolean")]), u32::MAX);
        if compare_semver(odoo.sync_odoo.full_version.as_str(), "18.1") >= Ordering::Equal {
            boolean_field = odoo.sync_odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_misc")], vec![Sy!("Boolean")]), u32::MAX);
        }
        if let Some(boolean) = boolean_field.first() {
            let mut eval = Evaluation::eval_from_symbol(&Rc::downgrade(&boolean), Some(true));
            let weak = eval.symbol.get_mut_symbol_ptr().as_mut_weak();
            weak.context.insert(S!("compute"), ContextValue::STRING(S!("_compute_global")));
            symbol.borrow_mut().set_evaluations(vec![eval]);
        }
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            trees: vec![(Sy!("0.0"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("_monkeypatches"), Sy!("werkzeug")], vec![]))],
                            if_exist_only: true,
                            func: |session: &mut SessionInfo, _entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let odoo = &mut session.sync_odoo;
        let url_decode = symbol.borrow().get_symbol(&(vec![], vec![Sy!("url_decode")]), u32::MAX);
        let werkzeug_url_decode = odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_decode")]), u32::MAX);
        if let Some(werkzeug_url_decode) = werkzeug_url_decode.first() {
            if werkzeug_url_decode.borrow().typ() == SymType::VARIABLE { //if not variable, no need to patch it
                if let Some(eval_1) = url_decode.first() {
                    werkzeug_url_decode.borrow_mut().set_evaluations(vec![
                        Evaluation::eval_from_symbol(&Rc::downgrade(&eval_1), Some(false))
                    ]);
                }
            }
        }
        let url_encode = symbol.borrow().get_symbol(&(vec![], vec![Sy!("url_encode")]), u32::MAX);
        let werkzeug_url_encode = odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_encode")]), u32::MAX);
        if let Some(werkzeug_url_encode) = werkzeug_url_encode.first() {
            if werkzeug_url_encode.borrow().typ() == SymType::VARIABLE { //if not variable, no need to patch it
                if let Some(eval_1) = url_encode.first() {
                    werkzeug_url_encode.borrow_mut().set_evaluations(vec![
                        Evaluation::eval_from_symbol(&Rc::downgrade(&eval_1), Some(false))
                    ]);
                }
            }
        }
        let url_join = symbol.borrow().get_symbol(&(vec![], vec![Sy!("url_join")]), u32::MAX);
        let werkzeug_url_join = odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_join")]), u32::MAX);
        if let Some(werkzeug_url_join) = werkzeug_url_join.first() {
            if werkzeug_url_join.borrow().typ() == SymType::VARIABLE { //if not variable, no need to patch it
                if let Some(eval_1) = url_join.first() {
                    werkzeug_url_join.borrow_mut().set_evaluations(vec![
                        Evaluation::eval_from_symbol(&Rc::downgrade(&eval_1), Some(false))
                    ]);
                }
            }
        }
        let url_parse = symbol.borrow().get_symbol(&(vec![], vec![Sy!("url_parse")]), u32::MAX);
        let werkzeug_url_parse = odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_parse")]), u32::MAX);
        if let Some(werkzeug_url_parse) = werkzeug_url_parse.first() {
            if werkzeug_url_parse.borrow().typ() == SymType::VARIABLE { //if not variable, no need to patch it
                if let Some(eval_1) = url_parse.first() {
                    werkzeug_url_parse.borrow_mut().set_evaluations(vec![
                        Evaluation::eval_from_symbol(&Rc::downgrade(&eval_1), Some(false))
                    ]);
                }
            }
        }
        let url_quote = symbol.borrow().get_symbol(&(vec![], vec![Sy!("url_quote")]), u32::MAX);
        let werkzeug_url_quote = odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_quote")]), u32::MAX);
        if let Some(werkzeug_url_quote) = werkzeug_url_quote.first() {
            if werkzeug_url_quote.borrow().typ() == SymType::VARIABLE { //if not variable, no need to patch it
                if let Some(eval_1) = url_quote.first() {
                    werkzeug_url_quote.borrow_mut().set_evaluations(vec![
                        Evaluation::eval_from_symbol(&Rc::downgrade(&eval_1), Some(false))
                    ]);
                }
            }
        }
        let url_unquote = symbol.borrow().get_symbol(&(vec![], vec![Sy!("url_unquote")]), u32::MAX);
        let werkzeug_url_unquote = odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_unquote")]), u32::MAX);
        if let Some(werkzeug_url_unquote) = werkzeug_url_unquote.first() {
            if werkzeug_url_unquote.borrow().typ() == SymType::VARIABLE { //if not variable, no need to patch it
                if let Some(eval_1) = url_unquote.first() {
                    werkzeug_url_unquote.borrow_mut().set_evaluations(vec![
                        Evaluation::eval_from_symbol(&Rc::downgrade(&eval_1), Some(false))
                    ]);
                }
            }
        }
        let url_quote_plus = symbol.borrow().get_symbol(&(vec![], vec![Sy!("url_quote_plus")]), u32::MAX);
        let werkzeug_url_quote_plus = odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_quote_plus")]), u32::MAX);
        if let Some(werkzeug_url_quote_plus) = werkzeug_url_quote_plus.first() {
            if werkzeug_url_quote_plus.borrow().typ() == SymType::VARIABLE { //if not variable, no need to patch it
                if let Some(eval_1) = url_quote_plus.first() {
                    werkzeug_url_quote_plus.borrow_mut().set_evaluations(vec![
                        Evaluation::eval_from_symbol(&Rc::downgrade(&eval_1), Some(false))
                    ]);
                }
            }
        }
        let url_unquote_plus = symbol.borrow().get_symbol(&(vec![], vec![Sy!("url_unquote_plus")]), u32::MAX);
        let werkzeug_url_unquote_plus = odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_unquote_plus")]), u32::MAX);
        if let Some(werkzeug_url_unquote_plus) = werkzeug_url_unquote_plus.first() {
            if werkzeug_url_unquote_plus.borrow().typ() == SymType::VARIABLE { //if not variable, no need to patch it
                if let Some(eval_1) = url_unquote_plus.first() {
                    werkzeug_url_unquote_plus.borrow_mut().set_evaluations(vec![
                        Evaluation::eval_from_symbol(&Rc::downgrade(&eval_1), Some(false))
                    ]);
                }
            }
        }
        let url_unparse = symbol.borrow().get_symbol(&(vec![], vec![Sy!("url_unparse")]), u32::MAX);
        let werkzeug_url_unparse = odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("url_unparse")]), u32::MAX);
        if let Some(werkzeug_url_unparse) = werkzeug_url_unparse.first() {
            if werkzeug_url_unparse.borrow().typ() == SymType::VARIABLE { //if not variable, no need to patch it
                if let Some(eval_1) = url_unparse.first() {
                    werkzeug_url_unparse.borrow_mut().set_evaluations(vec![
                        Evaluation::eval_from_symbol(&Rc::downgrade(&eval_1), Some(false))
                    ]);
                }
            }
        }
        let url = symbol.borrow().get_symbol(&(vec![], vec![Sy!("URL")]), u32::MAX);
        let werkzeug_url_syms = odoo.get_symbol(_file_symbol.borrow().paths()[0].as_str(), &(vec![Sy!("werkzeug"), Sy!("urls")], vec![Sy!("URL")]), u32::MAX);
        if let Some(werkzeug_url) = werkzeug_url_syms.first() {
            if werkzeug_url.borrow().typ() == SymType::VARIABLE { //if not variable, no need to patch it
                if let Some(eval_1) = url.first() {
                    werkzeug_url.borrow_mut().set_evaluations(vec![
                        Evaluation::eval_from_symbol(&Rc::downgrade(&eval_1), Some(false))
                    ]);
                }
            }
        }
    }},
]});

type PythonArchEvalHookFunc = fn (odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>);

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
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(Weak::new(),
                Some(true),
                HashMap::from([(S!("hook_name"), ContextValue::STRING(S!("eval_env_get_item")))]),
                Some(PythonArchEvalHooks::eval_env_get_item)
            ),
            value: None,
            range: None
        }]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("modules"), Sy!("registry")], vec![Sy!("Registry"), Sy!("__getitem__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("registry")], vec![Sy!("Registry"), Sy!("__getitem__")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(Weak::new(),
                Some(true),
                HashMap::new(),
                Some(PythonArchEvalHooks::eval_registry_get_item)
            ),
            value: None,
            range: None
        }]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("__iter__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("__iter__")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_env")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_env")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("sudo")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("sudo")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("create")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("create")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("mapped")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("mapped")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("search")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("search")]))],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut search: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        let func = search.as_func_mut();
        if func.args.len() > 1 {
            if let Some(arg_symbol) = func.args.get(1).unwrap().symbol.upgrade() {
                if arg_symbol.borrow().name().eq(&Sy!("domain")) {
                    arg_symbol.borrow_mut().set_evaluations(vec![Evaluation::new_domain(odoo)]);
                } else {
                    warn!("domain not found on search signature")
                }
            }
        }
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("browse")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("browse")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_company")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_company")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_context")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_context")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_prefetch")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_prefetch")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_user")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_user")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("exists")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("exists")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Id"), Sy!("__get__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_misc")], vec![Sy!("Id"), Sy!("__get__")]))], //We have to put it at function level hook to remove evaluation from existing code
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_func_level(odoo, &entry_point, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("int")]));
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("One2many"), Sy!("__get__")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields_relational")], vec![Sy!("One2many"), Sy!("__get__")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_func_relational(symbol.clone());
    }},
    PythonArchEvalFunctionHook {
                        odoo_entry: true,
                        tree: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment"), Sy!("ref")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("environments")], vec![Sy!("Environment"), Sy!("ref")]))],
                        if_exist_only: true,
                        func: |_odoo: &mut SyncOdoo, _entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_validation_env_ref(symbol.clone());
    }},
]});


type PythonArchEvalHookDecorator = fn (session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments) -> Vec<Diagnostic>;

pub struct PythonArchEvalDecoratorHook {
    pub trees: Vec<(OYarn, OYarn, Tree)>, //min_version, max_version, tree
    pub func: PythonArchEvalHookDecorator
}

#[allow(non_upper_case_globals)]
static arch_eval_decorator_hooks: Lazy<Vec<PythonArchEvalDecoratorHook>> = Lazy::new(|| {vec![
    PythonArchEvalDecoratorHook {trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("returns")]))], //disappear in 18.1
                        func: |session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_returns_decorator(session, func_sym, arguments)
    }},
    PythonArchEvalDecoratorHook {trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("onchange")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("decorators")], vec![Sy!("onchange")]))],
                        func: |session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_simple_field_decorator(session, func_sym, arguments)
    }},
    PythonArchEvalDecoratorHook {trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("constrains")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("decorators")], vec![Sy!("constrains")]))],
                        func: |session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_simple_field_decorator(session, func_sym, arguments)
    }},
    PythonArchEvalDecoratorHook {trees: vec![(Sy!("0.0"), Sy!("18.1"), (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("depends")])),
                        (Sy!("18.1"), Sy!("999.0"), (vec![Sy!("odoo"), Sy!("orm"), Sy!("decorators")], vec![Sy!("depends")]))],
                        func: |session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments| {
                            PythonArchEvalHooks::handle_api_nested_field_decorator(session, func_sym, arguments)
    }},
]});
pub struct PythonArchEvalHooks {
}

impl PythonArchEvalHooks {

    pub fn on_file_eval(session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>) {
        let tree = symbol.borrow().get_tree();
        let odoo_tree = symbol.borrow().get_main_entry_tree(session);
        let name = symbol.borrow().name().clone();
        for hook in arch_eval_file_hooks.iter() {
            for (min_version, max_version, hook_tree) in hook.trees.iter() {
                if compare_semver(min_version, &session.sync_odoo.full_version) == Ordering::Greater || 
                    compare_semver(max_version, &session.sync_odoo.full_version) <= Ordering::Equal {
                    continue; //skip if version not in range
                }
                if name.eq(hook_tree.0.last().unwrap()) &&
                ((hook.odoo_entry && session.sync_odoo.has_main_entry && odoo_tree.0 == hook_tree.0) || (!hook.odoo_entry && tree.0 == hook_tree.0)) {
                    if hook_tree.1.is_empty() {
                        (hook.func)(session, entry_point, symbol.clone(), symbol.clone());
                    } else {
                        let sub_symbol = symbol.borrow().get_symbol(&(vec![], hook_tree.1.clone()), u32::MAX);
                        if !sub_symbol.is_empty() {
                            (hook.func)(session, entry_point, symbol.clone(), sub_symbol.last().unwrap().clone());
                        }
                    }
                }
            }
        }
    }

    pub fn on_function_eval(session: &mut SessionInfo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>) {
        let tree = symbol.borrow().get_tree();
        let odoo_tree = symbol.borrow().get_main_entry_tree(session);
        let name = symbol.borrow().name().clone();
        for hook in arch_eval_function_hooks.iter() {
            for hook_tree in hook.tree.iter() {
                if compare_semver(hook_tree.0.as_str(), session.sync_odoo.full_version.as_str()) == Ordering::Greater || 
                    compare_semver(hook_tree.1.as_str(), session.sync_odoo.full_version.as_str()) <= Ordering::Equal {
                    continue; //skip if version not in range
                }
                if name.eq(hook_tree.2.1.last().unwrap()) {
                    if (hook.odoo_entry && session.sync_odoo.has_main_entry && odoo_tree == hook_tree.2) || (!hook.odoo_entry && tree == hook_tree.2) {
                        (hook.func)(session.sync_odoo, entry_point, symbol.clone());
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
        func_sym: Rc<RefCell<Symbol>>,
        file: Rc<RefCell<Symbol>>,
        current_step: BuildSteps,
    ) -> Vec<Diagnostic>{
        let mut diagnostics = vec![];
        for decorator in func_stmt.decorator_list.iter(){
            let (decorator_base, decorator_args) = match &decorator.expression {
                Expr::Call(call_expr) => {
                    (&call_expr.func, &call_expr.arguments)
                },
                _ => {continue;}
            };
            if decorator_args.args.is_empty(){
                continue; // All the decorators we handle have at least one arg for now
            }
            let Some(parent) = func_sym.borrow().parent().and_then(|weak_parent| weak_parent.upgrade()).clone() else {
                return diagnostics // failed to find parent
            };
            let mut deps = vec![vec![], vec![], vec![]];
            let (dec_evals, diags) = Evaluation::eval_from_ast(session, &decorator_base, parent, &func_stmt.range.start(), false, &mut deps);
            Symbol::insert_dependencies(&file, &mut deps, current_step);
            diagnostics.extend(diags);
            let mut followed_evals = vec![];
            for eval in dec_evals {
                followed_evals.extend(Symbol::follow_ref(&eval.symbol.get_symbol(session, &mut None, &mut vec![], None), session, &mut None, true, false, None));
            }
            for decorator_eval in followed_evals {
                let EvaluationSymbolPtr::WEAK(decorator_eval_sym_weak) = decorator_eval else {
                    continue;
                };
                let Some(dec_sym) = decorator_eval_sym_weak.weak.upgrade() else {
                    continue;
                };
                let dec_sym_tree = dec_sym.borrow().get_tree();
                for hook in arch_eval_decorator_hooks.iter() {
                    for (min_version, max_version, hook_tree) in hook.trees.iter() {
                        if compare_semver(min_version, &session.sync_odoo.full_version) == Ordering::Greater ||
                            compare_semver(max_version,  &session.sync_odoo.full_version.as_str()) <= Ordering::Equal {
                            continue; //skip if version not in range
                        }
                        if !dec_sym_tree.0.ends_with(&hook_tree.0) || !dec_sym_tree.1.ends_with(&hook_tree.1) || !SyncOdoo::is_in_main_entry(session, &dec_sym_tree.0) {
                            continue;
                        }
                        diagnostics.extend((hook.func)(session, func_sym.clone(), decorator_args));
                    }
                }
            }
        }
        diagnostics
    }

    pub fn eval_env_get_item(session: &mut SessionInfo, _evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>
    {
        let res = Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(Weak::new(), Some(true), false)));
        let Some(context) = context else {
            return res
        };
        let in_validation = context.get(&S!("is_in_validation")).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool();
        let Some(ContextValue::STRING(s)) = context.get(&S!("args")) else {
            return res
        };
        let maybe_model = session.sync_odoo.models.get(&oyarn!("{}", s));
        let has_class_in_parents = scope.as_ref().map(|scope| scope.borrow().get_in_parents(&vec![SymType::CLASS], true).is_some()).unwrap_or(false);
        if maybe_model.map(|m| m.borrow_mut().has_symbols()).unwrap_or(false) {
            let Some(model) = maybe_model else {unreachable!()};
            let module = context.get(&S!("module"));
            let from_module = if let Some(ContextValue::MODULE(m)) = module {
                m.upgrade().clone()
            } else {
                None
            };
            if let Some(scope) = scope
                .and_then(|s| s.borrow().get_file())
                .and_then(|w| w.upgrade()) {
                let env_files = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("api")], vec![]), u32::MAX);
                let env_file = env_files.last().unwrap();
                if !Rc::ptr_eq(env_file, &scope) {
                    let mut f = scope.borrow_mut();
                    f.add_model_dependencies(model);
                }
            }
            let model = model.clone();
            let model = model.borrow();
            let symbols = model.get_main_symbols(session, from_module.clone());
            if let Some(first_symbol) = symbols.first() {
                return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(Rc::downgrade(first_symbol), Some(true), false)));
            }
            if in_validation && has_class_in_parents { //we don't want to show error for functions outside of a model body
                if from_module.is_some(){
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
                        let valid_modules: Vec<OYarn> = symbols.iter().map(|s| match s.borrow().find_module() {
                            Some(sym) => sym.borrow().name().clone(),
                            None => Sy!("Unknown").clone()
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
            let Some(file_symbol) = scope.and_then(|scope| scope.borrow().get_file()).and_then(|file| file.upgrade()) else {
              return res
            };
            file_symbol.borrow_mut().as_file_mut().not_found_models.insert(Sy!(s.clone()), BuildSteps::VALIDATION);
            session.sync_odoo.get_main_entry().borrow_mut().not_found_symbols_for_models.insert(file_symbol.clone());
        }
        res
    }

    pub fn eval_registry_get_item(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>
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

    fn eval_get(_session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, _diagnostics: &mut Vec<Diagnostic>, _scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>
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

    fn _update_get_eval_func_level(odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, function: Rc<RefCell<Symbol>>, tree: Tree) {
        let return_sym = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &tree, u32::MAX);
        if return_sym.is_empty() {
            let file = function.borrow_mut().get_file().clone();
            file.as_ref().unwrap().upgrade().unwrap().borrow_mut().not_found_paths_mut().push((BuildSteps::ARCH_EVAL, flatten_tree(&tree)));
            entry_point.borrow_mut().not_found_symbols.insert(function.borrow().parent().unwrap().upgrade().unwrap());
            return;
        }
        function.borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Rc::downgrade(return_sym.last().unwrap()),
                Some(true),
                HashMap::new(),
                Some(PythonArchEvalHooks::eval_get)
            ),
            value: None,
            range: None
        }]);
    }

    fn _update_get_eval(odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>, tree: Tree) {
        let get_syms = symbol.borrow().get_symbol(&(vec![], vec![Sy!("__get__")]), u32::MAX);
        let Some(get_sym) = get_syms.last() else {
            return;
        };
        let return_syms = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &tree, u32::MAX);
        let Some(return_sym) = return_syms.last() else {
            let file = symbol.borrow().get_file().clone();
            file.as_ref().unwrap().upgrade().unwrap().borrow_mut().not_found_paths_mut().push((BuildSteps::ARCH_EVAL, flatten_tree(&tree)));
            entry_point.borrow_mut().not_found_symbols.insert(symbol);
            return;
        };
        get_sym.borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Rc::downgrade(return_sym),
                Some(true),
                HashMap::new(),
                Some(PythonArchEvalHooks::eval_get)
            ),
            value: None,
            range: None
        }]);

        let tree = if compare_semver(odoo.full_version.as_str(), "18.1.0") == Ordering::Less {
            (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Field"), Sy!("__get__")])
        } else {
            (vec![Sy!("odoo"), Sy!("orm"), Sy!("fields")], vec![Sy!("Field"), Sy!("__get__")])
        };
        let Some(field_get) = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(),  &tree, u32::MAX).first().cloned()
        else {
            return;
        };
        let field_get_borrowed = field_get.borrow();
        get_sym.borrow_mut().as_func_mut().args = field_get_borrowed.as_func().args.clone();
    }
    fn eval_relational_with_related(session: &mut SessionInfo, related_field: &ContextValue, context: &Context) -> Option<EvaluationSymbolPtr>{
        let Some(ContextValue::SYMBOL(class_sym_weak)) = context.get(&S!("field_parent")) else {return None};
        let Some(class_sym) = class_sym_weak.upgrade() else {return None};
        let related_field_name = related_field.as_string();
        let from_module = class_sym.borrow().find_module();
        let syms = PythonArchEval::get_nested_sub_field(session, &related_field_name, class_sym.clone(), from_module.clone());
        if let Some(symbol) = syms.first(){
            return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: Rc::downgrade(symbol), context: HashMap::new(), instance: Some(true), is_super: false}))
        }
        None
    }

    fn eval_relational_with_comodel(session: &mut SessionInfo, comodel: &ContextValue, context: &Context, scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>{
        let comodel = oyarn!("{}", comodel.as_string());
        let comodel_sym = session.sync_odoo.models.get(&comodel).cloned();
        if let Some(comodel_sym) = comodel_sym {
            // Add dependency
            if let Some(scope) = scope
                .and_then(|s| s.borrow().get_file())
                .and_then(|w| w.upgrade()) {
                let mut f = scope.borrow_mut();
                f.add_model_dependencies(&comodel_sym);
            }
            let module = context.get(&S!("module"));
            let mut from_module = None;
            if let Some(ContextValue::MODULE(m)) = module {
                if let Some(m) = m.upgrade() {
                    from_module = Some(m.clone());
                }
            }
            let main_symbol = comodel_sym.borrow().get_main_symbols(session, from_module);
            if main_symbol.len() == 1 {
                return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: Rc::downgrade(&main_symbol[0]), context: HashMap::new(), instance: Some(true), is_super: false}))
            }
        }
        Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: Weak::new(), context: HashMap::new(), instance: Some(true), is_super: false}))
    }

    fn eval_relational(session: &mut SessionInfo, _evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, _diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>
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

    fn _update_get_eval_relational(symbol: Rc<RefCell<Symbol>>) {
        let get_sym = symbol.borrow().get_symbol(&(vec![], vec![Sy!("__get__")]), u32::MAX);
        if get_sym.is_empty() {
            return;
        }
        get_sym.last().unwrap().borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Weak::new(),
                Some(true),
                HashMap::new(),
                Some(PythonArchEvalHooks::eval_relational)
            ),
            value: None,
            range: None,
        }]);
    }

    fn _update_get_eval_func_relational(get_symbol: Rc<RefCell<Symbol>>) {
        get_symbol.borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Weak::new(),
                Some(true),
                HashMap::new(),
                Some(PythonArchEvalHooks::eval_relational)
            ),
            value: None,
            range: None,
        }]);
    }

    fn find_special_arguments<'a>(parameters: &'a Arguments, arg_name: &str) -> Option<(&'a Expr, TextRange)> {
        parameters.keywords.iter().find_map(|keyword| {
            keyword.arg
                .as_ref().filter(|kw_arg| kw_arg.id == arg_name)
                .map(|_| (&keyword.value, keyword.range()))
        })
    }

    fn eval_init_common(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: &mut Option<Context>, _diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<Rc<RefCell<Symbol>>>, relational: bool) -> Option<EvaluationSymbolPtr>
    {
        let Some(context) = maybe_context else {return None};

        let Some(parameters) = context.get(&S!("parameters")).map(|ps| ps.as_arguments()) else {return None};

        let parent = Symbol::get_scope_symbol(
            file_symbol.unwrap().clone(),
            context.get(&S!("range")).unwrap().as_text_range().start().to_u32(),
            false
        );
        let mut context = HashMap::new();

        let mut contexts_to_add = HashMap::new();
        if relational {
            if let Some(first_param) = parameters.args.get(0) {
                contexts_to_add.insert("comodel_name", (first_param, first_param.range(), "str"));
            }
        }

        // Keyword Arguments for fields that we would like to keep in the context
        let context_arguments = [
            ("comodel_name", "str"),
            ("related", "str"),
            ("compute", "str"),
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
                "str" => if let Some(related_string) = Evaluation::expr_to_str(session, field_name_expr, parent.clone(), &parameters.range.start(), false, &mut vec![]).0 {
                    context.insert(S!(arg_name), ContextValue::STRING(related_string.to_string()));
                    context.insert(format!("{arg_name}_arg_range"), ContextValue::RANGE(arg_range.clone()));
                },
                "bool" => {
                    let maybe_boolean = Evaluation::expr_to_bool(session, field_name_expr, parent.clone(), &parameters.range.start(), false, &mut vec![]).0;
                    if let Some(boolean) = maybe_boolean {
                        context.insert(S!(arg_name), ContextValue::BOOLEAN(boolean));
                    }
                    if arg_name == "default" {
                        context.insert(S!("default"), ContextValue::BOOLEAN(true)); //set to True as the value is not really useful for now, but we want the key in context if one default is set
                    }
                },
                _ => {}
            }
        }

        context.extend([
            (S!("field_parent"), ContextValue::SYMBOL(Rc::downgrade(&parent))),
        ]);
        return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak {
            weak: evaluation_sym.get_weak().weak.clone(),
            context,
            instance: Some(true),
            is_super: false
        }));
    }

    fn eval_init(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr> {
        return PythonArchEvalHooks::eval_init_common(session, evaluation_sym, maybe_context, diagnostics, file_symbol, false)
    }

    fn eval_init_relational(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, maybe_context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr> {
        return PythonArchEvalHooks::eval_init_common(session, evaluation_sym, maybe_context, diagnostics, file_symbol, true)
    }

    fn _update_field_init(symbol: Rc<RefCell<Symbol>>, relational: bool) {
        let init_sym = symbol.borrow().get_symbol(&(vec![], vec![Sy!("__init__")]), u32::MAX);
        if init_sym.is_empty() {
            return;
        }
        init_sym.last().unwrap().borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Rc::downgrade(&symbol), //use the weak to keep reference to the class for the hook.
                Some(true),
                HashMap::new(),
                Some(if relational {PythonArchEvalHooks::eval_init_relational} else {PythonArchEvalHooks::eval_init})
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
    fn handle_api_returns_decorator(session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments) -> Vec<Diagnostic>{
        let mut diagnostics = vec![];
        let Some(Expr::StringLiteral(expr)) = arguments.args.first() else {return diagnostics};
        let returns_str = expr.value.to_string();
        if returns_str == S!("self"){
            func_sym.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
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
        let Some(ref main_model_sym) =  model.borrow().get_main_symbols(session, func_sym.borrow().find_module()).first().cloned() else {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03001, &[]) {
                diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&expr.range()),
                    ..diagnostic
                });
            }
            return diagnostics
        };
        func_sym.borrow_mut().set_evaluations(vec![Evaluation::eval_from_symbol(&Rc::downgrade(main_model_sym), Some(false))]);
        diagnostics
    }

    /// For @api.constrains and @api.onchange, both can only take a simple field name
    fn handle_api_simple_field_decorator(session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments) -> Vec<Diagnostic>{
        let mut diagnostics = vec![];
        let from_module = func_sym.borrow().find_module();

        let Some(class_sym) = func_sym.borrow().get_in_parents(&vec![SymType::CLASS], true).and_then(
            |class_sym_weak| class_sym_weak.upgrade()
        ) else {
            return diagnostics;
        };

        let Some(model_name) = class_sym.borrow().as_class_sym()._model.as_ref().map(|model| &model.name).cloned() else {
            return diagnostics;
        };

        for arg in arguments.args.iter() {
            let Expr::StringLiteral(expr) = arg else {return diagnostics};
            let field_name = expr.value.to_string();
            let (syms, _) = class_sym.borrow().get_member_symbol(session, &field_name, from_module.clone(), false, false, true, false);
            if syms.is_empty(){
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
    fn handle_api_nested_field_decorator(session: &mut SessionInfo, func_sym: Rc<RefCell<Symbol>>, arguments: &Arguments) -> Vec<Diagnostic>{
        let mut diagnostics = vec![];
        let from_module = func_sym.borrow().find_module();

        let Some(class_sym) = func_sym.borrow().get_in_parents(&vec![SymType::CLASS], true).and_then(
            |class_sym_weak| class_sym_weak.upgrade()
        ) else {
            return diagnostics;
        };

        let Some(model_name) = class_sym.borrow().as_class_sym()._model.as_ref().map(|model| &model.name).cloned() else {
            return diagnostics;
        };

        for arg in arguments.args.iter() {
            let Expr::StringLiteral(expr) = arg else {return diagnostics};
            let field_name = expr.value.to_string();
            let syms = PythonArchEval::get_nested_sub_field(session, &field_name, class_sym.clone(), from_module.clone());
            if syms.is_empty(){
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

    fn eval_env_ref(session: &mut SessionInfo, _evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, _scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr> {
        let Some(context) = context else {return None};
        let in_validation = context.get(&S!("is_in_validation")).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool();
        let Some(parameters) = context.get(&S!("parameters")).map(|ps| ps.as_arguments()) else {return None};
        if parameters.args.is_empty() {
            return None; // No arguments to process
        }
        if !parameters.args[0].is_string_literal_expr() {
            return None;
        }
        if parameters.keywords.len() == 1 {
            if parameters.keywords[0].value.as_boolean_literal_expr().unwrap().value == false {
                return None; // No need to process if the second argument (raise_if_not_found) is false
            }
        }
        let xml_id_expr = parameters.args[0].as_string_literal_expr().unwrap();
        let xml_id_str = xml_id_expr.value.to_str();
        let mut xml_id_split = xml_id_str.split('.');
        let module_name = xml_id_split.next().unwrap();
        let xml_id = xml_id_split.collect::<Vec<&str>>().join(".");
        let module = session.sync_odoo.modules.get(module_name).cloned();
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
        let Some(module_rc) = module.unwrap().upgrade() else {
            return None;
        };
        let module_rc_bw = module_rc.borrow();
        let Some(_symbol) = module_rc_bw.as_module_package().xml_id_locations.get(xml_id.as_str()) else {
            if in_validation {
                /*diagnostics.push(Diagnostic::new(
                    FileMgr::textRange_to_temporary_Range(&xml_id_expr.range()),
                    Some(DiagnosticSeverity::ERROR),
                    Some(NumberOrString::String(S!("OLS30329"))),
                    Some(EXTENSION_NAME.to_string()),
                    S!("Unknown XML ID"),
                    None,
                    None
                ));*/ //removed, because there is too many valid place where we can't evaluate it correctly (see stock tests)
            }
            return None;
        };
        //TODO => csv xml_id
        //TODO check module dependencies
        //TODO in xml ONLY, ref can omit the 'module.' before the xml_id
        //TODO implement base.model_'nameofmodel' - to test
        return None; //TODO implement returned value
    }

    fn _validation_env_ref(func_sym: Rc<RefCell<Symbol>>) -> Vec<Diagnostic> {
        let diagnostics = vec![];
        func_sym.borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Rc::downgrade(&func_sym),
                Some(true),
                HashMap::new(),
                Some(PythonArchEvalHooks::eval_env_ref)
            ),
            value: None,
            range: None
        }]);

        diagnostics
    }

}
