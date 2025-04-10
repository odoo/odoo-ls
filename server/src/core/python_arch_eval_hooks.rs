use std::collections::HashMap;
use std::rc::Rc;
use std::rc::Weak;
use std::cell::RefCell;
use lsp_types::Diagnostic;
use lsp_types::DiagnosticSeverity;
use lsp_types::NumberOrString;
use once_cell::sync::Lazy;
use ruff_python_ast::Arguments;
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;
use crate::core::odoo::SyncOdoo;
use crate::core::evaluation::Context;
use crate::core::symbols::symbol::Symbol;
use crate::constants::*;
use crate::oyarn;
use crate::threads::SessionInfo;
use crate::Sy;
use crate::S;

use super::entry_point::EntryPoint;
use super::evaluation::{ContextValue, Evaluation, EvaluationSymbolPtr, EvaluationSymbol, EvaluationSymbolWeak};
use super::file_mgr::add_diagnostic;
use super::file_mgr::FileMgr;
use super::python_arch_eval::PythonArchEval;
use super::symbols::module_symbol::ModuleSymbol;

type PythonArchEvalHookFile = fn (odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>);

pub struct PythonArchEvalFileHook {
    pub odoo_entry: bool,
    pub file_tree: Vec<OYarn>,
    pub content_tree: Vec<OYarn>, //if set, will provide symbol in file content instead of the file symbol to func
    pub if_exist_only: bool,
    pub func: PythonArchEvalHookFile
}

static arch_eval_file_hooks: Lazy<Vec<PythonArchEvalFileHook>> = Lazy::new(|| {vec![
    PythonArchEvalFileHook {odoo_entry: true,
                        file_tree: vec![Sy!("odoo"), Sy!("models")],
                        content_tree: vec![Sy!("BaseModel"), Sy!("env")],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let env_file = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("api")], vec![]), u32::MAX);
        let env_class = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment")]), u32::MAX);
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
                        file_tree: vec![Sy!("odoo"), Sy!("models")],
                        content_tree: vec![Sy!("BaseModel"), Sy!("ids")],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let values: Vec<ruff_python_ast::Expr> = Vec::new();
        let mut id = symbol.borrow_mut();
        let range = id.range().clone();
        id.set_evaluations(vec![Evaluation::new_list(odoo, values, range)]);
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
                            file_tree: vec![Sy!("odoo"), Sy!("api")],
                            content_tree: vec![Sy!("Environment"), Sy!("registry")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let registry_sym = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("modules"), Sy!("registry")], vec![Sy!("Registry")]), u32::MAX);
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
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Boolean")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("bool")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Integer")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("int")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Float")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("float")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Monetary")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("float")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Char")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Text")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Html")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("markupsafe")], vec![Sy!("Markup")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Date")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("datetime")], vec![Sy!("date")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Datetime")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("datetime")], vec![Sy!("datetime")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Binary")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("bytes")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Image")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("bytes")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Selection")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Reference")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("str")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Json")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("object")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Properties")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("object")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("PropertiesDefinition")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("object")]));
        PythonArchEvalHooks::_update_field_init(symbol.clone(), false);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Many2one")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_relational(symbol.clone());
        PythonArchEvalHooks::_update_field_init(symbol.clone(), true);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("One2many")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
                PythonArchEvalHooks::_update_field_init(symbol.clone(), true);
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![Sy!("odoo"), Sy!("fields")],
                            content_tree: vec![Sy!("Many2many")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_relational(symbol.clone());
        PythonArchEvalHooks::_update_field_init(symbol.clone(), true);
    }},
]});

type PythonArchEvalHookFunc = fn (odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>);

pub struct PythonArchEvalFunctionHook {
    pub odoo_entry: bool,
    pub tree: Tree,
    pub if_exist_only: bool,
    pub func: PythonArchEvalHookFunc
}

