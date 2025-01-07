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

use super::evaluation::Evaluation;
use super::evaluation::ContextValue;
use super::evaluation::EvaluationSymbol;
use super::evaluation::EvaluationSymbolWeak;
use super::file_mgr::FileMgr;
use super::symbols::module_symbol::ModuleSymbol;

type PythonArchEvalHookFile = fn (odoo: &mut SyncOdoo, file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>);

pub struct PythonArchEvalFileHook {
    pub file_tree: Vec<String>,
    pub content_tree: Vec<String>, //if set, will provide symbol in file content instead of the file symbol to func
    pub if_exist_only: bool,
    pub func: PythonArchEvalHookFile
}

static arch_eval_file_hooks: Lazy<Vec<PythonArchEvalFileHook>> = Lazy::new(|| {vec![
    PythonArchEvalFileHook { file_tree: vec![S!("odoo"), S!("models")],
                        content_tree: vec![S!("BaseModel"), S!("env")],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let env_file = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![]), u32::MAX);
        let env_class = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![S!("Environment")]), u32::MAX);
        if !env_class.is_empty() {
            let mut env = symbol.borrow_mut();
            let env_class = env_class.last().unwrap();
            let mut context = HashMap::new();
            context.insert(S!("test_mode"), super::evaluation::ContextValue::BOOLEAN(true));
            env.set_evaluations(vec![Evaluation {
                symbol: EvaluationSymbol::new_with_symbol(
                    Rc::downgrade(env_class),
                    true,
                    context,
                    None,
                    None,
                ),
                value: None,
                range: None,
            }]);
            file_symbol.borrow_mut().add_dependency(&mut env_file.last().unwrap().borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
            env.set_doc_string(Some(S!("")));
        }
    }},
    PythonArchEvalFileHook { file_tree: vec![S!("odoo"), S!("models")],
                        content_tree: vec![S!("BaseModel"), S!("ids")],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let values: Vec<ruff_python_ast::Expr> = Vec::new();
        let mut id = symbol.borrow_mut();
        let range = id.range().clone();
        id.set_evaluations(vec![Evaluation::new_list(odoo, values, range)]);
    }},
    /*PythonArchEvalFileHook { file_tree: vec![S!("odoo"), S!("models")],
                        content_tree: vec![S!("BaseModel"), S!("search_count")],
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let values: Vec<ruff_python_ast::Expr> = Vec::new();
        let mut id = symbol.borrow_mut();
        let range = id.range().clone();
        id.set_evaluations(vec![Evaluation::eval_from_symbol(odoo, values, range.clone())]);
    }},*/
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("api")],
                            content_tree: vec![S!("Environment"), S!("cr")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let cursor_file = odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![]), u32::MAX);
        let cursor_sym = odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![S!("Cursor")]), u32::MAX);
        if !cursor_sym.is_empty() {
            symbol.borrow_mut().set_evaluations(vec![Evaluation {
                symbol: EvaluationSymbol::new_with_symbol(
                    Rc::downgrade(cursor_sym.last().unwrap()),
                    true,
                    HashMap::new(),
                    None,
                    Some(PythonArchEvalHooks::eval_test_cursor)
                ),
                value: None,
                range: None,
            }]);
            file_symbol.borrow_mut().add_dependency(&mut cursor_file.last().unwrap().borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
        }
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("tests"), S!("common")],
                            content_tree: vec![S!("TransactionCase"), S!("env")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        let env_file = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![]), u32::MAX);
        let env_model = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![S!("Environment")]), u32::MAX);
        if !env_model.is_empty() {
            let env_model = env_model.last().unwrap();
            let mut context = HashMap::new();
            context.insert(S!("test_mode"), ContextValue::BOOLEAN(true));
            symbol.borrow_mut().set_evaluations(vec![Evaluation {
                symbol: EvaluationSymbol::new_with_symbol(
                    Rc::downgrade(env_model),
                    true,
                    context,
                    None,
                    None
                ),
                value: None,
                range: None,
            }]);
            file_symbol.borrow_mut().add_dependency(&mut env_file.last().unwrap().borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
        }
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Boolean")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("bool")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Integer")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("int")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Float")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("float")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Monetary")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("float")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Char")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("str")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Text")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("str")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Html")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("markupsafe")], vec![S!("Markup")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Date")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("datetime")], vec![S!("date")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Datetime")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("datetime")], vec![S!("datetime")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Binary")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("bytes")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Image")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("bytes")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Selection")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("str")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Reference")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("str")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Json")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("object")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Properties")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("object")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("PropertiesDefinition")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("object")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Id")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval(odoo, symbol.clone(), (vec![S!("builtins")], vec![S!("int")]));
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Many2one")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_relational(symbol.clone());
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("One2many")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_relational(symbol.clone());
    }},
    PythonArchEvalFileHook {file_tree: vec![S!("odoo"), S!("fields")],
                            content_tree: vec![S!("Many2many")],
                            if_exist_only: true,
                            func: |odoo: &mut SyncOdoo, _file_symbol: Rc<RefCell<Symbol>>, symbol: Rc<RefCell<Symbol>>| {
        PythonArchEvalHooks::_update_get_eval_relational(symbol.clone());
    }},
]});

