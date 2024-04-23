use std::collections::HashMap;
use std::rc::Rc;
use std::rc::Weak;
use std::cell::RefCell;
use crate::core::odoo::SyncOdoo;
use crate::core::evaluation::Context;
use crate::core::symbol::Symbol;
use crate::constants::*;
use crate::S;

use super::evaluation::Evaluation;
use super::evaluation::ContextValue;
use super::evaluation::EvaluationValue;
use super::evaluation::EvaluationSymbol;

pub struct PythonArchEvalHooks {}

impl PythonArchEvalHooks {

    pub fn on_file_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        let tree = symbol.borrow().get_tree();
        let name = symbol.borrow().name.clone();
        match name.as_str() {
            "model" => {
                if tree == (vec![S!("odoo"), S!("models")], vec!()) {
                    let base_model = symbol.borrow().get_symbol(&(vec![], vec![S!("BaseModel")]));
                    if base_model.is_some() {
                        PythonArchEvalHooks::on_base_model_eval(odoo, base_model.unwrap())
                    }
                }
            },
            "api" => {
                if tree == (vec![S!("odoo"), S!("api")], vec!()) {
                    let env = symbol.borrow().get_symbol(&(vec![], vec![S!("Environment")]));
                    if env.is_some() {
                        PythonArchEvalHooks::on_env_eval(odoo, env.unwrap())
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
                        PythonArchEvalHooks::on_transaction_class_eval(odoo, transaction_class.unwrap());
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

    fn eval_get_take_parent(odoo: &mut SyncOdoo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>) -> Weak<RefCell<Symbol>>
    {
        todo!()
    }

    fn on_base_model_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        let sym = symbol.borrow();
        // ----------- __iter__ ------------
        let mut iter = sym.get_symbol(&(vec![], vec![S!("__iter__")]));
        if iter.is_some() && iter.as_ref().unwrap().borrow().evaluation.is_some() {
            let mut iter = iter.as_mut().unwrap().borrow_mut();
            let evaluation = iter.evaluation.as_mut().unwrap();
            let eval_sym = evaluation.as_symbol_mut().unwrap();
            eval_sym.get_symbol_hook = Some(PythonArchEvalHooks::eval_get_take_parent);
        }
        // ----------- env ------------
        let env = sym.get_symbol(&(vec![], vec![S!("env")]));
        let env_class = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![S!("Environment")]));
        if env.is_some() && env_class.is_some() {
            let env = env.unwrap();
            let env_class = env_class.unwrap();
            let mut env = env.borrow_mut();
            let mut context = HashMap::new();
            context.insert(S!("test_mode"), super::evaluation::ContextValue::BOOLEAN(true));
            env.evaluation = Some(Evaluation::EvaluationSymbol(EvaluationSymbol{
                symbol: Rc::downgrade(&env_class),
                context: context,
                instance: true,
                _internal_hold_symbol: None,
                get_symbol_hook: None
            }));
            env.add_dependency(&mut env_class.borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
            env.doc_string = Some(S!(""));
        }
        // ------------ ids ------------
        let ids = sym.get_symbol(&(vec![], vec![S!("ids")]));
        if ids.is_some() {
            let mut list = Symbol::new(S!("_l"), SymType::VARIABLE);
            let mut values: Vec<ruff_python_ast::Expr> = Vec::new();
            list.evaluation = Some(Evaluation::EvaluationValue(EvaluationValue::LIST(values)));
            let rc_list = Rc::new(RefCell::new(list));
            let mut ids = ids.as_ref().unwrap().borrow_mut();
            ids.evaluation = Some(Evaluation::EvaluationSymbol(EvaluationSymbol{
                symbol: Rc::downgrade(&rc_list),
                context: HashMap::new(),
                instance: true,
                _internal_hold_symbol: Some(rc_list),
                get_symbol_hook: None
            }));
        }
        // ------------ sudo ------------
        let mut sudo = sym.get_symbol(&(vec![], vec![S!("sudo")]));
        if sudo.is_some() && sudo.as_ref().unwrap().borrow().evaluation.is_some() {
            let mut sudo = sudo.as_mut().unwrap().borrow_mut();
            let evaluation = sudo.evaluation.as_mut().unwrap();
            let eval_sym = evaluation.as_symbol_mut().unwrap();
            eval_sym.get_symbol_hook = Some(PythonArchEvalHooks::eval_get_take_parent);
        }
        // ------------ create ------------
        let mut create = sym.get_symbol(&(vec![], vec![S!("create")]));
        if create.is_some() && create.as_ref().unwrap().borrow().evaluation.is_some() {
            let mut create = create.as_mut().unwrap().borrow_mut();
            let evaluation = create.evaluation.as_mut().unwrap();
            let eval_sym = evaluation.as_symbol_mut().unwrap();
            eval_sym.get_symbol_hook = Some(PythonArchEvalHooks::eval_get_take_parent);
        }
        // ------------ search ------------
        let mut search = sym.get_symbol(&(vec![], vec![S!("search")]));
        if search.is_some() && search.as_ref().unwrap().borrow().evaluation.is_some() {
            let mut search = search.as_mut().unwrap().borrow_mut();
            let evaluation = search.evaluation.as_mut().unwrap();
            let eval_sym = evaluation.as_symbol_mut().unwrap();
            eval_sym.get_symbol_hook = Some(PythonArchEvalHooks::eval_get_take_parent);
        }
    }

    fn eval_get_item(odoo: &mut SyncOdoo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>) -> Weak<RefCell<Symbol>>
    {
        //TODO after models
        todo!()
    }

    fn eval_test_cursor(odoo: &mut SyncOdoo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>) -> Weak<RefCell<Symbol>>
    {
        if context.is_some() && context.as_ref().unwrap().get(&S!("test_mode")).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool() {
            let test_cursor_sym = odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![S!("TestCursor")]));
            match test_cursor_sym {
                Some(test_cursor_sym) => {
                    return Rc::downgrade(&test_cursor_sym);
                },
                None => {
                    return evaluation_sym.symbol.clone();
                }
            }
        }
        return evaluation_sym.symbol.clone();
    }

