use std::collections::HashMap;
use std::rc::Rc;
use std::rc::Weak;
use std::cell::RefCell;
use lsp_types::Diagnostic;
use lsp_types::DiagnosticSeverity;
use lsp_types::NumberOrString;
use crate::core::odoo::SyncOdoo;
use crate::core::evaluation::Context;
use crate::core::symbols::symbol::Symbol;
use crate::constants::*;
use crate::threads::SessionInfo;
use crate::S;

use super::evaluation::Evaluation;
use super::evaluation::ContextValue;
use super::evaluation::EvaluationSymbol;
use super::file_mgr::FileMgr;
use super::symbols::module_symbol::ModuleSymbol;

pub struct PythonArchEvalHooks {}

impl PythonArchEvalHooks {

    pub fn on_file_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        let tree = symbol.borrow().get_tree();
        let name = symbol.borrow().name().clone();
        match name.as_str() {
            "models" => {
                if tree == (vec![S!("odoo"), S!("models")], vec!()) {
                    let base_model = symbol.borrow().get_symbol(&(vec![], vec![S!("BaseModel")]), u32::MAX);
                    if base_model.len() > 0 {
                        PythonArchEvalHooks::on_base_model_eval(odoo, base_model.last().unwrap().clone(), symbol)
                    }
                }
            },
            "api" => {
                if tree == (vec![S!("odoo"), S!("api")], vec!()) {
                    let env = symbol.borrow().get_symbol(&(vec![], vec![S!("Environment")]), u32::MAX);
                    if env.len() > 0 {
                        PythonArchEvalHooks::on_env_eval(odoo, env.last().unwrap().clone(), symbol)
                    }
                }
            },
            "common" => {
                if tree == (vec![S!("odoo"), S!("tests"), S!("common")], vec!()) {
                    let form = symbol.borrow().get_symbol(&(vec![], vec![S!("Form")]), u32::MAX);
                    if form.len() > 0 {
                        PythonArchEvalHooks::on_form_eval(odoo, form.last().unwrap().clone())
                    }
                    let transaction_class = symbol.borrow().get_symbol(&(vec![], vec![S!("TransactionCase")]), u32::MAX);
                    if transaction_class.len() > 0 {
                        PythonArchEvalHooks::on_transaction_class_eval(odoo, transaction_class.last().unwrap().clone(), symbol);
                    }
                }
            },
            "fields" => {
                if tree == (vec![S!("odoo"), S!("fields")], vec!()) {
                    let boolean = symbol.borrow().get_symbol(&(vec![], vec![S!("Boolean")]), u32::MAX);
                    if boolean.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, boolean.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("bool")]));
                    }
                    let integer = symbol.borrow().get_symbol(&(vec![], vec![S!("Integer")]), u32::MAX);
                    if integer.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, integer.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("int")]));
                    }
                    let float = symbol.borrow().get_symbol(&(vec![], vec![S!("Float")]), u32::MAX);
                    if float.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, float.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("float")]));
                    }
                    let monetary = symbol.borrow().get_symbol(&(vec![], vec![S!("Monetary")]), u32::MAX);
                    if monetary.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, monetary.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("float")]));
                    }
                    let char = symbol.borrow().get_symbol(&(vec![], vec![S!("Char")]), u32::MAX);
                    if char.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, char.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("str")]));
                    }
                    let text = symbol.borrow().get_symbol(&(vec![], vec![S!("Text")]), u32::MAX);
                    if text.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, text.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("str")]));
                    }
                    let html = symbol.borrow().get_symbol(&(vec![], vec![S!("Html")]), u32::MAX);
                    if html.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, html.last().unwrap().clone(), (vec![S!("markupsafe")], vec![S!("Markup")]));
                    }
                    let date = symbol.borrow().get_symbol(&(vec![], vec![S!("Date")]), u32::MAX);
                    if date.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, date.last().unwrap().clone(), (vec![S!("datetime")], vec![S!("date")]));
                    }
                    let datetime = symbol.borrow().get_symbol(&(vec![], vec![S!("Datetime")]), u32::MAX);
                    if datetime.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, datetime.last().unwrap().clone(), (vec![S!("datetime")], vec![S!("datetime")]));
                    }
                    let binary = symbol.borrow().get_symbol(&(vec![], vec![S!("Binary")]), u32::MAX);
                    if binary.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, binary.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("bytes")]));
                    }
                    let image = symbol.borrow().get_symbol(&(vec![], vec![S!("Image")]), u32::MAX);
                    if image.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, image.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("bytes")]));
                    }
                    let selection = symbol.borrow().get_symbol(&(vec![], vec![S!("Selection")]), u32::MAX);
                    if selection.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, selection.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("str")]));
                    }
                    let reference = symbol.borrow().get_symbol(&(vec![], vec![S!("Reference")]), u32::MAX);
                    if reference.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, reference.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("str")]));
                    }
                    let json_sym = symbol.borrow().get_symbol(&(vec![], vec![S!("Json")]), u32::MAX);
                    if json_sym.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, json_sym.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("object")]));
                    }
                    let properties = symbol.borrow().get_symbol(&(vec![], vec![S!("Properties")]), u32::MAX);
                    if properties.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, properties.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("object")]));
                    }
                    let properties_def = symbol.borrow().get_symbol(&(vec![], vec![S!("PropertiesDefinition")]), u32::MAX);
                    if properties_def.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, properties_def.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("object")]));
                    }
                    let id = symbol.borrow().get_symbol(&(vec![], vec![S!("Id")]), u32::MAX);
                    if id.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval(odoo, id.last().unwrap().clone(), (vec![S!("builtins")], vec![S!("int")]));
                    }
                    let many2one = symbol.borrow().get_symbol(&(vec![], vec![S!("Many2one")]), u32::MAX);
                    if many2one.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval_relational(many2one.last().unwrap().clone());
                    }
                    let one2many = symbol.borrow().get_symbol(&(vec![], vec![S!("One2many")]), u32::MAX);
                    if one2many.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval_relational(one2many.last().unwrap().clone());
                    }
                    let many2many = symbol.borrow().get_symbol(&(vec![], vec![S!("Many2many")]), u32::MAX);
                    if many2many.len() > 0 {
                        PythonArchEvalHooks::_update_get_eval_relational(many2many.last().unwrap().clone());
                    }
                }
            }
            _ => {}
        }
    }

    fn on_base_model_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>, file_symbol: Rc<RefCell<Symbol>>) {
        let symbol = symbol.borrow();
        // ----------- __iter__ ------------
        let mut iter = symbol.get_symbol(&(vec![], vec![S!("__iter__")]), u32::MAX);
        if !iter.is_empty() {
            let mut iter = iter.last().unwrap().borrow_mut();
            iter.evaluations_mut().unwrap().clear();
            iter.evaluations_mut().unwrap().push(Evaluation {
                symbol: EvaluationSymbol::new_self(HashMap::new(), None, None),
                range: None,
                value: None
            });
        }
        // ----------- env ------------
        let env = symbol.get_symbol(&(vec![], vec![S!("env")]), u32::MAX);
        let env_file = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![]), u32::MAX);
        let env_class = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![S!("Environment")]), u32::MAX);
        if !env.is_empty() && !env_class.is_empty() {
            let env_rc = env.last().unwrap();
            let mut env = env_rc.borrow_mut();
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
        // ------------ ids ------------
        let ids = symbol.get_symbol(&(vec![], vec![S!("ids")]), u32::MAX);
        if !ids.is_empty() {
            let values: Vec<ruff_python_ast::Expr> = Vec::new();
            let mut last_id = ids.last().unwrap().borrow_mut();
            let range = last_id.range().clone();
            last_id.set_evaluations(vec![Evaluation::new_list(odoo, values, range.clone())]);
        }
        // ------------ sudo ------------
        let mut sudo = symbol.get_symbol(&(vec![], vec![S!("sudo")]), u32::MAX);
        if !sudo.is_empty()  {
            let mut sudo = sudo.last().unwrap().borrow_mut();
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
        }
        // ------------ create ------------
        let mut create = symbol.get_symbol(&(vec![], vec![S!("create")]), u32::MAX);
        if !create.is_empty() {
            let mut create = create.last().unwrap().borrow_mut();
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
        }
        // ------------ search ------------
        let mut search = symbol.get_symbol(&(vec![], vec![S!("search")]), u32::MAX);
        if !search.is_empty() {
            let mut search: std::cell::RefMut<Symbol> = search.last().unwrap().borrow_mut();
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
        }
    }

    fn eval_get_item(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (Weak<RefCell<Symbol>>, bool)
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
                            let symbols = model.clone().borrow().get_main_symbols(session, from_module.clone(), &mut None);
                            if symbols.len() > 0 {
                                for s in symbols.iter() {
                                    if from_module.is_none() || ModuleSymbol::is_in_deps(session, &from_module.as_ref().unwrap(),&s.borrow().find_module().unwrap().borrow().as_module_package().dir_name, &mut None) {
                                        return (Rc::downgrade(s), true);
                                    }
                                }
                                //still here? If from module is set, dependencies are not met
                                if from_module.is_some() {
                                    let range = FileMgr::textRange_to_temporary_Range(&context.get(&S!("range")).unwrap().as_text_range());
                                    diagnostics.push(Diagnostic::new(range,
                                        Some(DiagnosticSeverity::ERROR),
                                        Some(NumberOrString::String(S!("OLS30101"))),
                                        None,
                                        S!("This model is not in the dependencies of your module."),
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
                                    None,
                                    S!("Unknown model. Check your addons path"),
                                    None,
                                    None
                                ));
                            }
                        }
                    }
                    _ => {
                        //NOT A STRING
                    }
                }
            }
        }
        (Weak::new(), false)
    }

    fn eval_test_cursor(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (Weak<RefCell<Symbol>>, bool)
    {
        if context.is_some() && context.as_ref().unwrap().get(&S!("test_mode")).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool() {
            let test_cursor_sym = session.sync_odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![S!("TestCursor")]), u32::MAX);
            if test_cursor_sym.len() > 0 {
                    return (Rc::downgrade(test_cursor_sym.last().unwrap()), true);
            } else {
                    return evaluation_sym.get_symbol(session, &mut None, diagnostics);
            }
        }
        (evaluation_sym.get_weak().weak.clone() , evaluation_sym.get_weak().instance)
    }

    fn on_env_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>, file_symbol: Rc<RefCell<Symbol>>) {
        let mut get_item = symbol.borrow().get_symbol(&(vec![], vec![S!("__getitem__")]), u32::MAX);
        if !get_item.is_empty() {
            let mut get_item = get_item.last().unwrap().borrow_mut();
            get_item.set_evaluations(vec![Evaluation {
                symbol: EvaluationSymbol::new_with_symbol(Weak::new(),
                    true,
                    HashMap::new(),
                    None,
                    Some(PythonArchEvalHooks::eval_get_item)
                ),
                value: None,
                range: None
            }]);
        }
        let cr = symbol.borrow().get_symbol(&(vec![], vec![S!("cr")]), u32::MAX);
        let cursor_file = odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![]), u32::MAX);
        let cursor_sym = odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![S!("Cursor")]), u32::MAX);
        if !cursor_sym.is_empty() && !cr.is_empty() {
            let cr_mut = cr.last().unwrap();
            cr_mut.borrow_mut().set_evaluations(vec![Evaluation {
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
    }

    fn on_form_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        if odoo.full_version < S!("16.3") {
            return;
        }
        let file = symbol.borrow().get_file();
        todo!()
    }

    fn on_transaction_class_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>, file_symbol: Rc<RefCell<Symbol>>) {
        let env_file = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![]), u32::MAX);
        let env_model = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![S!("Environment")]), u32::MAX);
        let env_var = symbol.borrow().get_symbol(&(vec![], vec![S!("env")]), u32::MAX);
        if !env_model.is_empty() && !env_var.is_empty() {
            let env_model = env_model.last().unwrap();
            let env_var = env_var.last().unwrap();
            let mut context = HashMap::new();
            context.insert(S!("test_mode"), ContextValue::BOOLEAN(true));
            env_var.borrow_mut().set_evaluations(vec![Evaluation {
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
    }

    fn eval_get(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (Weak<RefCell<Symbol>>, bool)
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
        (evaluation_sym.get_weak().weak.clone() , evaluation_sym.get_weak().instance)
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

    fn eval_relational(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (Weak<RefCell<Symbol>>, bool)
    {
        if context.is_none() {
            return evaluation_sym.get_symbol(session, &mut None, diagnostics);
        }
        let comodel = context.as_ref().unwrap().get(&S!("comodel"));
        if comodel.is_none() {
            return evaluation_sym.get_symbol(session, &mut None, diagnostics);
        }
        let comodel = comodel.unwrap().as_string();
        //TODO let comodel_sym = odoo.models.get(comodel);
       (Weak::new(), false)
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