use rustpython_parser::ast::{Expr};
use rustpython_parser::text_size::TextRange;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::constants::*;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;

#[derive(Debug, Clone)]
pub enum EvaluationValue {
    CONSTANT(rustpython_parser::ast::Constant),
    DICT(Vec<(rustpython_parser::ast::Constant, rustpython_parser::ast::Constant)>),
    LIST(Vec<rustpython_parser::ast::Constant>),
    TUPLE(Vec<rustpython_parser::ast::Constant>)
}

#[derive(Debug)]
pub enum Evaluation {
    EvaluationSymbol(EvaluationSymbol),
    EvaluationValue(EvaluationValue)
}

#[derive(Debug, Clone)]
pub enum ContextValue {
    BOOLEAN(bool),
    STRING(String)
}

impl ContextValue {
    pub fn as_bool(&self) -> bool {
        match self {
            ContextValue::BOOLEAN(b) => *b,
            _ => panic!("Not a boolean")
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            ContextValue::STRING(s) => s.clone(),
            _ => panic!("Not a string")
        }
    }
}

pub type Context = HashMap<String, ContextValue>;

type GetSymbolHook = fn (odoo: &mut SyncOdoo, eval: &EvaluationSymbol, context: &mut Option<Context>) -> Weak<RefCell<Symbol>>;

#[derive(Debug, Default)]
pub struct EvaluationSymbol {
    pub symbol: Weak<RefCell<Symbol>>,
    pub instance: bool,
    pub context: Context,
    pub _internal_hold_symbol: Option<Rc<RefCell<Symbol>>>,
    pub get_symbol_hook: Option<GetSymbolHook>,
}

impl Evaluation {

    pub fn is_symbol(&self) -> bool {
        match self {
            Evaluation::EvaluationSymbol(s) => true,
            _ => false
        }
    }

    pub fn is_value(&self) -> bool {
        match self {
            Evaluation::EvaluationValue(s) => true,
            _ => false
        }
    }

    pub fn as_symbol(&self) -> Option<&EvaluationSymbol> {
        match self {
            Evaluation::EvaluationSymbol(s) => Some(s),
            _ => None
        }
    }

    pub fn as_value(&self) -> Option<&EvaluationValue> {
        match self {
            Evaluation::EvaluationValue(s) => Some(s),
            _ => None
        }
    }

    pub fn as_symbol_mut(&mut self) -> Option<&mut EvaluationSymbol> {
        match self {
            Evaluation::EvaluationSymbol(s) => Some(s),
            _ => None
        }
    }

    pub fn as_value_mut(&mut self) -> Option<&mut EvaluationValue> {
        match self {
            Evaluation::EvaluationValue(s) => Some(s),
            _ => None
        }
    }

    pub fn follow_ref_and_get_value(&self, odoo: &mut SyncOdoo, context: &mut Option<Context>) -> Option<EvaluationValue> {
        match self {
            Evaluation::EvaluationValue(v) => {
                Some((*v).clone())
            },
            Evaluation::EvaluationSymbol(s) => {
                let symbol = s.get_symbol(odoo, context);
                let symbol = symbol.upgrade();
                if symbol.is_some() {
                    let symbol = Symbol::follow_ref(symbol.unwrap(), odoo, context, false);
                    let symbol = symbol.0.upgrade();
                    if symbol.is_some() {
                        let symbol = symbol.unwrap();
                        let symbol = symbol.borrow();
                        if symbol.evaluation.is_some() {
                            let eval = symbol.evaluation.as_ref().unwrap();
                            if eval.is_value() {
                                return Some((*eval).as_value().unwrap().clone());
                            }
                        }
                    }
                }
                None
            }
        }
    }

    pub fn eval_from_symbol(symbol: &Rc<RefCell<Symbol>>) -> Evaluation{
        let mut instance = false;
        if [SymType::VARIABLE, SymType::CONSTANT].contains(&symbol.borrow_mut().sym_type) {
            instance = true
        }
        Evaluation::EvaluationSymbol(EvaluationSymbol {
            symbol: Rc::downgrade(symbol),
            instance: instance,
            context: HashMap::new(),
            _internal_hold_symbol: None,
            get_symbol_hook: None
        })
    }

