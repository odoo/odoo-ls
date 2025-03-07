use std::collections::HashMap;
use std::rc::Rc;
use std::rc::Weak;
use std::cell::RefCell;
use lsp_types::Diagnostic;
use lsp_types::DiagnosticSeverity;
use lsp_types::NumberOrString;
use once_cell::sync::Lazy;
use crate::core::odoo::SyncOdoo;
use crate::core::evaluation::Context;
use crate::core::symbols::symbol::Symbol;
use crate::constants::*;
use crate::threads::SessionInfo;
use crate::S;

use super::entry_point::EntryPoint;
use super::evaluation::{ContextValue, Evaluation, EvaluationSymbolPtr, EvaluationSymbol, EvaluationSymbolWeak};
use super::file_mgr::FileMgr;
use super::symbols::module_symbol::ModuleSymbol;

type PythonArchEvalHookFile = fn (odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>);

pub struct PythonArchEvalFileHook {
    pub odoo_entry: bool,
    pub file_tree: Vec<String>,
    pub content_tree: Vec<String>, //if set, will provide symbol in file content instead of the file symbol to func
    pub if_exist_only: bool,
    pub func: PythonArchEvalHookFile
}

static arch_eval_file_hooks: Lazy<Vec<PythonArchEvalFileHook>> = Lazy::new(|| {vec![
    PythonArchEvalFileHook {odoo_entry: true,
                        file_tree: vec![S!("odoo"), S!("models")],
                        content_tree: vec![S!("BaseModel"), S!("env")],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let env_file = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &(vec![S!("odoo"), S!("api")], vec![]), u32::MAX);
        let env_class = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &(vec![S!("odoo"), S!("api")], vec![S!("Environment")]), u32::MAX);
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
                        file_tree: vec![S!("odoo"), S!("models")],
                        content_tree: vec![S!("BaseModel"), S!("ids")],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let values: Vec<ruff_python_ast::Expr> = Vec::new();
        let mut id = symbol.borrow_mut();
        let range = id.range().clone();
        id.set_evaluations(vec![Evaluation::new_list(odoo, values, range)]);
    }},
    /*PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("models")],
                        content_tree: vec![S!("BaseModel"), S!("search_count")],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let values: Vec<ruff_python_ast::Expr> = Vec::new();
        let mut id = symbol.borrow_mut();
        let range = id.range().clone();
        id.set_evaluations(vec![Evaluation::eval_from_symbol(odoo, values, range.clone())]);
    }},*/
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("api")],
                            content_tree: vec![S!("Environment"), S!("registry")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let registry_sym = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &(vec![S!("odoo"), S!("modules"), S!("registry")], vec![S!("Registry")]), u32::MAX);
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
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Boolean")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("bool")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Integer")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("int")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Float")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("float")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Monetary")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("float")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Char")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("str")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Text")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("str")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Html")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("markupsafe")], vec![S!("Markup")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Date")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("datetime")], vec![S!("date")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Datetime")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("datetime")], vec![S!("datetime")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Binary")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("bytes")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Image")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("bytes")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Selection")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("str")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Reference")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("str")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Json")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("object")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Properties")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("object")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("PropertiesDefinition")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, entry, symbol.clone(), (vec![S!("builtins")], vec![S!("object")]));
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Many2one")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_relational(symbol.clone());
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Many2many")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_relational(symbol.clone());
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Many2one")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_init_comodel_relational(symbol.clone());
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("One2many")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_init_comodel_relational(symbol.clone());
    }},
    PythonArchEvalFileHook {odoo_entry: true,
                            file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Many2many")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry: &Rc<RefCell<EntryPoint>>, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_init_comodel_relational(symbol.clone());
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
                        tree: (vec![S!("odoo"), S!("api")], vec![S!("Environment"), S!("__getitem__")]),
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
                        tree: (vec![S!("odoo"), S!("modules"), S!("registry")], vec![S!("Registry"), S!("__getitem__")]),
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
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("__iter__")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().evaluations_mut().unwrap().clear();
        symbol.borrow_mut().evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(None),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_env")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut with_env = symbol.borrow_mut();
        with_env.evaluations_mut().unwrap().clear();
        with_env.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("sudo")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut sudo = symbol.borrow_mut();
        sudo.evaluations_mut().unwrap().clear();
        sudo.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("create")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut create = symbol.borrow_mut();
        create.evaluations_mut().unwrap().clear();
        create.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("search")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut search: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        search.evaluations_mut().unwrap().clear();
        search.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
        let func = search.as_func_mut();
        if func.args.len() > 1 {
            if let Some(arg_symbol) = func.args.get(1).unwrap().symbol.upgrade() {
                if arg_symbol.borrow().name().eq(&S!("domain")) {
                    arg_symbol.borrow_mut().set_evaluations(vec![Evaluation::new_domain(odoo)]);
                } else {
                    println!("domain not found on search signature")
                }
            }
        }
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("browse")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_company")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_context")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_prefetch")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_user")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_env")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                        tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("exists")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                            tree: (vec![S!("odoo"), S!("fields")], vec![S!("Id"), S!("__get__")]), //We have to put it at function level hook to remove evaluation from existing code
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, entry_point: &Rc<RefCell<EntryPoint>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_func_level(odoo, &entry_point, symbol.clone(), (vec![S!("builtins")], vec![S!("int")]));
    }},
    PythonArchEvalFunctionHook {odoo_entry: true,
                            tree: (vec![S!("odoo"), S!("fields")], vec![S!("One2many"), S!("__get__")]),
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
                        let model = session.sync_odoo.models.get(s);
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
                                        diagnostics.push(Diagnostic::new(range,
                                            Some(DiagnosticSeverity::ERROR),
                                            Some(NumberOrString::String(S!("OLS30105"))),
                                            Some(EXTENSION_NAME.to_string()),
                                            S!("This model is inherited, but never declared."),
                                            None,
                                            None
                                            )
                                        );
                                    } else {
                                        let range = FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range());
                                        let valid_modules: Vec<String> = symbols.iter().map(|s| match s.borrow().find_module() {
                                            Some(sym) => sym.borrow().name().clone(),
                                            None => S!("Unknown").clone()
                                        }).collect();
                                        diagnostics.push(Diagnostic::new(range,
                                            Some(DiagnosticSeverity::ERROR),
                                            Some(NumberOrString::String(S!("OLS30101"))),
                                            Some(EXTENSION_NAME.to_string()),
                                            format!("This model is not declared in the dependencies of your module. You should consider adding one of the following dependency: {:?}", valid_modules),
                                            None,
                                            None
                                            )
                                        );
                                    }
                                } else {
                                    let range = FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range());
                                    diagnostics.push(Diagnostic::new(range,
                                        Some(DiagnosticSeverity::ERROR),
                                        Some(NumberOrString::String(S!("OLS30102"))),
                                        Some(EXTENSION_NAME.to_string()),
                                        S!("Unknown model. Check your addons path"),
                                        None,
                                        None
                                    ));
                                }
                            }
                        } else {
                            let range = FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range());
                            diagnostics.push(Diagnostic::new(range,
                                Some(DiagnosticSeverity::ERROR),
                                Some(NumberOrString::String(S!("OLS30102"))),
                                None,
                                S!("Unknown model. Check your addons path"),
                                None,
                                None
                            ));
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
        let get_sym = symbol.borrow().get_symbol(&(vec![], vec![S!("__get__")]), u32::MAX);
        if get_sym.is_empty() {
            return;
        }
        let return_sym = odoo.get_symbol(odoo.config.odoo_path.as_ref().unwrap(), &tree, u32::MAX);
        if return_sym.is_empty() {
            let file = symbol.borrow_mut().get_file().clone();
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

    fn eval_relational(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>
    {
        if context.is_none() {
            return None;
        }
        let comodel = context.as_ref().unwrap().get(&S!("comodel"));
        if comodel.is_none() {
            return None;
        }
        let comodel = comodel.unwrap().as_string();
        let comodel_sym = session.sync_odoo.models.get(&comodel).cloned();
        if let Some(comodel_sym) = comodel_sym {
            let module = context.as_ref().unwrap().get(&S!("module"));
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

    fn _update_get_eval_relational(symbol: Rc<RefCell<Symbol>>) {
        let get_sym = symbol.borrow().get_symbol(&(vec![], vec![S!("__get__")]), u32::MAX);
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

    fn eval_init_relational(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>
    {
        if context.is_none() {
            return None;
        }
        let parameters = context.as_ref().unwrap().get(&S!("parameters"));
        if parameters.is_none() {
            return None;
        }
        let parameters = parameters.unwrap().as_arguments();
        let comodel = if let Some(first_param) = parameters.args.get(0) {
            first_param
        } else {
            let mut comodel_name = None;
            for keyword in parameters.keywords {
                if keyword.arg.is_some() && keyword.arg.unwrap().id == "comodel_name" {
                    comodel_name = Some(keyword.value);
                }
            }
            if let Some(comodel_name) = comodel_name {
                &comodel_name.clone()
            } else {
                return None;
            }
        };
        let parent = Symbol::get_scope_symbol(file_symbol.unwrap().clone(), 
            context.as_ref().unwrap().get(&S!("range")).unwrap().as_text_range().start().to_u32(), 
            false);
        let comodel_string_evals = Evaluation::expr_to_str(session, comodel, parent, &parameters.range.start(), &mut vec![]);
        if let Some(comodel_string_value) = comodel_string_evals.0 {
            return Some(EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak {
                weak: evaluation_sym.get_weak().weak.clone(),
                context: HashMap::from([(S!("comodel"), ContextValue::STRING(comodel_string_value.to_string()))]),
                instance: Some(true),
                is_super: false
            }));
        }
        None
    }

    fn _update_init_comodel_relational(symbol: Rc<RefCell<Symbol>>) {
        let init_sym = symbol.borrow().get_symbol(&(vec![], vec![S!("__init__")]), u32::MAX);
        if init_sym.is_empty() {
            return;
        }
        init_sym.last().unwrap().borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Rc::downgrade(&symbol), //use the weak to keep reference to the class for the hook.
                Some(true),
                HashMap::new(),
                Some(PythonArchEvalHooks::eval_init_relational)
            ),
            value: None,
            range: None,
        }]);
    }
}