static arch_eval_function_hooks: Lazy<Vec<PythonArchEvalFunctionHook>> = Lazy::new(|| {vec![
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("api")], vec![Sy!("Environment"), Sy!("__getitem__")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(Weak::new(),
                Some(true),
                HashMap::new(),
                Some(PythonArchEvalHooks::eval_env_get_item)
            ),
            value: None,
            range: None
        }]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("modules"), Sy!("registry")], vec![Sy!("Registry"), Sy!("__getitem__")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
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
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("__iter__")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_env")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("sudo")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("create")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("search")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut search: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        let func = search.as_func_mut();
        if func.args.len() > 1 {
            if let Some(arg_symbol) = func.args.get(1).unwrap().symbol.upgrade() {
                if arg_symbol.borrow().name().eq(&Sy!("domain")) {
                    arg_symbol.borrow_mut().set_evaluations(vec![Evaluation::new_domain(odoo)]);
                } else {
                    println!("domain not found on search signature")
                }
            }
        }
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("browse")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_company")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_context")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_prefetch")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_user")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("with_env")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel"), Sy!("exists")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation::new_self()]);
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                            tree: (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Id"), Sy!("__get__")]), //We have to put it at function level hook to remove evaluation from existing code
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_func_level(odoo, &entry_point, symbol.clone(), (vec![Sy!("builtins")], vec![Sy!("int")]));
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                            tree: (vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("One2many"), Sy!("__get__")]),
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_func_relational(symbol.clone());
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
            if name.eq(hook.file_tree.last().unwrap()) {
                if (hook.odoo_entry && session.sync_odoo.has_main_entry && odoo_tree.0 == hook.file_tree) || (!hook.odoo_entry && tree.0 == hook.file_tree) {
                    if hook.content_tree.is_empty() {
                        (hook.func)(session.sync_odoo, entry_point, symbol.clone(), symbol.clone());
                    } else {
                        let sub_symbol = symbol.borrow().get_symbol(&(vec![], hook.content_tree.clone()), u32::MAX);
                        if !sub_symbol.is_empty() {
                            (hook.func)(session.sync_odoo, entry_point, symbol.clone(), sub_symbol.last().unwrap().clone());
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
            if name.eq(hook.tree.1.last().unwrap()) {
                if (hook.odoo_entry && session.sync_odoo.has_main_entry && odoo_tree == hook.tree) || (!hook.odoo_entry && tree == hook.tree) {
                    (hook.func)(session.sync_odoo, entry_point, symbol.clone());
                }
            }
        }
    }

    pub fn eval_env_get_item(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>
    {
        if let Some(context) = context {
            let arg = context.get(&S!("args"));
            if let Some(arg) = arg {
                match arg {
                    ContextValue::STRING(s) => {
                        let model = session.sync_odoo.models.get(&oyarn!("{}", s));
                        if let Some(model) = model {
                            let module = context.get(&S!("module"));
                            let from_module;
                            if let Some(ContextValue::MODULE(m)) = module {
                                if let Some(m) = m.upgrade() {
                                    from_module = Some(m.clone());
                                } else {
                                    from_module = None;
                                }
                            } else {
                                from_module = None;
                            }
                            if let Some(scope) = scope {
                                let mut f = scope.borrow_mut();
                                f.add_model_dependencies(model);
                            }
                            let model = model.clone();
                            let model = model.borrow();
                            let symbols = model.get_main_symbols(session, from_module.clone());
                            if !symbols.is_empty() {
                                for s in symbols.iter() {
                                    if from_module.is_none() || ModuleSymbol::is_in_deps(session, &from_module.as_ref().unwrap(),&s.borrow().find_module().unwrap().borrow().as_module_package().dir_name) {
                                        return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(Rc::downgrade(s), Some(true), false)));
                                    }
                                }
                            } else {
                                if from_module.is_some() {
                                    //retry without from_module to see if model exists elsewhere
                                    let symbols = model.get_main_symbols(session, None);
                                    if symbols.is_empty() {
                                        let range = FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range());
                                        add_diagnostic(diagnostics, Diagnostic::new(range,
                                            Some(DiagnosticSeverity::ERROR),
                                            Some(NumberOrString::String(S!("OLS30105"))),
                                            Some(EXTENSION_NAME.to_string()),
                                            S!("This model is inherited, but never declared."),
                                            None,
                                            None
                                            )
                                        , &session.current_noqa);
                                    } else {
                                        let range = FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range());
                                        let valid_modules: Vec<OYarn> = symbols.iter().map(|s| match s.borrow().find_module() {
                                            Some(sym) => sym.borrow().name().clone(),
                                            None => Sy!("Unknown").clone()
                                        }).collect();
                                        add_diagnostic(diagnostics, Diagnostic::new(range,
                                            Some(DiagnosticSeverity::ERROR),
                                            Some(NumberOrString::String(S!("OLS30101"))),
                                            Some(EXTENSION_NAME.to_string()),
                                            format!("This model is not declared in the dependencies of your module. You should consider adding one of the following dependency: {:?}", valid_modules),
                                            None,
                                            None
                                            )
                                        , &session.current_noqa);
                                    }
                                } else {
                                    let range = FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range());
                                    add_diagnostic(diagnostics, Diagnostic::new(range,
                                        Some(DiagnosticSeverity::ERROR),
                                        Some(NumberOrString::String(S!("OLS30102"))),
                                        Some(EXTENSION_NAME.to_string()),
                                        S!("Unknown model. Check your addons path"),
                                        None,
                                        None
                                    ), &session.current_noqa);
                                }
                            }
                        } else {
                            let range = FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range());
                            add_diagnostic(diagnostics, Diagnostic::new(range,
                                Some(DiagnosticSeverity::ERROR),
                                Some(NumberOrString::String(S!("OLS30102"))),
                                None,
                                S!("Unknown model. Check your addons path"),
                                None,
                                None
                            ), &session.current_noqa);
                        }
                    }
                    _ => {
                        //NOT A STRING
                    }
                }
            }
        }
        Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(Weak::new(), Some(true), false)))
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

    fn eval_get(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>
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
        let get_sym = symbol.borrow().get_symbol(&(vec![], vec![Sy!("__get__")]), u32::MAX);
        if get_sym.is_empty() {
            return;
        }
        let return_sym = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &tree, u32::MAX);
        if return_sym.is_empty() {
            let file = symbol.borrow().get_file().clone();
            file.as_ref().unwrap().upgrade().unwrap().borrow_mut().not_found_paths_mut().push((BuildSteps::ARCH_EVAL, flatten_tree(&tree)));
            entry_point.borrow_mut().not_found_symbols.insert(symbol);
            return;
        }
        get_sym.last().unwrap().borrow_mut().set_evaluations(vec![Evaluation {
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

    fn eval_relational_with_comodel(session: &mut SessionInfo, comodel: &ContextValue, context: &Context) -> Option<EvaluationSymbolPtr>{
        let comodel = oyarn!("{}", comodel.as_string());
        let comodel_sym = session.sync_odoo.models.get(&comodel).cloned();
        if let Some(comodel_sym) = comodel_sym {
            let module = context.get(&S!("module"));
            let mut from_module = None;
            if let Some(ContextValue::MODULE(m)) = module {
                if let Some(m) = m.upgrade() {
                    from_module = Some(m.clone());
                }
            }
            let main_symbol = comodel_sym.borrow().get_main_symbols(session, from_module);
            if main_symbol.len() == 1 {
                return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: Rc::downgrade(&main_symbol[0]), context: HashMap::from([(S!("comodel"), ContextValue::SYMBOL(Rc::downgrade(&main_symbol[0])))]), instance: Some(true), is_super: false}))
            }
        }
        Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: Weak::new(), context: HashMap::new(), instance: Some(true), is_super: false}))
    }

    fn eval_relational(session: &mut SessionInfo, _evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, _diagnostics: &mut Vec<Diagnostic>, _scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>
    {
        let Some(context) = context else {
            return None;
        };
        if let Some(comodel) = context.get(&S!("comodel")) {
            return PythonArchEvalHooks::eval_relational_with_comodel(session, comodel, context);
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

    fn find_special_arguments<'a>(parameters: &'a Arguments, find_comdel_name: bool) -> Option<(&'a Expr, String, TextRange)> {
        let find_in_kwargs = |arg_name: &str, context_name: String| parameters.keywords.iter().find_map(|keyword| {
            keyword.arg
                .as_ref().filter(|kw_arg| kw_arg.id == arg_name)
                .map(|_| (&keyword.value, context_name.clone(), keyword.range()))
        });

        if find_comdel_name {
            if let Some(first_param) = parameters.args.get(0) {
                return Some((first_param, S!("comodel"), first_param.range()))
            }
        }
        find_in_kwargs("comodel_name", S!("comodel")).or_else(|| find_in_kwargs("related", S!("related")))
    }

    fn find_special_method_arguments(
        session: &mut SessionInfo,
        parameters: &Arguments,
        parent: Rc<RefCell<Symbol>>,
    ) -> Context{
        let mut context = HashMap::new();
        for kw_arg in parameters.keywords.iter(){
            let Some(kw_arg_name) = kw_arg.arg.as_ref().map(|kw_id| kw_id.id.as_str()) else {
                continue;
            };
            if !["compute", "inverse", "search"].contains(&kw_arg_name){
                continue;
            }
            let maybe_related_string = Evaluation::expr_to_str(session, &kw_arg.value, parent.clone(), &parameters.range.start(), &mut vec![]).0;
            let Some(related_string) = maybe_related_string else {
                continue;
            };
            context.insert(S!(kw_arg_name), ContextValue::STRING(related_string));
            context.insert(format!("{kw_arg_name}_range"), ContextValue::RANGE(kw_arg.range()));
        }
        context
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
        let mut context = PythonArchEvalHooks::find_special_method_arguments(session, &parameters, parent.clone());

        if let Some((field_name_expr, context_name, arg_range)) = PythonArchEvalHooks::find_special_arguments(&parameters, relational){
            let maybe_related_string = Evaluation::expr_to_str(session, &field_name_expr, parent.clone(), &parameters.range.start(), &mut vec![]).0;
            if let Some(related_string) = maybe_related_string {
                context.extend([
                    (context_name, ContextValue::STRING(related_string.to_string())),
                    (S!("field_parent"), ContextValue::SYMBOL(Rc::downgrade(&parent))),
                    (S!("special_arg_range"), ContextValue::RANGE(arg_range)),
                ]);
            }
        }
        if !context.is_empty(){
            return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak {
                weak: evaluation_sym.get_weak().weak.clone(),
                context,
                instance: Some(true),
                is_super: false
            }));
        }
        None
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
}