    // eval an ast expression that represent the evaluation of a symbol.
    // For example, in a= 1+2, it will create the evaluation of 1+2 to be stored on a
    pub fn eval_from_ast(odoo: &mut SyncOdoo, ast: &Expr<TextRange>, parent: Rc<RefCell<Symbol>>) -> Option<Evaluation> {
        let mut res = EvaluationSymbol::default();
        match ast {
            Expr::Constant(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_c".to_string(), SymType::CONSTANT)))); //TODO check to not hold a dummy symbol for constants
                res.symbol = Rc::downgrade(res._internal_hold_symbol.as_ref().unwrap());
                res.instance = true;
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().range = Some(expr.range);
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().evaluation = Some(Evaluation::EvaluationValue(EvaluationValue::CONSTANT(expr.value.clone())));
            },
            Expr::List(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_l".to_string(), SymType::CONSTANT))));
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().range = Some(expr.range);
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
                    res._internal_hold_symbol.as_ref().unwrap().borrow_mut().evaluation = Some(Evaluation::EvaluationValue(EvaluationValue::LIST(values)));
                }
            },
            Expr::Tuple(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_t".to_string(), SymType::CONSTANT))));
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().range = Some(expr.range);
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
                    res._internal_hold_symbol.as_ref().unwrap().borrow_mut().evaluation = Some(Evaluation::EvaluationValue(EvaluationValue::TUPLE(values)));
                }
            },
            Expr::Dict(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_d".to_string(), SymType::CONSTANT))));
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().range = Some(expr.range);
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
                    res._internal_hold_symbol.as_ref().unwrap().borrow_mut().evaluation = Some(Evaluation::EvaluationValue(EvaluationValue::DICT(values)));
                }
            },
            Expr::Call(expr) => {
                //TODO implement Call
            },
            Expr::Attribute(expr) => {
                let eval = Evaluation::eval_from_ast(odoo, &expr.value, parent);
                if eval.is_none() || eval.as_ref().unwrap().as_symbol().unwrap().symbol.upgrade().is_none() {
                    return None;
                }
                let base = eval.unwrap().as_symbol().unwrap().symbol.upgrade();
                if base.is_none() {
                    return None;
                }
                let base = base.unwrap();
                let (base, _) = Symbol::follow_ref(base, odoo, &mut None, false);
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
                let infered_sym = parent.infer_name(odoo, expr.id.to_string(), Some(expr.range));
                if infered_sym.is_none() {
                    return None;
                }
                res.symbol = Rc::downgrade(infered_sym.as_ref().unwrap());
                let infered_sym = infered_sym.as_ref().unwrap().borrow();
                res.instance = infered_sym.sym_type != SymType::CLASS;
                if infered_sym.evaluation.is_some() && infered_sym.evaluation.as_ref().unwrap().is_symbol() {
                    res.instance = infered_sym.evaluation.as_ref().unwrap().as_symbol().unwrap().instance;
                }

            },
            _ => {}
        }
        Some(Evaluation::EvaluationSymbol(res))
    }

}

impl EvaluationSymbol {

    pub fn new_with_symbol(symbol: Symbol, instance: bool, context: Context) -> EvaluationSymbol {
        let sym = Rc::new(RefCell::new(symbol));
        EvaluationSymbol {
            symbol: Rc::downgrade(&sym),
            instance: instance,
            context: context,
            _internal_hold_symbol: Some(sym),
            get_symbol_hook: None
        }
    }

    pub fn get_symbol(&self, odoo:&mut SyncOdoo, context: &mut Option<Context>) -> Weak<RefCell<Symbol>> {
        if self.get_symbol_hook.is_some() {
            let hook = self.get_symbol_hook.unwrap();
            return hook(odoo, self, context);
        }
        self.symbol.clone()
    }
}