    fn on_env_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        let mut get_item = symbol.borrow().get_symbol(&(vec![], vec![S!("__getitem__")]));
        if get_item.is_some() && get_item.as_ref().unwrap().borrow().evaluation.is_some() {
            let mut get_item = get_item.as_mut().unwrap().borrow_mut();
            let evaluation = get_item.evaluation.as_mut().unwrap();
            let eval_sym = evaluation.as_symbol_mut().unwrap();
            eval_sym.get_symbol_hook = Some(PythonArchEvalHooks::eval_get_item);
        }
        let mut cr = symbol.borrow().get_symbol(&(vec![], vec![S!("cr")]));
        let cursor_sym = odoo.get_symbol(&(vec![S!("odoo"), S!("sql_db")], vec![S!("Cursor")]));
        if cursor_sym.is_some() && cr.is_some() {
            let mut cr_mut = cr.as_mut().unwrap().borrow_mut();

            cr_mut.evaluation = Some(Evaluation::EvaluationSymbol(EvaluationSymbol{
                symbol: Rc::downgrade(cursor_sym.as_ref().unwrap()),
                context: HashMap::new(),
                instance: true,
                _internal_hold_symbol: None,
                get_symbol_hook: Some(PythonArchEvalHooks::eval_test_cursor)
            }));
            cr_mut.add_dependency(&mut cursor_sym.unwrap().borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
        }
    }

    fn on_form_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        if odoo.full_version < S!("16.3") {
            return;
        }
        let file = symbol.borrow().get_in_parents(&vec![SymType::FILE], false);
        todo!()
    }

    fn on_transaction_class_eval(odoo: &mut SyncOdoo, symbol: Rc<RefCell<Symbol>>) {
        let env_model = odoo.get_symbol(&(vec![S!("odoo"), S!("api")], vec![S!("Environment")]));
        let env_var = symbol.borrow().get_symbol(&(vec![], vec![S!("env")]));
        if env_model.is_some() && env_var.is_some() {
            let env_model = env_model.unwrap();
            let env_var = env_var.unwrap();
            let mut env_var = env_var.borrow_mut();
            let mut context = HashMap::new();
            context.insert(S!("test_mode"), ContextValue::BOOLEAN(true));
            env_var.evaluation = Some(Evaluation::EvaluationSymbol(EvaluationSymbol{
                symbol: Rc::downgrade(&env_model),
                context: context,
                instance: true,
                _internal_hold_symbol: None,
                get_symbol_hook: None
            }));
            env_var.add_dependency(&mut env_model.borrow_mut(), BuildSteps::ARCH_EVAL, BuildSteps::ARCH);
        }
    }

    fn eval_get(odoo: &mut SyncOdoo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>) -> Weak<RefCell<Symbol>>
    {
        if context.is_some() {
            let parent_instance = context.as_ref().unwrap().get(&S!("parent_instance"));
            if parent_instance.is_some() {
                match parent_instance.unwrap() {
                    ContextValue::BOOLEAN(b) => {
                        if !*b {
                            return Weak::new();
                        }
                    },
                    _ => {}
                }
            }
        }
        evaluation_sym.symbol.clone()
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
        let mut var_sym = Symbol::new(S!("returned_value"), SymType::CONSTANT);
        var_sym.evaluation = Some(Evaluation::EvaluationSymbol(EvaluationSymbol{
            symbol: Rc::downgrade(&return_sym.unwrap()),
            context: HashMap::new(),
            instance: true,
            _internal_hold_symbol: None,
            get_symbol_hook: None
        }));
        get_sym.as_ref().unwrap().borrow_mut().evaluation = Some(Evaluation::EvaluationSymbol(EvaluationSymbol::new_with_symbol(
            var_sym,
            true,
            HashMap::new())));
        get_sym.as_ref().unwrap().borrow_mut().evaluation.as_mut().unwrap().as_symbol_mut().unwrap().get_symbol_hook = Some(PythonArchEvalHooks::eval_get);
    }

    fn eval_relational(odoo: &mut SyncOdoo, evaluation_sym: &EvaluationSymbol, context: &mut Option<Context>) -> Weak<RefCell<Symbol>>
    {
        if context.is_none() {
            return evaluation_sym.symbol.clone();
        }
        let comodel = context.as_ref().unwrap().get(&S!("comodel"));
        if comodel.is_none() {
            return evaluation_sym.symbol.clone();
        }
        let comodel = comodel.unwrap().as_string();
        //TODO let comodel_sym = odoo.models.get(comodel);
        Weak::new()
    }

    fn _update_get_eval_relational(symbol: Rc<RefCell<Symbol>>) {
        let get_sym = symbol.borrow().get_symbol(&(vec![], vec![S!("__get__")]));
        if get_sym.is_none() {
            return;
        }
        get_sym.unwrap().borrow_mut().evaluation = Some(Evaluation::EvaluationSymbol(EvaluationSymbol{
            symbol: Weak::new(),
            context: HashMap::new(),
            instance: true,
            _internal_hold_symbol: None,
            get_symbol_hook: Some(PythonArchEvalHooks::eval_relational)
        }));
    }
}