type PythonArchEvalHookFunc = fn (odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>);

pub struct PythonArchEvalFunctionHook {
    pub tree: Tree,
    pub if_exist_only: bool,
    pub func: PythonArchEvalHookFunc
}

static arch_eval_function_hooks: Lazy<Vec<PythonArchEvalFunctionHook>> = Lazy::new(|| {vec![
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("api")], vec![S!("Environment"), S!("__getitem__")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(Weak::new(),
                true,
                HashMap::new(),
                None,
                Some(PythonArchEvalHooks::eval_env_get_item)
            ),
            value: None,
            range: None
        }]);
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("modules"), S!("registry")], vec![S!("Registry"), S!("__getitem__")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(Weak::new(),
                true,
                HashMap::new(),
                None,
                Some(PythonArchEvalHooks::eval_registry_get_item)
            ),
            value: None,
            range: None
        }]);
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("__iter__")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        symbol.borrow_mut().evaluations_mut().unwrap().clear();
        symbol.borrow_mut().evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(HashMap::new(), None, None),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_env")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut with_env = symbol.borrow_mut();
        with_env.evaluations_mut().unwrap().clear();
        with_env.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("sudo")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut sudo = symbol.borrow_mut();
        sudo.evaluations_mut().unwrap().clear();
        sudo.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("create")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut create = symbol.borrow_mut();
        create.evaluations_mut().unwrap().clear();
        create.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("search")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut search: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        search.evaluations_mut().unwrap().clear();
        search.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
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
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("browse")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_company")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_context")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_prefetch")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_user")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("with_env")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
                None,
            ),
            range: None,
            value: None
        });
    }},
    PythonArchEvalFunctionHook { tree: (vec![S!("odoo"), S!("models")], vec![S!("BaseModel"), S!("exists")]),
                        if_exist_only: true,
                        func: |odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>| {
        let mut browse: std::cell::RefMut<Symbol> = symbol.borrow_mut();
        browse.evaluations_mut().unwrap().clear();
        browse.evaluations_mut().unwrap().push(Evaluation {
            symbol: EvaluationSymbol::new_self(
                HashMap::new(),
                None,
                None,
            ),
            range: None,
            value: None
        });
    }},
]});

pub struct PythonArchEvalHooks {
}

impl PythonArchEvalHooks {

