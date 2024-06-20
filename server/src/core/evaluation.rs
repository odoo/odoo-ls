use ruff_python_ast::{Identifier, Expr, Operator};
use ruff_text_size::{Ranged, TextRange, TextSize};
use tower_lsp::lsp_types::Diagnostic;
use weak_table::traits::WeakElement;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::constants::*;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::S;

use super::python_validator::PythonValidator;

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

#[derive(Debug)]
pub enum ExprOrIdent<'a> {
    Expr(&'a Expr),
    Ident(&'a Identifier),
}

impl ExprOrIdent<'_> {

    pub fn range(&self) -> TextRange{
        match self {
            ExprOrIdent::Expr(e) => {
                e.range()
            },
            ExprOrIdent::Ident(i) => {
                i.range()
            }
        }
    }

    pub fn expr(&self) -> &Expr {
        match self {
            ExprOrIdent::Expr(e) => {
                e
            },
            ExprOrIdent::Ident(i) => {
                panic!("ExprOrIdent is not an expr")
            }
        }
    }

}

#[derive(Debug, Clone)]
pub enum ContextValue {
    BOOLEAN(bool),
    STRING(String),
    MODULE(Rc<RefCell<Symbol>>),
    RANGE(TextRange)
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

    pub fn as_module(&self) -> Rc<RefCell<Symbol>> {
        match self {
            ContextValue::MODULE(m) => m.clone(),
            _ => panic!("Not a module")
        }
    }

    pub fn as_text_range(&self) -> TextRange {
        match self {
            ContextValue::RANGE(r) => r.clone(),
            _ => panic!("Not a TextRange")
        }
    }
}

pub type Context = HashMap<String, ContextValue>;

