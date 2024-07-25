use std::collections::HashMap;
use std::rc::Rc;
use std::rc::Weak;
use std::cell::RefCell;
use lsp_types::Diagnostic;
use lsp_types::DiagnosticSeverity;
use lsp_types::NumberOrString;
use crate::core::odoo::SyncOdoo;
use crate::core::evaluation::Context;
use crate::core::symbol::Symbol;
use crate::constants::*;
use crate::threads::SessionInfo;
use crate::S;

use super::evaluation::Evaluation;
use super::evaluation::ContextValue;
use super::evaluation::EvaluationSymbol;
use super::evaluation::SymbolRef;
use super::file_mgr::FileMgr;
use super::localized_symbol::LocalizedSymbol;
use super::symbols::module_symbol::ModuleSymbol;

pub struct PythonArchEvalHooks {}

impl PythonArchEvalHooks {

    pub fn on_file_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        let tree = symbol.borrow().get_tree();
        let name = symbol.borrow().name.clone();
        match name.as_str() {
            "models" => {
                if tree == (vec![S!("odoo"), S!("models")], vec!()) {
                    let base_model = symbol.borrow().get_symbol(&(vec![], vec![S!("BaseModel")]));
                    if base_model.is_some() {
                        PythonArchEvalHooks::on_base_model_eval(odoo, base_model.unwrap().borrow().last_loc_sym(), symbol)
                    }
                }
            },
            "api" => {
                if tree == (vec![S!("odoo"), S!("api")], vec!()) {
                    let env = symbol.borrow().get_symbol(&(vec![], vec![S!("Environment")]));
                    if env.is_some() {
                        PythonArchEvalHooks::on_env_eval(odoo, env.unwrap(), symbol)
                    }
                }
            },
            "common" => {
                if tree == (vec![S!("odoo"), S!("tests"), S!("common")], vec!()) {
                    let form = symbol.borrow().get_symbol(&(vec![], vec![S!("Form")]));
                    if form.is_some() {
                        PythonArchEvalHooks::on_form_eval(odoo, form.unwrap())
                    }
                    let transaction_class = symbol.borrow().get_symbol(&(vec![], vec![S!("TransactionCase")]));
                    if transaction_class.is_some() {
                        PythonArchEvalHooks::on_transaction_class_eval(odoo, transaction_class.unwrap(), symbol);
                    }
                }
            },
            "fields" => {
                if tree == (vec![S!("odoo"), S!("fields")], vec!()) {
                    let boolean = symbol.borrow().get_symbol(&(vec![], vec![S!("Boolean")]));
                    if boolean.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, boolean.unwrap(), (vec![S!("builtins")], vec![S!("bool")]));
                    }
                    let integer = symbol.borrow().get_symbol(&(vec![], vec![S!("Integer")]));
                    if integer.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, integer.unwrap(), (vec![S!("builtins")], vec![S!("int")]));
                    }
                    let float = symbol.borrow().get_symbol(&(vec![], vec![S!("Float")]));
                    if float.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, float.unwrap(), (vec![S!("builtins")], vec![S!("float")]));
                    }
                    let monetary = symbol.borrow().get_symbol(&(vec![], vec![S!("Monetary")]));
                    if monetary.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, monetary.unwrap(), (vec![S!("builtins")], vec![S!("float")]));
                    }
                    let char = symbol.borrow().get_symbol(&(vec![], vec![S!("Char")]));
                    if char.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, char.unwrap(), (vec![S!("builtins")], vec![S!("str")]));
                    }
                    let text = symbol.borrow().get_symbol(&(vec![], vec![S!("Text")]));
                    if text.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, text.unwrap(), (vec![S!("builtins")], vec![S!("str")]));
                    }
                    let html = symbol.borrow().get_symbol(&(vec![], vec![S!("Html")]));
                    if html.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, html.unwrap(), (vec![S!("markupsafe")], vec![S!("Markup")]));
                    }
                    let date = symbol.borrow().get_symbol(&(vec![], vec![S!("Date")]));
                    if date.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, date.unwrap(), (vec![S!("datetime")], vec![S!("date")]));
                    }
                    let datetime = symbol.borrow().get_symbol(&(vec![], vec![S!("Datetime")]));
                    if datetime.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, datetime.unwrap(), (vec![S!("datetime")], vec![S!("datetime")]));
                    }
                    let binary = symbol.borrow().get_symbol(&(vec![], vec![S!("Binary")]));
                    if binary.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, binary.unwrap(), (vec![S!("builtins")], vec![S!("bytes")]));
                    }
                    let image = symbol.borrow().get_symbol(&(vec![], vec![S!("Image")]));
                    if image.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, image.unwrap(), (vec![S!("builtins")], vec![S!("bytes")]));
                    }
                    let selection = symbol.borrow().get_symbol(&(vec![], vec![S!("Selection")]));
                    if selection.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, selection.unwrap(), (vec![S!("builtins")], vec![S!("str")]));
                    }
                    let reference = symbol.borrow().get_symbol(&(vec![], vec![S!("Reference")]));
                    if reference.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, reference.unwrap(), (vec![S!("builtins")], vec![S!("str")]));
                    }
                    let json_sym = symbol.borrow().get_symbol(&(vec![], vec![S!("Json")]));
                    if json_sym.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, json_sym.unwrap(), (vec![S!("builtins")], vec![S!("object")]));
                    }
                    let properties = symbol.borrow().get_symbol(&(vec![], vec![S!("Properties")]));
                    if properties.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, properties.unwrap(), (vec![S!("builtins")], vec![S!("object")]));
                    }
                    let properties_def = symbol.borrow().get_symbol(&(vec![], vec![S!("PropertiesDefinition")]));
                    if properties_def.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, properties_def.unwrap(), (vec![S!("builtins")], vec![S!("object")]));
                    }
                    let id = symbol.borrow().get_symbol(&(vec![], vec![S!("Id")]));
                    if id.is_some() {
                        PythonArchEvalHooks::_update_get_eval(odoo, id.unwrap(), (vec![S!("builtins")], vec![S!("int")]));
                    }
                    let many2one = symbol.borrow().get_symbol(&(vec![], vec![S!("Many2one")]));
                    if many2one.is_some() {
                        PythonArchEvalHooks::_update_get_eval_relational(many2one.unwrap());
                    }
                    let one2many = symbol.borrow().get_symbol(&(vec![], vec![S!("One2many")]));
                    if one2many.is_some() {
                        PythonArchEvalHooks::_update_get_eval_relational(one2many.unwrap());
                    }
                    let many2many = symbol.borrow().get_symbol(&(vec![], vec![S!("Many2many")]));
                    if many2many.is_some() {
                        PythonArchEvalHooks::_update_get_eval_relational(many2many.unwrap());
                    }
                }
            }
            _ => {}
        }
    }

    fn eval_get_take_parent(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (SymbolRef, bool)
    {
        todo!()
    }

    fn on_base_model_eval(odoo: &mut SyncOdoo, loc_sym: Rc<RefCell<LocalizedSymbol>>, file_symbol: Rc<RefCell<Symbol>>) {
        let symbol = loc_sym.borrow().symbol();
        let symbol = symbol.borrow();
        let loc_sym = loc_sym.borrow();
        // ----------- __iter__ ------------
        let mut iter = symbol.get_symbol(&(vec![], vec![S!("__iter__")]));
        if iter.is_some() && iter.as_ref().unwrap().borrow().last_loc_sym().borrow().evaluations.len() > 0 {
            let iter = iter.as_mut().unwrap().borrow_mut();
            let loc_iter = iter.last_loc_sym();
            let evaluation = &mut loc_iter.borrow_mut().evaluations[0];
            let eval_sym = &mut evaluation.symbol;
            eval_sym.get_symbol_hook = Some(PythonArchEvalHooks::eval_get_take_parent);
        }
        // ----------- env ------------
        let env = symbol.get_symbol(&(vec![], vec![S!("env")]));
        let env_file = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![]));
        let env_class = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![S!("Environment")]));
        if env.is_some() && env_class.is_some() {
            let env = env.unwrap();
            let env_class = env_class.unwrap();
            let env = env.borrow_mut();
            let loc_env = env.last_loc_sym();
            let mut loc_env = loc_env.borrow_mut();
            let mut context = HashMap::new();
            context.insert(S!("test_mode"), super::evaluation::ContextValue::BOOLEAN(true));
            loc_env.evaluations = vec![Evaluation {
                symbol: EvaluationSymbol::new(
                    loc_env.to_symbol_ref(),
                    true,
                    context,
                    None,
                    None,
                ),
                value: None,
                range: None,
            }];
            file_symbol.borrow_mut().add_dependency(&mut env_file.unwrap().borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
            loc_env.doc_string = Some(S!(""));
        }
        // ------------ ids ------------
        let ids = symbol.get_symbol(&(vec![], vec![S!("ids")]));
        if let Some(ids) = ids {
            let values: Vec<ruff_python_ast::Expr> = Vec::new();
            let range = ids.borrow().last_loc_sym().borrow().range;
            ids.borrow().last_loc_sym().borrow_mut().evaluations = vec![Evaluation::new_list(odoo, values, range)];
        }
        // ------------ sudo ------------
        let mut sudo = symbol.get_symbol(&(vec![], vec![S!("sudo")]));
        if sudo.is_some() && sudo.as_ref().unwrap().borrow().last_loc_sym().borrow().evaluations.len() == 1 {
            let sudo = sudo.as_mut().unwrap().borrow_mut();
            let loc_sudo = sudo.last_loc_sym();
            let evaluation = &mut loc_sudo.borrow_mut().evaluations[0];
            let eval_sym = &mut evaluation.symbol;
            eval_sym.get_symbol_hook = Some(PythonArchEvalHooks::eval_get_take_parent);
        }
        // ------------ create ------------
        let mut create = symbol.get_symbol(&(vec![], vec![S!("create")]));
        if create.is_some() && create.as_ref().unwrap().borrow().last_loc_sym().borrow().evaluations.len() == 1 {
            let create = create.as_mut().unwrap().borrow_mut();
            let loc_create = create.last_loc_sym();
            let evaluation = &mut loc_create.borrow_mut().evaluations[0];
            let eval_sym = &mut evaluation.symbol;
            eval_sym.get_symbol_hook = Some(PythonArchEvalHooks::eval_get_take_parent);
        }
        // ------------ search ------------
        let mut search = symbol.get_symbol(&(vec![], vec![S!("search")]));
        if search.is_some() && search.as_ref().unwrap().borrow().last_loc_sym().borrow().evaluations.len() == 1 {
            let search = search.as_mut().unwrap().borrow_mut();
            let loc_search = search.last_loc_sym();
            let evaluation = &mut loc_search.borrow_mut().evaluations[0];
            let eval_sym = &mut evaluation.symbol;
            eval_sym.get_symbol_hook = Some(PythonArchEvalHooks::eval_get_take_parent);
        }
    }

    fn eval_get_item(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (SymbolRef, bool)
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
                                from_module = Some(m.clone());
                            } else {
                                from_module = None;
                            }
                            let symbols = model.clone().borrow().get_main_symbols(session, from_module.clone(), &mut None);
                            if symbols.len() > 0 {
                                for s in symbols.iter() {
                                    if from_module.is_none() || ModuleSymbol::is_in_deps(session, &from_module.as_ref().unwrap(),&s.borrow().get_module_sym().unwrap().borrow()._module.as_ref().unwrap().dir_name, &mut None) {
                                        return (s.borrow().to_symbol_ref(), true);
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
        (evaluation_sym.symbol.clone(), true)
    }

    fn eval_test_cursor(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (SymbolRef, bool)
    {
        if context.is_some() && context.as_ref().unwrap().get(&S!("test_mode")).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool() {
            let test_cursor_sym = session.sync_odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![S!("TestCursor")]));
            match test_cursor_sym {
                Some(test_cursor_sym) => {
                    return (test_cursor_sym.borrow().to_sym_ref(), true);
                },
                None => {
                    return (evaluation_sym.symbol.clone(), true); //TODO really true?
                }
            }
        }
        return (evaluation_sym.symbol.clone(), true); //TODO really true?
    }

    fn on_env_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>, file_symbol: Rc<RefCell<Symbol>>) {
        let mut get_item = symbol.borrow().get_symbol(&(vec![], vec![S!("__getitem__")]));
        if get_item.is_some() {
            let loc_get_item = get_item.as_mut().unwrap().borrow_mut().last_loc_sym();
            let mut get_item = loc_get_item.borrow_mut();
            get_item.evaluations = vec![Evaluation {
                symbol: EvaluationSymbol {symbol: SymbolRef::empty(),
                    instance: true,
                    context: HashMap::new(),
                    factory: None,
                    get_symbol_hook: Some(PythonArchEvalHooks::eval_get_item)
                },
                value: None,
                range: None
            }];
        }
        let mut cr = symbol.borrow().get_symbol(&(vec![], vec![S!("cr")]));
        let curosor_file = odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![]));
        let cursor_sym = odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![S!("Cursor")]));
        if cursor_sym.is_some() && cr.is_some() {
            let cr_mut = cr.as_mut().unwrap().borrow_mut().last_loc_sym();

            let sym_ref = cr_mut.borrow().to_symbol_ref();
            cr_mut.borrow_mut().evaluations = vec![Evaluation {
                symbol: EvaluationSymbol::new(
                    sym_ref,
                    true,
                    HashMap::new(),
                    None,
                    Some(PythonArchEvalHooks::eval_test_cursor)
                ),
                value: None,
                range: None,
            }];
            file_symbol.borrow_mut().add_dependency(&mut curosor_file.unwrap().borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
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
        let env_file = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![]));
        let env_model = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![S!("Environment")]));
        let env_var = symbol.borrow().get_symbol(&(vec![], vec![S!("env")]));
        if env_model.is_some() && env_var.is_some() {
            let env_model = env_model.unwrap();
            let env_var = env_var.unwrap();
            let env_var = env_var.borrow_mut();
            let mut context = HashMap::new();
            context.insert(S!("test_mode"), ContextValue::BOOLEAN(true));
            env_var.last_loc_sym().borrow_mut().evaluations = vec![Evaluation {
                symbol: EvaluationSymbol::new(
                    env_model.borrow().last_loc_sym().borrow().to_symbol_ref(),
                    true,
                    context,
                    None,
                    None
                ),
                value: None,
                range: None,
            }];
            file_symbol.borrow_mut().add_dependency(&mut env_file.unwrap().borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
        }
    }

    fn eval_get(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (SymbolRef, bool)
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
        (evaluation_sym.symbol.clone(), evaluation_sym.instance)
    }

    fn _update_get_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>, tree: Tree) {
        let get_sym = symbol.borrow().get_symbol(&(vec![], vec![S!("__get__")]));
        if get_sym.is_none() {
            return;
        }
        let return_sym = odoo.get_symbol(&tree);
        if return_sym.is_none() {
            symbol.borrow_mut().not_found_paths.push((BuildSteps::ARCH_EVAL, flatten_tree(&tree)));
            odoo.not_found_symbols.insert(symbol);
            return;
        }
        get_sym.as_ref().unwrap().borrow_mut().last_loc_sym().borrow_mut().evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new(
                return_sym.unwrap().borrow().last_loc_sym().borrow().to_symbol_ref(),
                true,
                HashMap::new(),
                None,
                Some(PythonArchEvalHooks::eval_get)
            ),
            value: None,
            range: None
        }];
    }

    fn eval_relational(session: &mut SessionInfo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (SymbolRef, bool)
    {
        if context.is_none() {
            return (evaluation_sym.symbol.clone(), evaluation_sym.instance);
        }
        let comodel = context.as_ref().unwrap().get(&S!("comodel"));
        if comodel.is_none() {
            return (evaluation_sym.symbol.clone(), evaluation_sym.instance);
        }
        let comodel = comodel.unwrap().as_string();
        //TODO let comodel_sym = odoo.models.get(comodel);
       (SymbolRef::empty(), false)
    }

    fn _update_get_eval_relational(symbol: Rc<RefCell<Symbol>>) {
        let get_sym = symbol.borrow().get_symbol(&(vec![], vec![S!("__get__")]));
        if get_sym.is_none() {
            return;
        }
        get_sym.unwrap().borrow_mut().last_loc_sym().borrow_mut().evaluations = vec![Evaluation {
            symbol: EvaluationSymbol::new(
                SymbolRef::empty(),
                true,
                HashMap::new(),
                None,
                Some(PythonArchEvalHooks::eval_relational)
            ),
            value: None,
            range: None,
        }];
    }
}