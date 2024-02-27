use rustpython_parser::ast::{Expr};
use rustpython_parser::text_size::TextRange;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::constants::*;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;

#[derive(Debug)]
pub enum EvaluationValue {
    CONSTANT(rustpython_parser::ast::Constant),
    DICT(Vec<(rustpython_parser::ast::Constant, rustpython_parser::ast::Constant)>),
    LIST(Vec<rustpython_parser::ast::Constant>),
    TUPLE(Vec<rustpython_parser::ast::Constant>)
}


#[derive(Debug, Default)]
pub struct Evaluation {
    symbol: Weak<RefCell<Symbol>>,
    pub instance: bool,
    pub context: HashMap<String, bool>,
    pub value: Option<EvaluationValue>,
    _internal_hold_symbol: Option<Rc<RefCell<Symbol>>>,
}

impl Evaluation {

    pub fn eval_from_symbol(symbol: &Rc<RefCell<Symbol>>) -> Evaluation{
        let mut instance = false;
        if [SymType::VARIABLE, SymType::CONSTANT].contains(&symbol.borrow_mut().sym_type) {
            instance = true
        }
        Evaluation {
            symbol: Rc::downgrade(symbol),
            instance: instance,
            context: HashMap::new(),
            value: None,
            _internal_hold_symbol: None,
        }
    }

    // eval an ast expression that represent the evaluation of a symbol.
    // For example, in a= 1+2, it will create the evaluation of 1+2 to be stored on a
    pub fn eval_from_ast(odoo: &mut SyncOdoo, ast: &Expr<TextRange>, parent: Rc<RefCell<Symbol>>) -> Option<Evaluation> {
        let mut res = Evaluation::default();
        match ast {
            Expr::Constant(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_c".to_string(), SymType::CONSTANT)))); //TODO check to not hold a dummy symbol for constants
                res.symbol = Rc::downgrade(res._internal_hold_symbol.as_ref().unwrap());
                res.instance = true;
                res.value = Some(EvaluationValue::CONSTANT(expr.value.clone()));
            },
            Expr::List(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_l".to_string(), SymType::CONSTANT))));
                res.symbol = Rc::downgrade(res._internal_hold_symbol.as_ref().unwrap());
                res.instance = true;
                let mut values: Vec<rustpython_parser::ast::Constant> = Vec::new();
                for e in expr.elts.iter() {
                    match e {
                        Expr::Constant(v) => {
                            values.push(v.value.clone());
                        },
                        _ => {values = Vec::new(); break;}
                    }
                }
                if values.len() > 0 {
                    res.value = Some(EvaluationValue::LIST(values));
                } else {
                    res.value = None;
                }
            },
            Expr::Tuple(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_t".to_string(), SymType::CONSTANT))));
                res.symbol = Rc::downgrade(res._internal_hold_symbol.as_ref().unwrap());
                res.instance = true;
                let mut values: Vec<rustpython_parser::ast::Constant> = Vec::new();
                for e in expr.elts.iter() {
                    match e {
                        Expr::Constant(v) => {
                            values.push(v.value.clone());
                        },
                        _ => {values = Vec::new(); break;}
                    }
                }
                if values.len() > 0 {
                    res.value = Some(EvaluationValue::TUPLE(values));
                } else {
                    res.value = None;
                }
            },
            Expr::Dict(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_d".to_string(), SymType::CONSTANT))));
                res.symbol = Rc::downgrade(res._internal_hold_symbol.as_ref().unwrap());
                res.instance = true;
                let mut values: Vec<(rustpython_parser::ast::Constant, rustpython_parser::ast::Constant)> = Vec::new();
                for (index, e) in expr.keys.iter().enumerate() {
                    let dict_value = expr.values.get(index).unwrap();
                    match e {
                        Some(key) => {
                            match key {
                                Expr::Constant(key_const) => {
                                    match dict_value {
                                        Expr::Constant(dict_value_const) => {
                                            values.push((key_const.value.clone(), dict_value_const.value.clone()));
                                        },
                                        _ => {
                                            values.clear(); break;
                                        }
                                    }
                                },
                                _ => {
                                    values.clear(); break;
                                }
                            }
                        },
                        None => {
                            // do not handle dict unpacking
                            values.clear(); break;
                        }
                    }
                }
                if values.len() > 0 {
                    res.value = Some(EvaluationValue::DICT(values));
                } else {
                    res.value = None;
                }
            },
            Expr::Call(expr) => {
                //TODO
            },
            Expr::Attribute(expr) => {
                let eval = Evaluation::eval_from_ast(odoo, &expr.value, parent);
                if eval.is_none() || eval.as_ref().unwrap().symbol.upgrade().is_none() {
                    return None;
                }
                let base = eval.unwrap().symbol.upgrade();
                if base.is_none() {
                    return None;
                }
                let base = base.unwrap();
                let (base, _) = Symbol::follow_ref(base, odoo, false);
                let attribute = base.upgrade().unwrap();
                let attribute = (*attribute).borrow();
                let attribute = attribute.symbols.get(expr.attr.as_str());
                match attribute {
                    Some(att) => {
                        res.symbol = Rc::downgrade(att);
                        res.instance = (**att).borrow().sym_type == SymType::VARIABLE;
                    }
                    None => {return None;}
                }
            },
            Expr::Name(expr) => {
                let parent = parent.borrow();
                let infered_sym = parent.infer_name(odoo, expr.id.to_string(), expr.range);
                if infered_sym.is_none() {
                    return None;
                }
                res.symbol = Rc::downgrade(infered_sym.as_ref().unwrap());
                let infered_sym = infered_sym.as_ref().unwrap().borrow();
                res.instance = infered_sym.sym_type != SymType::CLASS;
                if infered_sym.evaluation.is_some() {
                    res.instance = infered_sym.evaluation.as_ref().unwrap().instance;
                }

            },
            _ => {}
        }
        Some(res)
    }

    pub fn get_symbol(&self) -> &Weak<RefCell<Symbol>> { //TODO evaluate context
        &self.symbol
    }

}