type GetSymbolHook = fn (odoo: &mut SyncOdoo, eval: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (Weak<RefCell<Symbol>>, bool);

#[derive(Debug, Default)]
pub struct EvaluationSymbol {
    pub symbol: Weak<RefCell<Symbol>>,
    pub instance: bool,
    pub context: Context,
    pub _internal_hold_symbol: Option<Rc<RefCell<Symbol>>>,
    pub factory: Option<Weak<RefCell<Symbol>>>,
    pub get_symbol_hook: Option<GetSymbolHook>,
}

#[derive(Default)]
pub struct AnalyzeAstResult {
    pub symbol: Option<Evaluation>,
    pub effective_sym: Option<Weak<RefCell<Symbol>>>,
    pub factory: Option<Weak<RefCell<Symbol>>>,
    pub context: Option<Context>,
    pub diagnostics: Vec<Diagnostic>
}

impl AnalyzeAstResult {
    pub fn from_only_diagnostics(diags: Vec<Diagnostic>) -> Self {
        AnalyzeAstResult { symbol: None, effective_sym: None, factory: None, context: None, diagnostics: diags }
    }
}

impl Evaluation {

    pub fn new_list(odoo: &mut SyncOdoo, values: Vec<Expr>) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                symbol: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("list")])).expect("builtins list not found")),
                instance: true,
                context: HashMap::new(),
                _internal_hold_symbol: None,
                factory: None,
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
                factory: None,
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
                factory: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::DICT(values))
        }
    }

    pub fn new_constant(odoo: &mut SyncOdoo, values: Expr) -> Evaluation {
        let tree_value = match &values {
            Expr::StringLiteral(s) => {
                (vec![S!("builtins")], vec![S!("str")])
            },
            Expr::BooleanLiteral(b) => {
                (vec![S!("builtins")], vec![S!("bool")])
            },
            Expr::NumberLiteral(n) => {
                (vec![S!("builtins")], vec![S!("int")]) //TODO
            },
            Expr::BytesLiteral(b) => {
                (vec![S!("builtins")], vec![S!("bytes")])
            }
            _ => {(vec![S!("builtins")], vec![S!("object")])}
        };
        let symbol;
        if !values.is_none_literal_expr() {
            symbol = Rc::downgrade(&odoo.get_symbol(&tree_value).expect("builtins class not found"));
        } else {
            symbol = Weak::new();
        }
        Evaluation {
            symbol: EvaluationSymbol {
                symbol: symbol,
                instance: true,
                context: HashMap::new(),
                _internal_hold_symbol: None,
                factory: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::CONSTANT(values))
        }
    }

    pub fn follow_ref_and_get_value(&self, odoo: &mut SyncOdoo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> Option<EvaluationValue> {
        if self.value.is_some() {
            Some(self.value.as_ref().unwrap().clone())
        } else {
            let symbol = self.symbol.get_symbol(odoo, context, diagnostics).0;
            let symbol = symbol.upgrade();
            if symbol.is_some() {
                let symbol = Symbol::follow_ref(symbol.unwrap(), odoo, context, false, true, diagnostics);
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
                factory: None,
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

    //Build an evaluation from an ast node that can be associated to a symbol
    //For example: a = "5"
    // eval_from_ast should be called on '"5"' to build the evaluation of 'a'
    pub fn eval_from_ast(odoo: &mut SyncOdoo, ast: &Expr, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize) -> (Option<Evaluation>, Vec<Diagnostic>) {
        let analyze_result = Evaluation::analyze_ast(odoo, &ExprOrIdent::Expr(ast), parent, max_infer);
        return (analyze_result.symbol, analyze_result.diagnostics)
    }

    /* Given an Expr, try to return the represented String. None if it can't be achieved */
    fn expr_to_str(odoo: &mut SyncOdoo, ast: &Expr, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize, diagnostics: &mut Vec<Diagnostic>) -> (Option<String>, Vec<Diagnostic>) {
        let value = Evaluation::analyze_ast(odoo, &ExprOrIdent::Expr(ast), parent, max_infer);
        if value.symbol.is_some() {
            let eval = value.symbol.unwrap();
            let v = eval.follow_ref_and_get_value(odoo, &mut None, diagnostics);
            if let Some(v) = v {
                match v {
                    EvaluationValue::CONSTANT(v) => {
                        match v {
                            Expr::StringLiteral(s) => {
                                return (Some(s.value.to_string()), value.diagnostics);
                            },
                            _ => {}
                        }
                    },
                    _ => {}
                }
            }
        }
        (None, value.diagnostics)
    }


    /*
    analyze_ast will extract all known information about an ast:
    result.0: the direct evaluation
    result.1: the effective symbol that would be used if the program is running
    result.2: the factory used to build the effective symbol
    result.3: the context after the evaluation
    result.4: the diagnostics that code is generating.
    Example:
        --------
        context
        --------
        A| class Char():
        B|     def __get__(self, instance, owner=None):
        C|         return ""
        D| MyChar = Char
        E| class Test():
        G|     a = MyChar()
        H| test = Test()
        --------
        result of analyze_ast("test.a") (with adapted parameters)
        --------
        symbol/evaluation: a (at G)
        effective_sym: str
        factory: Char (#TODO not MyChar?)
        context: {}
        diagnostics: vec![]

        this is used in following features:
        ast build -> symbol, diagnostics
        Hover -> effective_sym (will follow it to display type)
            -> factory (to show how it has been built)
        Definition -> symbol
        Autocompletion -> effective_sym
     */
    pub fn analyze_ast(odoo: &mut SyncOdoo, ast: &ExprOrIdent, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize) -> AnalyzeAstResult {
        let mut res = EvaluationSymbol::default();
        let mut effective_sym = None;
        let mut factory = None;
        let mut diagnostics = vec![];
        let from_module;
        if let Some(module) = parent.borrow().get_module_sym() {
            from_module = ContextValue::MODULE(module);
        } else {
            from_module = ContextValue::BOOLEAN(false);
        }
        let module: Option<Rc<RefCell<Symbol>>> = parent.borrow().get_module_sym();
        let mut context: Context = HashMap::from([
            (S!("module"), from_module),
            (S!("range"), ContextValue::RANGE(ast.range()))
        ]);

        match ast {
            ExprOrIdent::Expr(Expr::StringLiteral(expr)) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast.expr());
            },
            ExprOrIdent::Expr(Expr::BytesLiteral(expr)) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast.expr());
            },
            ExprOrIdent::Expr(Expr::NumberLiteral(expr)) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast.expr());
            },
            ExprOrIdent::Expr(Expr::BooleanLiteral(expr)) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast.expr());
            },
            ExprOrIdent::Expr(Expr::NoneLiteral(expr)) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast.expr());
            },
            ExprOrIdent::Expr(Expr::EllipsisLiteral(expr)) => {
                Evaluation::eval_literal(odoo, &mut res, &expr.range, ast.expr());
            }
            ExprOrIdent::Expr(Expr::List(expr)) => {
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
            ExprOrIdent::Expr(Expr::Tuple(expr)) => {
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
            ExprOrIdent::Expr(Expr::Dict(expr)) => {
                res._internal_hold_symbol = Some(Rc::new(RefCell::new(Symbol::new("_d".to_string(), SymType::CONSTANT))));
                res._internal_hold_symbol.as_ref().unwrap().borrow_mut().range = Some(expr.range);
                res.symbol = Rc::downgrade(res._internal_hold_symbol.as_ref().unwrap());
                res.instance = true;
                let mut values: Vec<(ruff_python_ast::Expr, ruff_python_ast::Expr)> = Vec::new();
                for (index, e) in expr.iter_keys().enumerate() {
                    let dict_value = &expr.items.get(index).unwrap().value;
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
            ExprOrIdent::Expr(Expr::Call(expr)) => {
                let (base_eval, diags) = Evaluation::eval_from_ast(odoo, &expr.func, parent, max_infer);
                diagnostics.extend(diags);
                if base_eval.is_none() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let (base_sym, instance) = base_eval.as_ref().unwrap().symbol.get_symbol(odoo, &mut None, &mut diagnostics);
                let base_sym = base_sym.upgrade();
                if let Some(base_sym) = base_sym {
                    if base_sym.borrow().sym_type == SymType::CLASS {
                        if instance {
                            //TODO handle call on class instance
                        } else {
                            //TODO diagnostic __new__ call parameters
                            res.symbol = Rc::downgrade(&base_sym);
                            res.instance = true;
                        }
                    } else if base_sym.borrow().sym_type == SymType::FUNCTION {
                        //function return evaluation can come from:
                        //  - type annotation parsing (ARCH_EVAL step)
                        //  - documentation parsing (Arch_eval and VALIDATION step)
                        //  - function body inference (VALIDATION step)
                        // Therefore, the actual version of the algorithm will trigger build from the different steps if this one has already been reached.
                        // We don't want to launch validation step while Arch evaluating the code.
                        if base_sym.borrow().evaluation.is_none() {
                            if base_sym.borrow().odoo_status == BuildStatus::DONE {
                                let mut v = PythonValidator::new(base_sym.clone());
                                v.validate(odoo);
                            }
                        }
                        if let Some(evaluation) = base_sym.borrow().evaluation.as_ref() {
                            (res.symbol, res.instance) = evaluation.symbol.get_symbol(odoo, &mut None, &mut diagnostics);
                        }
                    } else {
                        println!("not able to do a call on {:?}", base_sym.borrow().sym_type);
                    }
                }
            },
            ExprOrIdent::Expr(Expr::Attribute(expr)) => {
                let (eval, diags) = Evaluation::eval_from_ast(odoo, &expr.value, parent, max_infer);
                diagnostics.extend(diags);
                if eval.is_none() || eval.as_ref().unwrap().symbol.get_symbol(odoo, &mut None, &mut diagnostics).0.upgrade().is_none() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base = eval.unwrap().symbol.get_symbol(odoo, &mut None, &mut diagnostics).0.upgrade();
                let base = base.unwrap();
                let (base, _) = Symbol::follow_ref(base, odoo, &mut None, false, false, &mut diagnostics);
                let attribute = base.upgrade().unwrap();
                let attribute = (*attribute).borrow();
                let attribute = attribute.get_member_symbol(odoo, &expr.attr.to_string(), module, false, false, true, &mut diagnostics);
                if attribute.len() == 0 {
                    /*diagnostics.push(Diagnostic::new(
                            FileMgr::textRange_to_temporary_Range(&expr.range),
                            Some(DiagnosticSeverity::ERROR),
                            None,
                            Some(EXTENSION_NAME.to_string()),
                            format!("{} is unknown on {}", expr.attr.as_str(), base.upgrade().unwrap().borrow().name),
                            None,
                            None,
                    ));*/
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                res.symbol = Rc::downgrade(attribute.first().unwrap());
                res.instance = (**attribute.first().unwrap()).borrow().sym_type == SymType::VARIABLE;
            },
            ExprOrIdent::Expr(Expr::Name(_)) | ExprOrIdent::Ident(_) => {
                let mut infered_sym: Option<Rc<RefCell<Symbol>>> = match ast {
                    ExprOrIdent::Expr(Expr::Name(expr))  =>  {
                        Symbol::infer_name(odoo, & parent, & expr.id.to_string(), Some( * max_infer))
                    },
                    ExprOrIdent::Ident(expr) => {
                        Symbol::infer_name(odoo, & parent, & expr.id.to_string(), Some( * max_infer))
                    }
                    _ => {
                        unreachable!();
                    }
                };

                if infered_sym.is_none() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }

                if infered_sym.as_ref().unwrap().borrow().parent.is_none() {
                    //for temporary symbol, store it in internal storage
                    res._internal_hold_symbol = Some(infered_sym.as_ref().unwrap().clone());
                }
                res.symbol = Rc::downgrade(infered_sym.as_ref().unwrap());
                let infered_sym = infered_sym.as_ref().unwrap().borrow();
                res.instance = infered_sym.sym_type != SymType::CLASS;
                if infered_sym.evaluation.is_some() {
                    res.instance = infered_sym.evaluation.as_ref().unwrap().symbol.instance;
                }
            },
            ExprOrIdent::Expr(Expr::Subscript(sub)) => {
                let (eval_left, diags) = Evaluation::eval_from_ast(odoo, &sub.value, parent.clone(), max_infer);
                diagnostics.extend(diags);
                if eval_left.is_none() || eval_left.as_ref().unwrap().symbol.symbol.upgrade().is_none() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base = eval_left.unwrap().symbol.symbol.upgrade();
                let base = base.unwrap();
                let (base, _) = Symbol::follow_ref(base, odoo, &mut None, false, false, &mut diagnostics);
                let base = base.upgrade().unwrap();
                let value = Evaluation::expr_to_str(odoo, &sub.slice, parent.clone(), max_infer, &mut diagnostics);
                let base = base.borrow();
                diagnostics.extend(value.1);
                if let Some(value) = value.0 {
                    let get_item = base.get_symbol(&(vec![], vec![S!("__getitem__")]));
                    if let Some(get_item) = get_item {
                        let get_item = get_item.borrow();
                        if let Some(get_item_eval) = &get_item.evaluation {
                            if let Some(hook) = get_item_eval.symbol.get_symbol_hook {
                                context.insert(S!("args"), ContextValue::STRING(value));
                                let old_range = context.remove(&S!("range"));
                                context.insert(S!("range"), ContextValue::RANGE(sub.slice.range()));
                                let mut ctxt = Some(context);
                                let hook_result = hook(odoo, &get_item_eval.symbol, &mut ctxt, &mut diagnostics);
                                if !hook_result.0.is_expired() {
                                    res.symbol = hook_result.0;
                                    res.instance = hook_result.1;
                                }
                                context = ctxt.unwrap();
                                context.remove(&S!("args"));
                                context.insert(S!("range"), old_range.unwrap());
                            }
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::BinOp(operator)) => {
                match operator.op {
                    Operator::Add => {

                    },
                    _ => {}
                }
            }
            _ => {}
        }
        AnalyzeAstResult { symbol: Some(Evaluation {
            symbol: res,
            value: None,
        }), effective_sym, factory, context: Some(context), diagnostics }
    }
}

impl EvaluationSymbol {

    pub fn new(symbol: Weak<RefCell<Symbol>>, instance: bool, context: Context, _internal_hold_symbol: Option<Rc<RefCell<Symbol>>>, factory: Option<Weak<RefCell<Symbol>>>, get_symbol_hook: Option<GetSymbolHook>) -> Self {
        Self { symbol, instance, context, _internal_hold_symbol, factory, get_symbol_hook }
    }
    
    pub fn new_with_symbol(symbol: Symbol, instance: bool, context: Context) -> EvaluationSymbol {
        let sym = Rc::new(RefCell::new(symbol));
        EvaluationSymbol {
            symbol: Rc::downgrade(&sym),
            instance: instance,
            context: context,
            _internal_hold_symbol: Some(sym),
            factory: None,
            get_symbol_hook: None
        }
    }

    pub fn get_symbol(&self, odoo:&mut SyncOdoo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (Weak<RefCell<Symbol>>, bool) {
        if self.get_symbol_hook.is_some() {
            let hook = self.get_symbol_hook.unwrap();
            return hook(odoo, self, context, diagnostics);
        }
        (self.symbol.clone(), self.instance)
    }
}
