use ruff_python_ast::{Identifier, Expr, Operator};
use ruff_text_size::{Ranged, TextRange, TextSize};
use lsp_types::Diagnostic;
use tracing::{debug, error};
use weak_table::traits::WeakElement;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::constants::*;
use crate::core::odoo::SyncOdoo;
use crate::threads::SessionInfo;
use crate::S;

use super::python_validator::PythonValidator;
use super::symbols::symbol::Symbol;
use super::symbols::symbol_mgr::SectionIndex;

#[derive(Debug, Clone)]
pub enum EvaluationValue {
    ANY(), //we don't know what it is, so it can be everything !
    CONSTANT(ruff_python_ast::Expr), //expr is a literal
    DICT(Vec<(ruff_python_ast::Expr, ruff_python_ast::Expr)>), //expr is a literal
    LIST(Vec<ruff_python_ast::Expr>), //expr is a literal
    TUPLE(Vec<ruff_python_ast::Expr>) //expr is a literal
}

#[derive(Debug, Clone)]
pub struct Evaluation {
    //symbol lead to type evaluation, and value/range hold the evaluated value in case of a 'constant' value, like in "variable = 5".
    pub symbol: EvaluationSymbol,
    pub value: Option<EvaluationValue>,
    pub range: Option<TextRange>,
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
            ExprOrIdent::Ident(_) => {
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
    SYMBOL(Rc<RefCell<Symbol>>),
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

    pub fn as_symbol(&self) -> Rc<RefCell<Symbol>> {
        match self {
            ContextValue::SYMBOL(s) => s.clone(),
            _ => panic!("Not a symbol")
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

type GetSymbolHook = fn (session: &mut SessionInfo, eval: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (Weak<RefCell<Symbol>>, bool);

#[derive(Debug, Default, Clone)]
pub struct EvaluationSymbol {
    pub symbol: Weak<RefCell<Symbol>>,
    pub instance: bool,
    pub context: Context,
    pub factory: Option<Weak<RefCell<Symbol>>>,
    pub get_symbol_hook: Option<GetSymbolHook>,
}

#[derive(Default)]
pub struct AnalyzeAstResult {
    pub evaluations: Vec<Evaluation>,
    pub effective_sym: Option<Weak<RefCell<Symbol>>>,
    pub factory: Option<Weak<RefCell<Symbol>>>,
    pub context: Option<Context>,
    pub diagnostics: Vec<Diagnostic>
}

impl AnalyzeAstResult {
    pub fn from_only_diagnostics(diags: Vec<Diagnostic>) -> Self {
        AnalyzeAstResult { evaluations: vec![], effective_sym: None, factory: None, context: None, diagnostics: diags }
    }
}

impl Evaluation {

    pub fn new_list(odoo: &mut SyncOdoo, values: Vec<Expr>, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                symbol: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("list")]), u32::MAX).last().expect("builtins list not found")),
                instance: true,
                context: HashMap::new(),
                factory: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::LIST(values)),
            range: Some(range),
        }
    }

    pub fn new_tuple(odoo: &mut SyncOdoo, values: Vec<Expr>, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                symbol: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("tuple")]), u32::MAX).last().expect("builtins list not found")),
                instance: true,
                context: HashMap::new(),
                factory: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::TUPLE(values)),
            range: Some(range)
        }
    }

    pub fn new_dict(odoo: &mut SyncOdoo, values: Vec<(Expr, Expr)>, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                symbol: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("dict")]), u32::MAX).last().expect("builtins list not found")),
                instance: true,
                context: HashMap::new(),
                factory: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::DICT(values)),
            range: Some(range)
        }
    }

    pub fn new_constant(odoo: &mut SyncOdoo, values: Expr, range: TextRange) -> Evaluation {
        let tree_value = match &values {
            Expr::StringLiteral(_s) => {
                (vec![S!("builtins")], vec![S!("str")])
            },
            Expr::BooleanLiteral(_b) => {
                (vec![S!("builtins")], vec![S!("bool")])
            },
            Expr::NumberLiteral(_n) => {
                (vec![S!("builtins")], vec![S!("int")]) //TODO
            },
            Expr::BytesLiteral(_b) => {
                (vec![S!("builtins")], vec![S!("bytes")])
            }
            _ => {(vec![S!("builtins")], vec![S!("object")])}
        };
        let symbol;
        if !values.is_none_literal_expr() {
            symbol = Rc::downgrade(&odoo.get_symbol(&tree_value, u32::MAX).last().expect("builtins class not found"));
        } else {
            symbol = Weak::new();
        }
        Evaluation {
            symbol: EvaluationSymbol {
                symbol: symbol,
                instance: true,
                context: HashMap::new(),
                factory: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::CONSTANT(values)),
            range: Some(range)
        }
    }

    pub fn follow_ref_and_get_value(&self, session: &mut SessionInfo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> Option<EvaluationValue> {
        if self.value.is_some() {
            Some(self.value.as_ref().unwrap().clone())
        } else {
            let symbol = self.symbol.get_symbol(session, context, diagnostics).0;
            let evals = Symbol::follow_ref(&symbol.upgrade().unwrap(), session, context, false, true, diagnostics);
            if evals.len() == 1 {
                let eval = &evals[0];
                let eval_sym = eval.0.upgrade();
                if let Some(eval_sym) = eval_sym {
                    if eval_sym.borrow().evaluations().is_some() && eval_sym.borrow().evaluations().unwrap().len() == 1 {
                        let eval_borrowed = eval_sym.borrow();
                        let eval = &eval_borrowed.evaluations().unwrap()[0];
                        if eval.value.is_some() {
                            return Some(eval.value.as_ref().unwrap().clone());
                        }
                    }
                }
            }
            None
        }
    }

    ///Return a list of evaluations of the symbol that hold these sections.
    ///For example:
    /// if X:
    ///     i=5
    /// else:
    ///     i="test"
    /// It will return two evaluation for i, one with 5 and one for "test"
    pub fn from_sections(parent: &Symbol, sections: &HashMap<u32, Vec<Rc<RefCell<Symbol>>>>) -> Vec<Evaluation> {
        let mut res = vec![];
        let section = parent.as_symbol_mgr().get_section_for(u32::MAX);
        let syms = parent.as_symbol_mgr()._get_loc_symbol(sections, u32::MAX, &SectionIndex::INDEX(section.index), &mut vec![]);
        for sym in syms {
            res.push(Evaluation::eval_from_symbol(&Rc::downgrade(&sym)));
        }
        res
    }

    //create an evaluation that is evaluating to the given symbol
    pub fn eval_from_symbol(symbol: &Weak<RefCell<Symbol>>) -> Evaluation{
        let mut instance = false;
        if symbol.upgrade().unwrap().borrow().typ() == SymType::VARIABLE {
            instance = true;
        }
        Evaluation {
            symbol: EvaluationSymbol {symbol: symbol.clone(),
                instance: instance,
                context: HashMap::new(),
                factory: None,
                get_symbol_hook: None
            },
            value: None,
            range: None
        }
    }

    //Build evaluations from an ast node that can be associated to a LocalizedSymbol
    //For example: a = "5"
    // eval_from_ast should be called on '"5"' to build the evaluation of 'a'
    //The result is a list, because some ast can give various possible results. For example: a = func()
    pub fn eval_from_ast(session: &mut SessionInfo, ast: &Expr, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize) -> (Vec<Evaluation>, Vec<Diagnostic>) {
        let analyze_result = Evaluation::analyze_ast(session, &ExprOrIdent::Expr(ast), parent, max_infer);
        return (analyze_result.evaluations, analyze_result.diagnostics)
    }

    /* Given an Expr, try to return the represented String. None if it can't be achieved */
    fn expr_to_str(session: &mut SessionInfo, ast: &Expr, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize, diagnostics: &mut Vec<Diagnostic>) -> (Option<String>, Vec<Diagnostic>) {
        let value = Evaluation::analyze_ast(session, &ExprOrIdent::Expr(ast), parent, max_infer);
        if value.evaluations.len() == 1 { //only handle strict evaluations
            let eval = &value.evaluations[0];
            let v = eval.follow_ref_and_get_value(session, &mut None, diagnostics);
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
    pub fn analyze_ast(session: &mut SessionInfo, ast: &ExprOrIdent, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize) -> AnalyzeAstResult {
        let odoo = &mut session.sync_odoo;
        let mut evals = vec![];
        let effective_sym = None;
        let factory = None;
        let mut diagnostics = vec![];
        let from_module;
        if let Some(module) = parent.borrow().find_module() {
            from_module = ContextValue::MODULE(module);
        } else {
            from_module = ContextValue::BOOLEAN(false);
        }
        let module: Option<Rc<RefCell<Symbol>>> = parent.borrow().find_module();
        let mut context: Context = HashMap::from([
            (S!("module"), from_module),
            (S!("range"), ContextValue::RANGE(ast.range()))
        ]);

        match ast {
            ExprOrIdent::Expr(Expr::StringLiteral(expr)) => {
                evals.push(Evaluation::new_constant(odoo, ast.expr().clone(), expr.range));
            },
            ExprOrIdent::Expr(Expr::BytesLiteral(expr)) => {
                evals.push(Evaluation::new_constant(odoo, ast.expr().clone(), expr.range));
            },
            ExprOrIdent::Expr(Expr::NumberLiteral(expr)) => {
                evals.push(Evaluation::new_constant(odoo, ast.expr().clone(), expr.range));
            },
            ExprOrIdent::Expr(Expr::BooleanLiteral(expr)) => {
                evals.push(Evaluation::new_constant(odoo, ast.expr().clone(), expr.range));
            },
            ExprOrIdent::Expr(Expr::NoneLiteral(expr)) => {
                evals.push(Evaluation::new_constant(odoo, ast.expr().clone(), expr.range));
            },
            ExprOrIdent::Expr(Expr::EllipsisLiteral(expr)) => {
                evals.push(Evaluation::new_constant(odoo, ast.expr().clone(), expr.range));
            }
            ExprOrIdent::Expr(Expr::List(expr)) => {
                let mut values: Vec<ruff_python_ast::Expr> = Vec::new();
                for e in expr.elts.iter() {
                    if e.is_literal_expr() {
                        values.push(e.clone());
                    } else {
                        values = Vec::new(); break;
                    }
                }
                evals.push(Evaluation::new_list(odoo, values, expr.range));
            },
            ExprOrIdent::Expr(Expr::Tuple(expr)) => {
                let mut values: Vec<ruff_python_ast::Expr> = Vec::new();
                for e in expr.elts.iter() {
                    if e.is_literal_expr() {
                        values.push(e.clone());
                    } else {
                        values = Vec::new(); break;
                    }
                }
                evals.push(Evaluation::new_tuple(odoo, values, expr.range));
            },
            ExprOrIdent::Expr(Expr::Dict(expr)) => {
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
                evals.push(Evaluation::new_dict(odoo, values, expr.range));
            },
            ExprOrIdent::Expr(Expr::Call(expr)) => {
                let (base_eval, diags) = Evaluation::eval_from_ast(session, &expr.func, parent, max_infer);
                diagnostics.extend(diags);
                //TODO actually we only evaluate if there is only one function behind the evaluation.
                // we could evaluate the result of each function and filter results by signature matching.
                /* example:

                def test():
                    return "5"

                def other_test():
                    return 5

                b = input()
                if b:
                    a = test
                else:
                    a = other_test

                print(a)

                c = a()

                print(c) <= string/int with value 5. if we had a parameter to 'other_test', only string with value 5
                */
                if base_eval.len() != 1 {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let (base_sym_ref, instance) = base_eval[0].symbol.get_symbol(session, &mut None, &mut diagnostics);
                let base_sym = base_sym_ref.upgrade();
                if let Some(base_sym) = base_sym {
                    if base_sym.borrow().typ() == SymType::CLASS {
                        if instance {
                            //TODO handle call on class instance
                        } else {
                            //TODO diagnostic __new__ call parameters
                            evals.push(Evaluation{
                                symbol: EvaluationSymbol {
                                    symbol: base_sym_ref.clone(),
                                    instance: true,
                                    context: HashMap::new(),
                                    factory: None,
                                    get_symbol_hook: None,
                                },
                                value: None,
                                range: Some(expr.range)
                            });
                        }
                    } else if base_sym.borrow().typ() == SymType::FUNCTION {
                        //function return evaluation can come from:
                        //  - type annotation parsing (ARCH_EVAL step)
                        //  - documentation parsing (Arch_eval and VALIDATION step)
                        //  - function body inference (VALIDATION step)
                        // Therefore, the actual version of the algorithm will trigger build from the different steps if this one has already been reached.
                        // We don't want to launch validation step while Arch evaluating the code.
                        if base_sym.borrow().evaluations().is_some() && base_sym.borrow().evaluations().unwrap().len() == 0 {
                            if base_sym.borrow().get_file().as_ref().unwrap().upgrade().unwrap().borrow().build_status(BuildSteps::ODOO) == BuildStatus::DONE &&
                            base_sym.borrow().build_status(BuildSteps::VALIDATION) == BuildStatus::PENDING { //TODO update with new step validation to lower it to localized level
                                let mut v = PythonValidator::new(base_sym.clone());
                                v.validate(session);
                            }
                        }
                        if base_sym.borrow().evaluations().is_some() {
                            for eval in base_sym.borrow().evaluations().unwrap().iter() {
                                let mut e = eval.clone();
                                e.range = Some(expr.range.clone());
                                evals.push(e);
                            }
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::Attribute(expr)) => {
                let (base_evals, diags) = Evaluation::eval_from_ast(session, &expr.value, parent, max_infer);
                diagnostics.extend(diags);
                if base_evals.len() != 1 || base_evals[0].symbol.get_symbol(session, &mut None, &mut diagnostics).0.is_expired() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base_ref = base_evals[0].symbol.get_symbol(session, &mut None, &mut diagnostics).0;
                let bases = Symbol::follow_ref(&base_ref.upgrade().unwrap(), session, &mut None, false, false, &mut diagnostics);
                for ibase in bases.iter() {
                    let base_loc = ibase.0.upgrade();
                    if let Some(base_loc) = base_loc {
                        let attributes = base_loc.borrow().get_member_symbol(session, &expr.attr.to_string(), module.clone(), false, true, &mut diagnostics);
                        if !attributes.is_empty() {
                            evals.push(Evaluation::eval_from_symbol(&Rc::downgrade(attributes.first().unwrap())));
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::Name(_)) | ExprOrIdent::Ident(_) => {
                let infered_syms = match ast {
                    ExprOrIdent::Expr(Expr::Name(expr))  =>  {
                        Symbol::infer_name(odoo, & parent, & expr.id.to_string(), Some( max_infer.to_u32()))
                    },
                    ExprOrIdent::Ident(expr) => {
                        Symbol::infer_name(odoo, & parent, & expr.id.to_string(), Some( max_infer.to_u32()))
                    }
                    _ => {
                        unreachable!();
                    }
                };

                if infered_syms.is_empty() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                for infered_sym in infered_syms.iter() {
                    evals.push(Evaluation::eval_from_symbol(&Rc::downgrade(infered_sym)));
                }
            },
            ExprOrIdent::Expr(Expr::Subscript(sub)) => {
                let (eval_left, diags) = Evaluation::eval_from_ast(session, &sub.value, parent.clone(), max_infer);
                diagnostics.extend(diags);
                if eval_left.len() != 1 || eval_left[0].symbol.symbol.is_expired() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base = &eval_left[0].symbol.symbol;
                let bases = Symbol::follow_ref(&base.upgrade().unwrap(), session, &mut None, false, false, &mut diagnostics);
                if bases.len() != 1 {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base = &bases[0];
                let base = base.0.upgrade().unwrap();
                let value = Evaluation::expr_to_str(session, &sub.slice, parent.clone(), max_infer, &mut diagnostics);
                let base = base.borrow();
                diagnostics.extend(value.1);
                if let Some(value) = value.0 {
                    let get_item = base.get_content_symbol("__getitem__", u32::MAX);
                    if get_item.len() == 1 {
                        let get_item = &get_item[0];
                        let get_item = get_item.borrow();
                        if get_item.evaluations().is_some() && get_item.evaluations().unwrap().len() == 1 {
                            let get_item_eval = &get_item.evaluations().unwrap()[0];
                            if let Some(hook) = get_item_eval.symbol.get_symbol_hook {
                                context.insert(S!("args"), ContextValue::STRING(value));
                                let old_range = context.remove(&S!("range"));
                                context.insert(S!("range"), ContextValue::RANGE(sub.slice.range()));
                                let mut ctxt = Some(context);
                                let hook_result = hook(session, &get_item_eval.symbol, &mut ctxt, &mut diagnostics);
                                if !hook_result.0.is_expired() {
                                    evals.push(Evaluation::eval_from_symbol(&hook_result.0));
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
        AnalyzeAstResult { evaluations: evals, effective_sym, factory, context: Some(context), diagnostics }
    }
}

impl EvaluationSymbol {

    pub fn new(symbol: Weak<RefCell<Symbol>>, instance: bool, context: Context, factory: Option<Weak<RefCell<Symbol>>>, get_symbol_hook: Option<GetSymbolHook>) -> Self {
        Self { symbol, instance, context, factory, get_symbol_hook }
    }

    pub fn get_symbol(&self, session: &mut SessionInfo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> (Weak<RefCell<Symbol>>, bool) {
        if self.get_symbol_hook.is_some() {
            let hook = self.get_symbol_hook.unwrap();
            return hook(session, self, context, diagnostics);
        }
        (self.symbol.clone(), self.instance)
    }
}