    pub fn on_file_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        let tree = symbol.borrow().get_tree();
        let name = symbol.borrow().name().clone();
        for hook in arch_eval_file_hooks.iter() {
            if name.eq(hook.file_tree.last().unwrap()) {
                if tree.0 == hook.file_tree {
                    if hook.content_tree.is_empty() {
                        (hook.func)(odoo, symbol.clone(), symbol.clone());
                    } else {
                        let sub_symbol = symbol.borrow().get_symbol(&(vec![], hook.content_tree.clone()), u32::MAX);
                        if !sub_symbol.is_empty() {
                            (hook.func)(odoo, symbol.clone(), sub_symbol.last().unwrap().clone());
                        }
                    }
                }
            }
        }
    }

    pub fn on_function_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        let tree = symbol.borrow().get_tree();
        let name = symbol.borrow().name().clone();
        for hook in arch_eval_function_hooks.iter() {
            if name.eq(hook.tree.1.last().unwrap()) {
                if tree == hook.tree {
                    (hook.func)(odoo, symbol.clone());
                }
            }
        }
    }

    pub fn eval_env_get_item(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> EvaluationSymbolWeak
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
                                        return EvaluationSymbolWeak::new(Rc::downgrade(s), Some(true), false);
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
        EvaluationSymbolWeak::new(Weak::new(), Some(true), false)
    }

    pub fn eval_registry_get_item(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> EvaluationSymbolWeak
    {
        let mut result = PythonArchEvalHooks::eval_env_get_item(session, evaluation_sym, context, diagnostics, scope);
        result.instance = Some(false);
        result
    }

    fn eval_test_cursor(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> EvaluationSymbolWeak
    {
        if context.is_some() && context.as_ref().unwrap().get(&S!("test_mode")).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool() {
            let test_cursor_sym = session.sync_odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![S!("TestCursor")]), u32::MAX);
            if test_cursor_sym.len() > 0 {
                    return EvaluationSymbolWeak::new(Rc::downgrade(test_cursor_sym.last().unwrap()), Some(true), false);
            } else {
                    return evaluation_sym.get_symbol(session, &mut None, diagnostics, None);
            }
        }
        evaluation_sym.get_weak().clone()
    }

    fn eval_get(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> EvaluationSymbolWeak
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
        evaluation_sym.get_weak().clone()
    }

    fn _update_get_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>, tree: Tree) {
        let get_sym = symbol.borrow().get_symbol(&(vec![], vec![S!("__get__")]), u32::MAX);
        if get_sym.is_empty() {
            return;
        }
        let return_sym = odoo.get_symbol(&tree, u32::MAX);
        if return_sym.is_empty() {
            let file = symbol.borrow_mut().get_file().clone();
            file.as_ref().unwrap().upgrade().unwrap().borrow_mut().not_found_paths_mut().push((BuildSteps::ARCH_EVAL, flatten_tree(&tree)));
            odoo.not_found_symbols.insert(symbol);
            return;
        }
        get_sym.last().unwrap().borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Rc::downgrade(return_sym.last().unwrap()),
                true,
                HashMap::new(),
                None,
                Some(PythonArchEvalHooks::eval_get)
            ),
            value: None,
            range: None
        }]);
    }

    fn eval_relational(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> EvaluationSymbolWeak
    {
        if context.is_none() {
            return evaluation_sym.get_symbol(session, &mut None, diagnostics, None);
        }
        let comodel = context.as_ref().unwrap().get(&S!("comodel"));
        if comodel.is_none() {
            return evaluation_sym.get_symbol(session, &mut None, diagnostics, None);
        }
        let comodel = comodel.unwrap().as_string();
        //TODO let comodel_sym = odoo.models.get(comodel);
        EvaluationSymbolWeak{weak: Weak::new(), instance: Some(false), is_super: false}
    }

    fn _update_get_eval_relational(symbol: Rc<RefCell<Symbol>>) {
        let get_sym = symbol.borrow().get_symbol(&(vec![], vec![S!("__get__")]), u32::MAX);
        if get_sym.is_empty() {
            return;
        }
        get_sym.last().unwrap().borrow_mut().set_evaluations(vec![Evaluation {
            symbol: EvaluationSymbol::new_with_symbol(
                Weak::new(),
                true,
                HashMap::new(),
                None,
                Some(PythonArchEvalHooks::eval_relational)
            ),
            value: None,
            range: None,
        }]);
    }
}
