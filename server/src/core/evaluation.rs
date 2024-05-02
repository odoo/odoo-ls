use ruff_python_ast::Expr;
use ruff_text_size::TextRange;
use tower_lsp::lsp_types::Diagnostic;
use weak_table::traits::WeakElement;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::constants::*;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::S;

#[derive(Debug, Clone)]
pub enum EvaluationValue {
    CONSTANT(ruff_python_ast::Expr), //expr is a literal
    DICT(Vec<(ruff_python_ast::Expr, ruff_python_ast::Expr)>), //expr is a literal
    LIST(Vec<ruff_python_ast::Expr>), //expr is a literal
    TUPLE(Vec<ruff_python_ast::Expr>) //expr is a literal
}

#[derive(Debug)]
pub struct Evaluation {
    //symbol lead to type evaluation, while value evaluate the value if it is the evaluation of a CONSTANT Symbol.
    pub symbol: EvaluationSymbol,
    pub value: Option<EvaluationValue>
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

    pub fn new_list(odoo: &mut SyncOdoo, values: Vec<Expr>) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                symbol: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("list")])).expect("builtins list not found")),
                instance: true,
                context: HashMap::new(),
                _internal_hold_symbol: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::LIST(values))
        }
    }

    pub fn new_tuple(odoo: &mut SyncOdoo, values: Vec<Expr>) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                symbol: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("tuple")])).expect("builtins list not found")),
                instance: true,
                context: HashMap::new(),
                _internal_hold_symbol: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::TUPLE(values))
        }
    }

    pub fn new_dict(odoo: &mut SyncOdoo, values: Vec<(Expr, Expr)>) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                symbol: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("dict")])).expect("builtins list not found")),
                instance: true,
                context: HashMap::new(),
                _internal_hold_symbol: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::DICT(values))
        }
    }

    pub fn new_constant(odoo: &mut SyncOdoo, values: Expr) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                symbol: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("dict")])).expect("builtins list not found")),
                instance: true,
                context: HashMap::new(),
                _internal_hold_symbol: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::CONSTANT(values))
        }
    }

    pub fn follow_ref_and_get_value(&self, odoo: &mut SyncOdoo, context: &mut Option<Context>) -> Option<EvaluationValue> {
        if self.value.is_some() {
            Some(self.value.as_ref().unwrap().clone())
        } else {
            let symbol = self.symbol.get_symbol(odoo, context);
            let symbol = symbol.upgrade();
            if symbol.is_some() {
                let symbol = Symbol::follow_ref(symbol.unwrap(), odoo, context, false);
                let symbol = symbol.0.upgrade();
                if symbol.is_some() {
                    let symbol = symbol.unwrap();
                    let symbol = symbol.borrow();
                    if symbol.evaluation.is_some() {
                        let eval = symbol.evaluation.as_ref().unwrap();
                        if eval.value.is_some() {
                            return Some((*eval).value.as_ref().unwrap().clone());
                        }
                    }
                }
            }
            None
        }
    }

    pub fn eval_from_symbol(symbol: &Rc<RefCell<Symbol>>) -> Evaluation{
        let mut instance = false;
        if [SymType::VARIABLE, SymType::CONSTANT].contains(&symbol.borrow_mut().sym_type) {
            instance = true
        }
        Evaluation {
            symbol: EvaluationSymbol {symbol: Rc::downgrade(symbol),
                instance: instance,
                context: HashMap::new(),
                _internal_hold_symbol: None,
                get_symbol_hook: None
            },
            value: None,
        }
    }

    fn eval_literal(odoo: &mut SyncOdoo, eval_sym: &mut EvaluationSymbol, range: &TextRange, expr: &Expr) {
        eval_sym._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_c".to_string(), SymType::CONSTANT))));
        eval_sym.symbol = Rc::downgrade(eval_sym._internal_hold_symbol.as_ref().unwrap());
        eval_sym.instance = true;
        eval_sym._internal_hold_symbol.as_ref().unwrap().borrow_mut().range = Some(*range);
        eval_sym._internal_hold_symbol.as_ref().unwrap().borrow_mut().evaluation = Some(Evaluation::new_constant(odoo, expr.clone()));
    }

    // eval an ast expression that represent the evaluation of a symbol.
    // For example, in a= 1+2, it will create the evaluation of 1+2 to be stored on 'a'
    // max_infer must be the range of 'a'
    pub fn eval_from_ast(odoo: &mut SyncOdoo, ast: &Expr, parent: Rc<RefCell<Symbol>>, max_infer: &TextRange) -> (Option<Evaluation>, Vec<Diagnostic>) {
        let mut res = EvaluationSymbol::default();
        let mut diagnostics = vec![];
        match ast {
            Expr::StringLiteral(expr) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast);
            },
            Expr::BytesLiteral(expr) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast);
            },
            Expr::NumberLiteral(expr) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast);
            },
            Expr::BooleanLiteral(expr) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast);
            },
            Expr::NoneLiteral(expr) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast);
            },
            Expr::EllipsisLiteral(expr) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast);
            }
            Expr::List(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_l".to_string(), SymType::CONSTANT))));
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().range = Some(expr.range);
                res.symbol = Rc::downgrade(res._internal_hold_symbol.as_ref().unwrap());
                res.instance = true;
                let mut values: Vec<ruff_python_ast::Expr> = Vec::new();
                for e in expr.elts.iter() {
                    if e.is_literal_expr() {
                        values.push(e.clone());
                    } else {
                        values = Vec::new(); break;
                    }
                }
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().evaluation = Some(Evaluation::new_list(odoo, values));
            },
            Expr::Tuple(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_t".to_string(), SymType::CONSTANT))));
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().range = Some(expr.range);
                res.symbol = Rc::downgrade(res._internal_hold_symbol.as_ref().unwrap());
                res.instance = true;
                let mut values: Vec<ruff_python_ast::Expr> = Vec::new();
                for e in expr.elts.iter() {
                    if e.is_literal_expr() {
                        values.push(e.clone());
                    } else {
                        values = Vec::new(); break;
                    }
                }
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().evaluation = Some(Evaluation::new_tuple(odoo, values));
            },
            Expr::Dict(expr) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_d".to_string(), SymType::CONSTANT))));
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().range = Some(expr.range);
                res.symbol = Rc::downgrade(res._internal_hold_symbol.as_ref().unwrap());
                res.instance = true;
                let mut values: Vec<(ruff_python_ast::Expr, ruff_python_ast::Expr)> = Vec::new();
                for (index, e) in expr.keys.iter().enumerate() {
                    let dict_value = expr.values.get(index).unwrap();
                    match e {
                        Some(key) => {
                            if key.is_literal_expr() && dict_value.is_literal_expr() {
                                values.push((key.clone(), dict_value.clone()));
                            } else {
                                values.clear(); break;
                            }
                        },
                        None => {
                            // do not handle dict unpacking
                            values.clear(); break;
                        }
                    }
                }
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().evaluation = Some(Evaluation::new_dict(odoo, values));
            },
            Expr::Call(expr) => {
                //TODO implement Call
            },
            Expr::Attribute(expr) => {
                let (eval, diags) = Evaluation::eval_from_ast(odoo, &expr.value, parent, max_infer);
                diagnostics.extend(diags);
                if eval.is_none() || eval.as_ref().unwrap().symbol.symbol.upgrade().is_none() {
                    return (None, diagnostics);
                }
                let base = eval.unwrap().symbol.symbol.upgrade();
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
                    None => {return (None, diagnostics);}
                }
            },
            Expr::Name(expr) => {
                let infered_sym = Symbol::infer_name(odoo, &parent, &expr.id.to_string(), Some(*max_infer));
                if infered_sym.is_none() {
                    return (None, diagnostics);
                }
                res.symbol = Rc::downgrade(infered_sym.as_ref().unwrap());
                let infered_sym = infered_sym.as_ref().unwrap().borrow();
                res.instance = infered_sym.sym_type != SymType::CLASS;
                if infered_sym.evaluation.is_some() {
                    res.instance = infered_sym.evaluation.as_ref().unwrap().symbol.instance;
                }
            },
            Expr::Subscript(sub) => {
                let (eval_left, diags) = Evaluation::eval_from_ast(odoo, &sub.value, parent, max_infer);
                diagnostics.extend(diags);
                if eval_left.is_none() || eval_left.as_ref().unwrap().symbol.symbol.upgrade().is_none() {
                    return (None, diagnostics);
                }
                let base = eval_left.unwrap().symbol.symbol.upgrade();
                let base = base.unwrap();
                let (base, _) = Symbol::follow_ref(base, odoo, &mut None, false);
                let base = base.upgrade().unwrap();
                let base = base.borrow();
                let get_item = base.get_symbol(&(vec![], vec![S!("__getitem__")]));
                if let Some(get_item) = get_item {
                    let get_item = get_item.borrow();
                    if let Some(get_item_eval) = &get_item.evaluation {
                        if let Some(hook) = get_item_eval.symbol.get_symbol_hook {
                            //TODO work on context 
                            // let hook_result = hook(odoo, &get_item_eval.symbol, context);
                            // if !hook_result.is_expired() {
                                
                            // }
                        }
                    }
                }
            },
            _ => {}
        }
        (Some(Evaluation {
            symbol: res,
            value: None,
        }), diagnostics)
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