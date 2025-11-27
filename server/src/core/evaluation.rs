use itertools::Itertools;
use itertools::FoldWhile::{Continue, Done};
use ruff_python_ast::{Arguments, Expr, ExprCall, Identifier, Number, Operator, Parameter, UnaryOp};
use ruff_text_size::{Ranged, TextRange, TextSize};
use lsp_types::{Diagnostic, Position, Range};
use weak_table::traits::WeakElement;
use std::cmp::{max, min};
use std::collections::{HashMap, HashSet};
use std::i32;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::{constants::*, Sy};
use crate::core::odoo::SyncOdoo;
use crate::threads::SessionInfo;
use crate::S;

use super::file_mgr::FileMgr;
use super::python_validator::PythonValidator;
use super::symbols::function_symbol::{Argument, ArgumentType, FunctionSymbol};
use super::symbols::symbol::Symbol;
use super::symbols::symbol_mgr::SectionIndex;


#[derive(Debug, Clone, PartialEq)]
pub enum EvaluationValue {
    ANY(), //we don't know what it is, so it can be everything !
    CONSTANT(ruff_python_ast::Expr), //expr is a literal
    DICT(Vec<(ruff_python_ast::Expr, ruff_python_ast::Expr)>), //expr is a literal
    LIST(Vec<ruff_python_ast::Expr>), //expr is a literal
    TUPLE(Vec<ruff_python_ast::Expr>) //expr is a literal
}

impl EvaluationValue {
    pub fn as_any(&self) -> bool {
        match self {
            EvaluationValue::ANY() => true,
            _ => false
        }
    }

    pub fn as_constant(&self) -> &ruff_python_ast::Expr {
        match self {
            EvaluationValue::CONSTANT(e) => e,
            _ => panic!("Not a constant")
        }
    }

    pub fn as_dict(&self) -> &Vec<(ruff_python_ast::Expr, ruff_python_ast::Expr)> {
        match self {
            EvaluationValue::DICT(d) => d,
            _ => panic!("Not a dict")
        }
    }

    pub fn as_list(&self) -> &Vec<ruff_python_ast::Expr> {
        match self {
            EvaluationValue::LIST(l) => l,
            _ => panic!("Not a list")
        }
    }

    pub fn as_tuple(&self) -> &Vec<ruff_python_ast::Expr> {
        match self {
            EvaluationValue::TUPLE(t) => t,
            _ => panic!("Not a tuple")
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    //symbol lead to type evaluation, and value/range hold the evaluated value in case of a 'constant' value, like in "variable = 5".
    pub symbol: EvaluationSymbol,
    pub value: Option<EvaluationValue>, //
    pub range: Option<TextRange>, //evaluated part
}

#[derive(Debug)]
pub enum ExprOrIdent<'a> {
    Expr(&'a Expr),
    Ident(&'a Identifier),
    Parameter(&'a Parameter),
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
            ExprOrIdent::Parameter(p) => {
                p.range()
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
            },
            ExprOrIdent::Parameter(_) => {
                panic!("ExprOrIdent is not an expr")
            }
        }
    }

}

#[derive(Debug, Clone)]
pub enum ContextValue {
    BOOLEAN(bool),
    STRING(String),
    MODULE(Weak<RefCell<Symbol>>),
    SYMBOL(Weak<RefCell<Symbol>>),
    ARGUMENTS(Arguments),
    RANGE(TextRange)
}

impl PartialEq for ContextValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ContextValue::MODULE(me), ContextValue::MODULE(them)) => Symbol::weak_ptr_eq(me, them),
            (ContextValue::SYMBOL(me), ContextValue::SYMBOL(them)) => Symbol::weak_ptr_eq(me, them),
            (ContextValue::BOOLEAN(me), ContextValue::BOOLEAN(them)) => me == them,
            (ContextValue::STRING(me), ContextValue::STRING(them)) => me == them,
            (ContextValue::ARGUMENTS(me), ContextValue::ARGUMENTS(them)) => me == them,
            (ContextValue::RANGE(me), ContextValue::RANGE(them)) => me == them,
            _ => false,
        }
    }
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

    pub fn as_module(&self) -> Weak<RefCell<Symbol>> {
        match self {
            ContextValue::MODULE(m) => m.clone(),
            _ => panic!("Not a module")
        }
    }

    pub fn as_symbol(&self) -> Weak<RefCell<Symbol>> {
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

    pub fn as_arguments(&self) -> Arguments {
        match self {
            ContextValue::ARGUMENTS(a) => a.clone(),
            _ => panic!("Not an arguments")
        }
    }
}

/** A context can contains: (non-exhaustive)
* module: the current module the file belongs to
* parent: in an expression, like self.test, the parent is the base attribute, so 'self' for test
* object: the object the expression is executed on (useful if function is defined in parent object).
*/
pub type Context = HashMap<String, ContextValue>;

/**
 * A hook will receive:
 * session: current active session
 * eval: the evaluationSymbol the hook is executed on
 * context: if provided, can contains useful information
 * diagnostics: a vec the hook can fill to add diagnostics
 * file_symbol: if provided, can be used to add dependencies
 */
type GetSymbolHookCallable = fn (session: &mut SessionInfo, eval: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> Option<EvaluationSymbolPtr>;

#[derive(Debug, Clone)]
pub struct GetSymbolHook {
    pub callable: GetSymbolHookCallable,
    pub name: String
}

impl PartialEq for GetSymbolHook {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}


#[derive(Debug, Clone)]
pub struct EvaluationSymbolWeak {
    pub weak: Weak<RefCell<Symbol>>,
    pub context: Context,
    pub instance: Option<bool>,
    pub is_super: bool,
}

impl PartialEq for EvaluationSymbolWeak {
    fn eq(&self, other: &Self) -> bool {
        self.context == other.context
        && self.context == other.context
        && self.instance == other.instance
        && self.is_super == other.is_super
        && Symbol::weak_ptr_eq(&self.weak, &other.weak)
    }
}

impl EvaluationSymbolWeak {
    pub fn new(weak: Weak<RefCell<Symbol>>, instance: Option<bool>, is_super: bool) -> Self {
        EvaluationSymbolWeak {
            weak,
            context: HashMap::new(),
            instance,
            is_super
        }
    }

    pub fn is_instance(&self) -> Option<bool> {
        return self.instance;
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum EvaluationSymbolPtr {
    WEAK(EvaluationSymbolWeak),
    SELF,
    ARG(u32),
    DOMAIN,
    NONE,
    UNBOUND(OYarn),
    #[default]
    ANY
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct EvaluationSymbol {
    sym: EvaluationSymbolPtr,
    pub get_symbol_hook: Option<GetSymbolHook>,
}

#[derive(Default)]
pub struct AnalyzeAstResult {
    pub evaluations: Vec<Evaluation>,
    pub diagnostics: Vec<Diagnostic>
}

impl AnalyzeAstResult {
    pub fn from_only_diagnostics(diags: Vec<Diagnostic>) -> Self {
        AnalyzeAstResult { evaluations: vec![], diagnostics: diags }
    }
}

impl Evaluation {

    pub fn new_list(odoo: &mut SyncOdoo, values: Vec<Expr>, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: Rc::downgrade(&odoo.get_symbol("", &(vec![Sy!("builtins")], vec![Sy!("list")]), u32::MAX).last().expect("builtins list not found")),
                    context: HashMap::new(),
                    instance: Some(true),
                    is_super: false,
                }),
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::LIST(values)),
            range: Some(range),
        }
    }

    pub fn new_tuple(odoo: &mut SyncOdoo, values: Vec<Expr>, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: Rc::downgrade(&odoo.get_symbol("", &(vec![Sy!("builtins")], vec![Sy!("tuple")]), u32::MAX).last().expect("builtins list not found")),
                    context: HashMap::new(),
                    instance: Some(true),
                    is_super: false,
                }),
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::TUPLE(values)),
            range: Some(range)
        }
    }

    pub fn new_dict(odoo: &mut SyncOdoo, values: Vec<(Expr, Expr)>, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: Rc::downgrade(&odoo.get_symbol("", &(vec![Sy!("builtins")], vec![Sy!("dict")]), u32::MAX).last().expect("builtins list not found")),
                    context: HashMap::new(),
                    instance: Some(true),
                    is_super: false,
                }),
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::DICT(values)),
            range: Some(range)
        }
    }

    pub fn new_set(odoo:&mut SyncOdoo, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: Rc::downgrade(&odoo.get_symbol("", &(vec![Sy!("builtins")], vec![Sy!("set")]), u32::MAX).last().expect("builtins set not found")),
                    context: HashMap::new(),
                    instance: Some(true),
                    is_super: false,
                }),
                get_symbol_hook: None
            },
            value: None,
            range: Some(range)
        }
    }

    pub fn new_domain(_odoo: &mut SyncOdoo) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::DOMAIN,
                get_symbol_hook: None
            },
            value: None,
            range: None
        }
    }

    pub fn new_constant(odoo: &mut SyncOdoo, values: Expr, range: TextRange) -> Evaluation {
        let tree_value = match &values {
            Expr::StringLiteral(_s) => {
                (vec![Sy!("builtins")], vec![Sy!("str")])
            },
            Expr::BooleanLiteral(_b) => {
                (vec![Sy!("builtins")], vec![Sy!("bool")])
            },
            Expr::NumberLiteral(_n) => {
                match _n.value {
                    Number::Float(_) => (vec![Sy!("builtins")], vec![Sy!("float")]),
                    Number::Int(_) => (vec![Sy!("builtins")], vec![Sy!("int")]),
                    Number::Complex { .. } => (vec![Sy!("builtins")], vec![Sy!("complex")]),
                }
            },
            Expr::BytesLiteral(_b) => {
                (vec![Sy!("builtins")], vec![Sy!("bytes")])
            },
            Expr::EllipsisLiteral(_e) => {
                (vec![Sy!("builtins")], vec![Sy!("Ellipsis")])
            },
            Expr::NoneLiteral(_n) => {
                let mut eval = Evaluation::new_none();
                eval.range = Some(range);
                eval.value = Some(EvaluationValue::CONSTANT(values));
                return eval
            }
            _ => {(vec![Sy!("builtins")], vec![Sy!("object")])}
        };
        let symbol;
        if !values.is_none_literal_expr() {
            symbol = Rc::downgrade(&odoo.get_symbol("", &tree_value, u32::MAX).last().expect("builtins class not found"));
        } else {
            symbol = Weak::new();
        }
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: symbol,
                    context: HashMap::new(),
                    instance: Some(true),
                    is_super: false,
                }),
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::CONSTANT(values)),
            range: Some(range)
        }
    }

    pub fn new_none() -> Self {
        Self {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::NONE,
                get_symbol_hook: None,
            },
            value: None,
            range: None
        }
    }

    pub fn new_self() -> Self {
        Self {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::SELF,
                get_symbol_hook: None,
            },
            value: None,
            range: None
        }
    }

    pub fn new_any() -> Self {
        Self {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::ANY,
                get_symbol_hook: None,
            },
            value: None,
            range: None
        }
    }
    pub fn new_unbound(name: String) -> Self {
        Self {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::UNBOUND(Sy!(name)),
                get_symbol_hook: None,
            },
            value: None,
            range: None
        }
    }
    ///return the evaluation but valid outside of the given function scope
    pub fn get_eval_out_of_function_scope(&self, session: &mut SessionInfo, function: &Rc<RefCell<Symbol>>) -> Vec<Evaluation> {
        let mut res = vec![];
        match self.symbol.sym {
            EvaluationSymbolPtr::WEAK(_) => {
                //take the weak by get_symbol instead of the match
                let symbol_eval = self.symbol.get_symbol(session, &mut None, &mut vec![], Some(function.clone()));
                let out_of_scope = Symbol::follow_ref(&symbol_eval, session, &mut None, false, false, None, Some(function.clone()));
                for sym in out_of_scope {
                    if !sym.is_expired_if_weak() {
                        res.push(Evaluation {
                            symbol: EvaluationSymbol {
                                sym: sym,
                                get_symbol_hook: None,
                            },
                            value: None,
                            range: None
                        })
                    }
                }
            },
            _ => {
                res.push(self.clone());
            },
        }
        res
    }

    pub fn follow_ref_and_get_value(&self, session: &mut SessionInfo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> Option<EvaluationValue> {
        if self.value.is_some() {
            Some(self.value.as_ref().unwrap().clone())
        } else {
            let eval_symbol = self.symbol.get_symbol(session, &mut None, diagnostics, None);
            if eval_symbol.is_expired_if_weak() {
                return None;
            }
            let evals = Symbol::follow_ref(&eval_symbol, session, context, false, true, None, None);
            if evals.len() == 1 {
                let eval = &evals[0];
                match eval {
                    EvaluationSymbolPtr::WEAK(w) => {
                        let eval_sym = w.weak.upgrade();
                        if let Some(eval_sym) = eval_sym {
                            if eval_sym.borrow().evaluations().is_some() && eval_sym.borrow().evaluations().unwrap().len() == 1 {
                                let eval_borrowed = eval_sym.borrow();
                                let eval = &eval_borrowed.evaluations().unwrap()[0];
                                if eval.value.is_some() {
                                    return Some(eval.value.as_ref().unwrap().clone());
                                }
                            }
                        }
                    },
                    _ => {}
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
        let content_symbols = parent.as_symbol_mgr()._get_loc_symbol(sections, u32::MAX, &SectionIndex::INDEX(section.index), &mut HashSet::new());
        for sym in content_symbols.symbols {
            let mut is_instance = None;
            if matches!(sym.borrow().typ(), SymType::VARIABLE | SymType::FUNCTION) {
                for eval in sym.borrow().evaluations().unwrap().iter() {
                    match eval.symbol.is_instance() {
                        Some(instance) => {
                            if is_instance.is_some() && is_instance.unwrap() != instance {
                                is_instance = None;
                                break;
                            }
                            is_instance = Some(instance);
                        },
                        None => {is_instance = None; continue},
                    }
                }
            } else if matches!(sym.borrow().typ(), SymType::CLASS) {
                is_instance = Some(false);
            }
            res.push(Evaluation::eval_from_symbol(&Rc::downgrade(&sym), is_instance));
        }
        res
    }

    /// Create an evaluation that is evaluating to the given symbol
    pub fn eval_from_symbol(symbol: &Weak<RefCell<Symbol>>, instance: Option<bool>) -> Evaluation{
        if symbol.is_expired() {
            return Evaluation::new_none();
        }
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: symbol.clone(),
                    context: HashMap::new(),
                    instance: instance,
                    is_super: false,
                }),
                get_symbol_hook: None
            },
            value: None,
            range: None
        }
    }

    pub fn eval_from_ptr(ptr: &EvaluationSymbolPtr) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: ptr.clone(),
                get_symbol_hook: None
            },
            value: None,
            range: None
        }
    }

    /** Build evaluations from an ast node that can be associated to a LocalizedSymbol
    * For example: a = "5"
    *  eval_from_ast should be called on '"5"' to build the evaluation of 'a'
    * The result is a list, because some ast can give various possible results. For example: a = func()
    * required_dependencies will be filled with dependencies required to build the value, step by step.
    * You have to provide a vector with the length matching the available steps. For example, in arch_eval, required_dependencies
    * should be equal to vec![vec![], vec![]] to be able to get arch and arch_eval deps at index 0 and 1. It means that if validation is 
    * not build but required during the eval_from_ast, it will NOT be built
    */
    pub fn eval_from_ast(session: &mut SessionInfo, ast: &Expr, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize, for_annotation: bool, required_dependencies: &mut Vec<Vec<Rc<RefCell<Symbol>>>>) -> (Vec<Evaluation>, Vec<Diagnostic>) {
        let from_module;
        if let Some(module) = parent.borrow().find_module() {
            from_module = ContextValue::MODULE(Rc::downgrade(&module));
        } else {
            from_module = ContextValue::BOOLEAN(false);
        }
        let mut context: Option<Context> = Some(HashMap::from([
            (S!("module"), from_module),
            (S!("range"), ContextValue::RANGE(ast.range()))
        ]));
        let analyze_result = Evaluation::analyze_ast(session, &ExprOrIdent::Expr(ast), parent, max_infer, &mut context, for_annotation, required_dependencies);
        return (analyze_result.evaluations, analyze_result.diagnostics)
    }

    /* Given an Expr, try to return the represented String. None if it can't be achieved */
    pub fn expr_to_str(session: &mut SessionInfo, ast: &Expr, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize, for_annotation: bool, diagnostics: &mut Vec<Diagnostic>) -> (Option<String>, Vec<Diagnostic>) {
        let from_module;
        if let Some(module) = parent.borrow().find_module() {
            from_module = ContextValue::MODULE(Rc::downgrade(&module));
        } else {
            from_module = ContextValue::BOOLEAN(false);
        }
        let mut context: Option<Context> = Some(HashMap::from([
            (S!("module"), from_module),
            (S!("range"), ContextValue::RANGE(ast.range()))
        ]));
        let value = Evaluation::analyze_ast(session, &ExprOrIdent::Expr(ast), parent, max_infer, &mut context, for_annotation, &mut vec![]);
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

    /* Given an Expr, try to return the represented Boolean. None if it can't be achieved */
    pub fn expr_to_bool(session: &mut SessionInfo, ast: &Expr, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize, for_annotation: bool, diagnostics: &mut Vec<Diagnostic>) -> (Option<bool>, Vec<Diagnostic>) {
        let from_module;
        if let Some(module) = parent.borrow().find_module() {
            from_module = ContextValue::MODULE(Rc::downgrade(&module));
        } else {
            from_module = ContextValue::BOOLEAN(false);
        }
        let mut context: Option<Context> = Some(HashMap::from([
            (S!("module"), from_module),
            (S!("range"), ContextValue::RANGE(ast.range()))
        ]));
        let value = Evaluation::analyze_ast(session, &ExprOrIdent::Expr(ast), parent, max_infer, &mut context, for_annotation, &mut vec![]);
        if value.evaluations.len() == 1 { //only handle strict evaluations
            let eval = &value.evaluations[0];
            let v = eval.follow_ref_and_get_value(session, &mut None, diagnostics);
            if let Some(v) = v {
                match v {
                    EvaluationValue::CONSTANT(v) => {
                        match v {
                            Expr::BooleanLiteral(s) => {
                                return (Some(s.value), value.diagnostics);
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


    /**
    analyze_ast will extract all known information about an ast:
    result.0: the direct evaluation
    result.3: the context after the evaluation. Can't be None
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
        context: {}
        diagnostics: vec![]
     */
    pub fn analyze_ast(session: &mut SessionInfo, ast: &ExprOrIdent, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize, context: &mut Option<Context>, for_annotation: bool, required_dependencies: &mut Vec<Vec<Rc<RefCell<Symbol>>>>) -> AnalyzeAstResult {
        let odoo = &mut session.sync_odoo;
        let mut evals = vec![];
        let mut diagnostics = vec![];
        let module = parent.borrow().find_module();

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
            ExprOrIdent::Expr(Expr::Set(expr)) => {
                evals.push(Evaluation::new_set(odoo, expr.range))
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
                let (base_evals, diags) = Evaluation::eval_from_ast(session, &expr.func, parent.clone(), max_infer, false, required_dependencies);
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
                if base_evals.len() == 0 {
                    /*TODO if multiple evals are found, we could maybe try to validate that they all have the same signature in case of diamond inheritance?
                    However, other cases should be handled by arch step or syntax? */
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base_eval_ptrs: Vec<EvaluationSymbolPtr> = base_evals.iter().map(|base_eval| {
                    let base_sym_weak_eval_base = base_eval.symbol.get_symbol_weak_transformed(session, context, &mut diagnostics, None);
                    Symbol::follow_ref(&base_sym_weak_eval_base, session, context, true, false, None, None)
                }).flatten().collect();

                let parent_file_or_func = parent.clone().borrow().parent_file_or_function().as_ref().unwrap().upgrade().unwrap();
                let is_in_validation = match parent_file_or_func.borrow().typ().clone() {
                    SymType::FILE | SymType::PACKAGE(_) | SymType::FUNCTION => {
                        parent_file_or_func.borrow().build_status(BuildSteps::VALIDATION) == BuildStatus::IN_PROGRESS
                    },
                    _ => {false}
                };

                let mut call_argument_diagnostics = Vec::new();
                for base_eval_ptr in base_eval_ptrs.iter() {
                    call_argument_diagnostics.push(Vec::new()); //one list per evaluation
                    let EvaluationSymbolPtr::WEAK(base_sym_weak_eval) = base_eval_ptr else {continue};
                    let Some(base_sym) = base_sym_weak_eval.weak.upgrade() else {continue};
                    if base_sym.borrow().typ() == SymType::CLASS {
                        if base_sym_weak_eval.instance.unwrap_or(false) {
                            //TODO handle call on class instance
                        } else {
                            if base_sym.borrow().match_tree_from_any_entry(session, &(vec![Sy!("builtins")], vec![Sy!("super")])){
                                //  - If 1st argument exists, we add that class with symbol_type Super
                                let super_class = if !expr.arguments.is_empty(){
                                    let (class_eval, diags) = Evaluation::eval_from_ast(session, &expr.arguments.args[0], parent.clone(), max_infer, false, required_dependencies);
                                    diagnostics.extend(diags);
                                    if class_eval.len() != 1 {
                                        return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                                    }
                                    let class_sym_weak_eval= class_eval[0].symbol.get_symbol_as_weak(session, context, &mut diagnostics, None);
                                    let res = class_sym_weak_eval.weak.upgrade().and_then(|class_sym|{
                                        let class_sym_weak_eval = &Symbol::follow_ref(&&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
                                            Rc::downgrade(&class_sym), None, false
                                        )), session, &mut None, false, false, None, None)[0];
                                        if class_sym_weak_eval.upgrade_weak().unwrap().borrow().typ() != SymType::CLASS{
                                            return None;
                                        }
                                        let class_sym_weak_eval = class_sym_weak_eval.as_weak();
                                        if class_sym_weak_eval.instance.unwrap_or(false) {
                                            if let Some(diagnostic_base) = create_diagnostic(&session, DiagnosticCode::OLS01005, &[]){
                                                diagnostics.push(Diagnostic {
                                                    range: Range::new(Position::new(expr.arguments.args[0].range().start().to_u32(), 0),
                                                    Position::new(expr.arguments.args[0].range().end().to_u32(), 0)),
                                                    ..diagnostic_base
                                                });
                                            }
                                            None
                                        } else {
                                            let mut is_instance = None;
                                            let mut default_instance = true; //used if we can't evaluate the instance parameter
                                            if parent.borrow().typ() == SymType::FUNCTION && parent.borrow().as_func().is_class_method {
                                                default_instance = false;
                                            }
                                            if expr.arguments.args.len() >= 2 {
                                                let (object_or_type_eval, diags) = Evaluation::eval_from_ast(session, &expr.arguments.args[1], parent.clone(), max_infer, false, required_dependencies);
                                                diagnostics.extend(diags);
                                                if object_or_type_eval.len() != 1 {
                                                    return Some((class_sym_weak_eval.weak.clone(), Some(default_instance)))
                                                }
                                                let object_or_type_weak_eval = &Symbol::follow_ref(
                                                    &object_or_type_eval[0].symbol.get_symbol(
                                                        session, context, &mut diagnostics, Some(parent.clone())),
                                                        session, &mut None, false, false, None, None)[0];
                                                if object_or_type_weak_eval.is_weak() {
                                                    is_instance = Some(object_or_type_weak_eval.as_weak().instance.unwrap_or(default_instance));
                                                } else {
                                                    is_instance = Some(default_instance);
                                                }
                                            }
                                            Some((class_sym_weak_eval.weak.clone(), is_instance))
                                        }
                                    });
                                    res
                                //  - Otherwise we get the encapsulating class
                                } else {
                                    match parent.borrow().get_in_parents(&vec![SymType::CLASS], true){
                                        None => {
                                            if let Some(diagnostic) = create_diagnostic(&session, DiagnosticCode::OLS01006, &[]) {
                                                diagnostics.push(Diagnostic {
                                                    range: Range::new(Position::new(expr.range().start().to_u32(), 0),
                                                    Position::new(expr.range().end().to_u32(), 0)),
                                                    ..diagnostic
                                                });
                                            }
                                            None
                                        },
                                        Some(parent_class) => {
                                            let mut instance = Some(true);
                                            if parent.borrow().typ() == SymType::FUNCTION {
                                                if parent.borrow().as_func().is_class_method {
                                                    instance = Some(false);
                                                }
                                                if parent.borrow().as_func().is_static {
                                                    instance = None;
                                                }
                                            }
                                            Some((parent_class.clone(), instance))
                                        }
                                    }
                                };
                                if let Some((super_class, instance)) = super_class{
                                    evals.push(Evaluation{
                                        symbol: EvaluationSymbol {
                                            sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                                                weak: super_class,
                                                context: HashMap::new(),
                                                instance,
                                                is_super: true,
                                            }),
                                            get_symbol_hook: None,
                                        },
                                        value: None,
                                        range: Some(expr.range)
                                    });
                                }
                            } else {
                                //let be sure that the class file has been loaded, and add dependency to it
                                if required_dependencies.len() >= 2 {
                                    let class_file = base_sym.borrow().get_file().unwrap().upgrade().unwrap();
                                    SyncOdoo::build_now(session, &class_file, BuildSteps::ARCH_EVAL);
                                    if !class_file.borrow().is_external() {
                                        required_dependencies[1].push(class_file.clone());
                                    }
                                }
                                //1: find __init__ method
                                let init = base_sym.borrow().get_member_symbol(session, &S!("__init__"), module.clone(), true, false, false, false, false);
                                let mut found_hook = false;
                                if let Some(init) = init.0.first() {
                                    let init_sym_file = init.borrow().get_file().as_ref().unwrap().upgrade().unwrap().clone();
                                    if init.borrow().evaluations().is_some()
                                    && init.borrow().evaluations().unwrap().len() == 0
                                    && !init_sym_file.borrow().is_external()
                                    && init_sym_file.borrow().build_status(BuildSteps::ARCH_EVAL) == BuildStatus::DONE
                                    && init.borrow().build_status(BuildSteps::ARCH) != BuildStatus::IN_PROGRESS
                                    && init.borrow().build_status(BuildSteps::ARCH_EVAL) != BuildStatus::IN_PROGRESS
                                    && init.borrow().build_status(BuildSteps::VALIDATION) == BuildStatus::PENDING {
                                        let mut v = PythonValidator::new(init.borrow().get_entry().unwrap(), init.clone());
                                        v.validate(session);
                                    }
                                    if let Some(init_eval) = init.borrow().evaluations() {
                                        //init will always return an instance of the class, so we are not searching the method to check its return type, but rather to check if there is 
                                        //an hook on it. Hooks, can be used to use parameters for context (see relational fields for example).
                                        if init_eval.len() == 1 && init_eval[0].symbol.get_symbol_hook.is_some() {
                                            context.as_mut().unwrap().insert(S!("constructing_class"), ContextValue::SYMBOL(Rc::downgrade(&base_sym)));
                                            context.as_mut().unwrap().insert(S!("parameters"), ContextValue::ARGUMENTS(expr.arguments.clone()));
                                            found_hook = true;
                                            let init_result = init_eval[0].symbol.get_symbol_as_weak(session, context, &mut diagnostics, Some(parent.borrow().get_file().unwrap().upgrade().unwrap().clone()));
                                            context.as_mut().unwrap().remove(&S!("parameters"));
                                            context.as_mut().unwrap().remove(&S!("constructing_class"));
                                            evals.push(Evaluation{
                                                symbol: EvaluationSymbol {
                                                    sym: EvaluationSymbolPtr::WEAK(init_result),
                                                    get_symbol_hook: None,
                                                },
                                                value: None,
                                                range: Some(expr.range)
                                            });
                                        }
                                        //It allows us to check parameters validity too if we are in validation step
                                        /*let parent_file_or_func = parent.borrow().parent_file_or_function().as_ref().unwrap().upgrade().unwrap();
                                        if is_in_validation {
                                            let from_module = parent.borrow().find_module();
                                            diagnostics.extend(Evaluation::validate_call_arguments(session,
                                                &init.borrow().as_func(),
                                                expr,
                                                context.as_ref().unwrap().get_key_value(&S!("parent")).unwrap_or((&S!(""), &ContextValue::SYMBOL(Weak::new()))).1.as_symbol(),
                                                from_module,
                                                false));
                                        }*/
                                    }
                                }
                                if !found_hook {
                                    evals.push(Evaluation{
                                        symbol: EvaluationSymbol {
                                            sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                                                weak: base_sym_weak_eval.weak.clone(),
                                                context: HashMap::new(),
                                                instance: Some(true),
                                                is_super: false,
                                            }),
                                            get_symbol_hook: None,
                                        },
                                        value: None,
                                        range: Some(expr.range)
                                    });
                                }
                            }
                        }
                    } else if base_sym.borrow().typ() == SymType::FUNCTION {
                        let base_sym_file = base_sym.borrow().get_file().as_ref().unwrap().upgrade().unwrap().clone();
                        SyncOdoo::build_now(session, &base_sym_file, BuildSteps::ARCH_EVAL);
                        let in_class = base_sym.borrow().get_in_parents(&vec![SymType::CLASS], true).is_some();
                        if required_dependencies.len() >= 2 {
                            if !in_class {
                                required_dependencies[1].push(base_sym_file.clone());
                            }
                        }
                        //function return evaluation can come from:
                        //  - type annotation parsing (ARCH_EVAL step)
                        //  - documentation parsing (Arch_eval and VALIDATION step)
                        //  - function body inference (VALIDATION step)
                        // Therefore, the actual version of the algorithm will trigger build from the different steps if this one has already been reached.
                        // We don't want to launch validation step while Arch evaluating the code.
                        if base_sym.borrow().evaluations().is_some()
                        && base_sym.borrow().evaluations().unwrap().len() == 0
                        && !base_sym_file.borrow().is_external()
                        && base_sym_file.borrow().build_status(BuildSteps::ARCH_EVAL) == BuildStatus::DONE
                        && base_sym.borrow().build_status(BuildSteps::ARCH) != BuildStatus::IN_PROGRESS
                        && base_sym.borrow().build_status(BuildSteps::ARCH_EVAL) != BuildStatus::IN_PROGRESS
                        && base_sym.borrow().build_status(BuildSteps::VALIDATION) == BuildStatus::PENDING {
                            let mut v = PythonValidator::new(base_sym.borrow().get_entry().unwrap(), base_sym.clone());
                            v.validate(session);
                        }
                        if required_dependencies.len() >= 3 {
                            if in_class {
                                required_dependencies[2].push(base_sym_file.clone());
                            }
                        }
                        if base_sym.borrow().evaluations().is_some() {
                            let call_parent = match base_sym_weak_eval.context.get(&S!("base_attr")){
                                Some(ContextValue::SYMBOL(s)) => s.clone(),
                                _ => Weak::new()
                            };
                            if is_in_validation {
                                let on_instance = base_sym_weak_eval.context.get(&S!("is_attr_of_instance")).map(|v| v.as_bool());
                                call_argument_diagnostics.last_mut().unwrap().extend(Evaluation::validate_call_arguments(session,
                                    &base_sym.borrow().as_func(),
                                    expr,
                                    call_parent.clone(),
                                    module.clone(),
                                    on_instance,
                                    ));
                            }
                            context.as_mut().unwrap().insert(S!("base_call"), ContextValue::SYMBOL(call_parent));
                            context.as_mut().unwrap().insert(S!("parameters"), ContextValue::ARGUMENTS(expr.arguments.clone()));
                            context.as_mut().unwrap().insert(S!("is_in_validation"), ContextValue::BOOLEAN(is_in_validation));
                            for eval in base_sym.borrow().evaluations().unwrap().iter() {
                                let eval_ptr = eval.symbol.get_symbol_weak_transformed(session, context, &mut diagnostics, Some(parent.borrow().get_file().unwrap().upgrade().unwrap().clone()));
                                evals.push(Evaluation{
                                    symbol: EvaluationSymbol {
                                        sym: eval_ptr,
                                        get_symbol_hook: None,
                                    },
                                    value: None,
                                    range: Some(expr.range)
                                });
                            }
                            context.as_mut().unwrap().remove(&S!("base_call"));
                            context.as_mut().unwrap().remove(&S!("parameters"));
                            context.as_mut().unwrap().remove(&S!("is_in_validation"));
                        }
                    }
                }
                diagnostics.extend(Evaluation::process_argument_diagnostics(&session, expr, call_argument_diagnostics, base_eval_ptrs.len()));
            },
            ExprOrIdent::Expr(Expr::Attribute(expr)) => {
                let (base_evals, diags) = Evaluation::eval_from_ast(session, &expr.value, parent.clone(), max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                if base_evals.is_empty() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                for base_eval in base_evals.iter() {
                    let base_ref = base_eval.symbol.get_symbol(session, context, &mut diagnostics, Some(parent.clone()));
                    if base_ref.is_expired_if_weak() {
                        return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                    }
                    let bases = Symbol::follow_ref(&base_ref, session, context, false, false, None, None);
                    for ibase in bases.iter() {
                        let base_loc = ibase.upgrade_weak();
                        if let Some(base_loc) = base_loc {
                            let file = base_loc.borrow().get_file().clone();
                            if let Some(base_loc_file) = file {
                                let base_loc_file = base_loc_file.upgrade().unwrap();
                                SyncOdoo::build_now(session, &base_loc_file, BuildSteps::ARCH_EVAL);
                                if base_loc_file.borrow().in_workspace() {
                                    if required_dependencies.len() == 2 {
                                        required_dependencies[1].push(base_loc_file.clone());
                                    } else if required_dependencies.len() == 3 {
                                        required_dependencies[2].push(base_loc_file.clone());
                                    }
                                }
                            }
                            let is_super = ibase.is_weak() && ibase.as_weak().is_super;
                            let (attributes, mut attributes_diagnostics) = base_loc.borrow().get_member_symbol(session, &expr.attr.to_string(), module.clone(), false, false, false, true, is_super);
                            for diagnostic in attributes_diagnostics.iter_mut(){
                                diagnostic.range = FileMgr::textRange_to_temporary_Range(&expr.range())
                            }
                            diagnostics.extend(attributes_diagnostics);
                            if !attributes.is_empty() {
                                let is_instance = ibase.as_weak().instance.unwrap_or(false);
                                attributes.iter().for_each(|attribute|{
                                    let instance = match attribute.borrow().typ() {
                                        SymType::CLASS => match for_annotation{
                                            true => Some(true),
                                            false => Some(false)
                                        },
                                        SymType::VARIABLE => match for_annotation {
                                            // this is a variable, but a follow_ref would probably lead to a class,
                                            // and here, because of annotation, we know we want an instance
                                            true => Some(true),
                                            false => None
                                        }
                                        _ => None
                                    };
                                    let mut eval = Evaluation::eval_from_symbol(&Rc::downgrade(attribute), instance);
                                    match eval.symbol.sym {
                                        EvaluationSymbolPtr::WEAK(ref mut weak) => {
                                            weak.context.insert(S!("base_attr"), ContextValue::SYMBOL(Rc::downgrade(&base_loc)));
                                            weak.context.insert(S!("is_attr_of_instance"), ContextValue::BOOLEAN(is_instance));
                                        },
                                        _ => {}
                                    }
                                    evals.push(eval);
                                });
                            }
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::Name(_)) | ExprOrIdent::Expr(Expr::Named(_)) | ExprOrIdent::Ident(_) | ExprOrIdent::Parameter(_) => {
                let (inferred_syms, name) = match ast {
                    ExprOrIdent::Expr(Expr::Name(expr))  =>  {
                        let name = expr.id.to_string();
                        (Symbol::infer_name(odoo, & parent, &name, Some( max_infer.to_u32())), name)
                    },
                    ExprOrIdent::Expr(Expr::Named(expr))  => {
                        match *expr.target {
                            Expr::Name(ref expr) => {
                                let name = expr.id.to_string();
                                (Symbol::infer_name(odoo, &parent, &name, Some(expr.range.end().to_u32())), name)
                            },
                            _ => return AnalyzeAstResult::from_only_diagnostics(diagnostics)
                        }
                    },
                    ExprOrIdent::Ident(expr) => {
                        let name = expr.id.to_string();
                        (Symbol::infer_name(odoo, & parent, &name, Some( max_infer.to_u32())), name)
                    },
                    ExprOrIdent::Parameter(expr) => {
                        let name = expr.name.id.to_string();
                        (Symbol::infer_name(odoo, & parent, &name, Some( max_infer.to_u32())), name)
                    }
                    _ => {
                        unreachable!();
                    }
                };
                match ast {
                    ExprOrIdent::Expr(Expr::Named(expr))  => {
                        let (_, diags) = Evaluation::eval_from_ast(session, &expr.value, parent.clone(), max_infer, false, required_dependencies);
                        diagnostics.extend(diags.clone());
                    }
                    _ => {}
                }

                if inferred_syms.symbols.is_empty() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                for inferred_sym in inferred_syms.symbols.iter() {
                    let instance = match inferred_sym.borrow().typ() {
                        SymType::CLASS => match for_annotation{
                            true => Some(true),
                            false => Some(false)
                        },
                        SymType::VARIABLE => match for_annotation {
                            // this is a variable, but a follow_ref would probably lead to a class,
                            // and here, because of annotation, we know we want an instance
                            true => Some(true),
                            false => None
                        }
                        _ => None
                    };
                    evals.push(Evaluation::eval_from_symbol(&Rc::downgrade(inferred_sym), instance));
                }
                if !inferred_syms.always_defined{
                    evals.push(Evaluation::new_unbound(name));
                }
            },
            ExprOrIdent::Expr(Expr::Subscript(sub)) => {
                let (eval_left, diags) = Evaluation::eval_from_ast(session, &sub.value, parent.clone(), max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                // TODO handle multiple eval_left
                if eval_left.is_empty() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base = &eval_left[0].symbol.get_symbol(session, context, &mut diagnostics, Some(parent.clone()));
                if base.is_expired_if_weak() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let bases = Symbol::follow_ref(&base, session, &mut None, false, false, None, None);
                let value = Evaluation::expr_to_str(session, &sub.slice, parent.clone(), max_infer, false, &mut diagnostics);
                diagnostics.extend(value.1);
                 for base in bases.iter() {
                    match base {
                        EvaluationSymbolPtr::WEAK(base_sym_weak_eval) if base_sym_weak_eval.instance == Some(false) => {
                            if let Some(SymType::CLASS) = base.upgrade_weak().map(|s| s.borrow().typ()) {
                                // This is a Generic type (Field[int], or List[int]), for now we just return the main type/Class (Field/List)
                                // TODO: handle generic types
                                let mut new_base = base.clone();
                                if for_annotation {
                                    new_base.as_mut_weak().instance = Some(true);
                                }
                                evals.push(Evaluation {
                                    symbol: EvaluationSymbol {
                                        sym: new_base,
                                        get_symbol_hook: None,
                                    },
                                    value: None,
                                    range: Some(sub.range())
                                });
                                continue;
                            }
                        }
                        _ => {}
                    }
                    if base.is_weak() && let Some(value) = &value.0 {
                        let base = base.upgrade_weak().unwrap();
                        let get_item = base.borrow().get_content_symbol("__getitem__", u32::MAX).symbols;
                        if get_item.len() == 1 {
                            let get_item = &get_item[0];
                            let get_item = get_item.borrow();
                            if get_item.evaluations().is_some() && get_item.evaluations().unwrap().len() == 1 {
                                let get_item_eval = &get_item.evaluations().unwrap()[0];
                                if let Some(hook) = get_item_eval.symbol.get_symbol_hook.as_ref() {
                                    let parent_file_or_func = parent.clone().borrow().parent_file_or_function().as_ref().unwrap().upgrade().unwrap();
                                    let is_in_validation = match parent_file_or_func.borrow().typ().clone() {
                                        SymType::FILE | SymType::PACKAGE(_) | SymType::FUNCTION => {
                                            parent_file_or_func.borrow().build_status(BuildSteps::VALIDATION) == BuildStatus::IN_PROGRESS
                                        },
                                        _ => {false}
                                    };
                                    context.as_mut().unwrap().insert(S!("args"), ContextValue::STRING(value.clone()));
                                    let old_range = context.as_mut().unwrap().remove(&S!("range"));
                                    context.as_mut().unwrap().insert(S!("range"), ContextValue::RANGE(sub.slice.range()));
                                    context.as_mut().unwrap().insert(S!("is_in_validation"), ContextValue::BOOLEAN(is_in_validation));
                                    let hook_result = (hook.callable)(session, &get_item_eval.symbol, context, &mut diagnostics, Some(parent.clone()));
                                    if let Some(hook_result) = hook_result {
                                        match hook_result {
                                            EvaluationSymbolPtr::WEAK(ref weak) => {
                                                if !weak.weak.is_expired() {
                                                    evals.push(Evaluation::eval_from_ptr(&hook_result));
                                                }
                                            },
                                            _ => {
                                                evals.push(Evaluation::eval_from_ptr(&hook_result));
                                            }
                                        }
                                    }
                                    context.as_mut().unwrap().remove(&S!("args"));
                                    context.as_mut().unwrap().remove(&S!("is_in_validation"));
                                    context.as_mut().unwrap().insert(S!("range"), old_range.unwrap());
                                }
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
            },
            ExprOrIdent::Expr(Expr::If(if_expr)) => {
                let (_, diags) = Evaluation::eval_from_ast(session, &if_expr.test, parent.clone(), max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                let (body_evals, diags) = Evaluation::eval_from_ast(session, &if_expr.body, parent.clone(), max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                let (orelse_evals, diags) = Evaluation::eval_from_ast(session, &if_expr.orelse, parent.clone(), max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                evals.extend(body_evals.into_iter().chain(orelse_evals.into_iter()));
            },
            ExprOrIdent::Expr(Expr::UnaryOp(unary_operator)) => 'u_op_block: {
                let method = match unary_operator.op {
                    UnaryOp::USub =>  "__neg__",
                    UnaryOp::UAdd =>  "__pos__",
                    UnaryOp::Invert =>  "__invert__",
                    UnaryOp::Not => {
                        // `Not` just uses internal __bool__ or __len__ and always returns bool
                        evals.push(Evaluation {
                            symbol: EvaluationSymbol {
                                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                                    weak: Rc::downgrade(&odoo.get_symbol("", &(vec![Sy!("builtins")], vec![Sy!("bool")]), u32::MAX).last().expect("builtins class not found")),
                                    context: HashMap::new(),
                                    instance: Some(true),
                                    is_super: false,
                                }),
                                get_symbol_hook: None
                            },
                            value: None,
                            range: Some(unary_operator.range()),
                        });
                        break 'u_op_block
                    },
                };
                let (bases, diags) = Evaluation::eval_from_ast(session, &unary_operator.operand, parent.clone(), max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                for base in bases.into_iter(){
                    let base_sym_weak_eval= base.symbol.get_symbol_weak_transformed(session, context, &mut diagnostics, None);
                    let base_eval_ptrs = Symbol::follow_ref(&base_sym_weak_eval, session, context, true, false, None, None);
                    for base_eval_ptr in base_eval_ptrs.iter() {
                        let EvaluationSymbolPtr::WEAK(base_sym_weak_eval) = base_eval_ptr else {continue};
                        let Some(base_sym) = base_sym_weak_eval.weak.upgrade() else {continue};
                        let (operator_functions, diags) = base_sym.borrow().get_member_symbol(session, &S!(method), module.clone(), true, false, true, false, false);
                        diagnostics.extend(diags);
                        for operator_function in operator_functions.into_iter(){
                            for eval in operator_function.borrow().evaluations().unwrap_or(&vec![]).iter() {
                                let eval_ptr = eval.symbol.get_symbol_weak_transformed(session, context, &mut diagnostics, Some(parent.borrow().get_file().unwrap().upgrade().unwrap().clone()));
                                evals.push(Evaluation{
                                    symbol: EvaluationSymbol {
                                        sym: eval_ptr,
                                        get_symbol_hook: None,
                                    },
                                    value: None,
                                    range: Some(unary_operator.range())
                                });
                            }
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::FString(_f_string_expr)) => {
                // TODO: Validate expression maybe?
                evals.push(
                    Evaluation {
                        symbol: EvaluationSymbol {
                            sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                                weak: Rc::downgrade(&odoo.get_symbol("", &(vec![Sy!("builtins")], vec![Sy!("str")]), u32::MAX).last().expect("builtins class not found")),
                                context: HashMap::new(),
                                instance: Some(true),
                                is_super: false,
                            }),
                            get_symbol_hook: None
                        },
                        value: None,
                        range: None,
                    }
                );
            }
            _ => {}
        }
        AnalyzeAstResult { evaluations: evals, diagnostics }
    }

    /**
     * parameters:
     * object_instance: None if called on nothing, true on an instance, false on a class
     */
    fn validate_call_arguments(session: &mut SessionInfo, function: &FunctionSymbol, expr_call: &ExprCall, on_object: Weak<RefCell<Symbol>>, from_module: Option<Rc<RefCell<Symbol>>>, object_instance: Option<bool>) -> Vec<Diagnostic> {
        if function.is_overloaded() || function.is_property {
            return vec![];
        }
        let mut diagnostics = vec![];
        //validate pos args first
        let mut arg_index = 0;
        let mut number_pos_arg = 0;
        let mut kword_only_args = Vec::new();
        let mut vararg_index = i32::MAX;
        let mut kwarg_index = i32::MAX;
        for (index, arg) in function.args.iter().enumerate() {
            match arg.arg_type {
                ArgumentType::POS_ONLY | ArgumentType::ARG => {
                    if arg.default_value.is_none() {
                        number_pos_arg += 1;
                    }
                }
                ArgumentType::VARARG => {
                    vararg_index = index as i32;
                },
                ArgumentType::KWORD_ONLY => {
                    kword_only_args.push(arg);
                },
                ArgumentType::KWARG => {
                    kwarg_index = index as i32;
                }
            }
        }
        if !function.is_static {
            if object_instance.is_some_and(|x| x) || //on instance
             object_instance.is_some_and(|x| !x) && function.is_class_method { //on classmethod
                //check that there is at least one positional argument
                let mut pos_arg = false;
                for arg in function.args.iter() {
                    match arg.arg_type {
                        ArgumentType::ARG | ArgumentType::VARARG | ArgumentType::POS_ONLY => {
                            pos_arg = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if !pos_arg {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01007, &[&function.name, &0.to_string(), &1.to_string()]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                    return diagnostics;
                }
                arg_index += 1;
            }
        }
        for arg in expr_call.arguments.args.iter() {
            if arg.is_starred_expr() {
                //TODO try to unpack the starred
                return diagnostics;
            }
            //match arg with argument from function
            let function_arg = function.args.get(min(arg_index, vararg_index) as usize);
            if function_arg.is_none() || function_arg.unwrap().arg_type == ArgumentType::KWORD_ONLY || function_arg.unwrap().arg_type == ArgumentType::KWARG {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01007, &[&function.name, &number_pos_arg.to_string(), &(arg_index + 1).to_string()]) {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                        ..diagnostic
                    });
                }
                return diagnostics;
            }
            if function_arg.unwrap().arg_type != ArgumentType::VARARG {
                //positional or arg
                diagnostics.extend(Evaluation::validate_func_arg(session, function_arg.unwrap(), arg, on_object.clone(), from_module.clone()));
            }
            arg_index += 1;
        }
        let min_arg_for_kword = arg_index;
        let mut found_pos_arg_with_kw = arg_index;
        let to_skip = min(min_arg_for_kword, vararg_index);
        for arg in expr_call.arguments.keywords.iter() {
            if let Some(arg_identifier) = &arg.arg { //if None, arg is a dictionary of keywords, like in self.func(a, b, **any_kwargs)
                let mut found_one = false;
                for func_arg in function.args.iter().skip(to_skip as usize) {
                    if func_arg.symbol.upgrade().unwrap().borrow().name().to_string() == arg_identifier.id {
                        diagnostics.extend(Evaluation::validate_func_arg(session, func_arg, &arg.value, on_object.clone(), from_module.clone()));
                        if func_arg.arg_type == ArgumentType::ARG {
                            found_pos_arg_with_kw += 1;
                        } else if func_arg.arg_type == ArgumentType::KWORD_ONLY {
                            kword_only_args.retain(|x| !Weak::ptr_eq(&x.symbol, &func_arg.symbol));
                        }
                        found_one = true;
                        break;
                    }
                }
                if !found_one && kwarg_index == i32::MAX {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01008, &[&function.name, &arg_identifier.id]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                }
            } else {
                // if arg is None, it means that it is a **arg
                found_pos_arg_with_kw = number_pos_arg;
            }
        }
        if found_pos_arg_with_kw < number_pos_arg {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01007, &[&function.name, &number_pos_arg.to_string(), &arg_index.to_string()]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                    ..diagnostic
                });
            }
            return diagnostics;
        }
        let mut kword_only_arg_missing = vec![]; // missing kword_only args without default value
        for kword_only_arg in kword_only_args.iter() {
            if kword_only_arg.default_value.is_none() {
                kword_only_arg_missing.push(kword_only_arg.symbol.upgrade().unwrap().borrow().name().clone());
            }
        }
        if !kword_only_arg_missing.is_empty() {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01010, &[&kword_only_arg_missing.join(", ")]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        }
        diagnostics
    }

    fn process_argument_diagnostics(session: &SessionInfo, expr_call: &ExprCall, diagnostics: Vec<Vec<Diagnostic>>, _eval_count: usize) -> Vec<Diagnostic> {
        let mut filtered_diagnostics = vec![];
        //iter through diagnostics and check that each evaluation has the same amount of diagnostics with code OLS01007 or OLS01008 or OLS01010
        let all_same_issues = diagnostics.iter().fold_while(None, |acc, diags| {
            let new_count = diags.iter().filter(|d|
                d.code == Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01007.to_string())) ||
                d.code == Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01008.to_string())) ||
                d.code == Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01010.to_string()))
            ).count() as i32;
            match acc {
                None => Continue(Some(new_count)),
                Some(count) => {
                    if count == new_count {
                        Continue(Some(count))
                    } else {
                        Done(Some(-1))
                    }
                }
            }
        }).into_inner();
        match all_same_issues {
            Some(-1) => {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01009, &[]) {
                    filtered_diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                        ..diagnostic
                    });
                }
            },
            Some(_count) => {
                filtered_diagnostics.extend(diagnostics[0].iter().filter(|d|
                    d.code == Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01007.to_string())) ||
                    d.code == Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01008.to_string())) ||
                    d.code == Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01010.to_string()))
                ).cloned().collect::<Vec<Diagnostic>>());
            },
            None => {}
        }
        // // we add the rest of the diagnostics as is
        for eval_diag in diagnostics {
            filtered_diagnostics.extend(eval_diag.into_iter().filter(|d| {
                d.code != Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01007.to_string())) &&
                d.code != Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01008.to_string())) &&
                d.code != Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01010.to_string()))
            }));
        }
        filtered_diagnostics
    }

    fn validate_domain(session: &mut SessionInfo, on_object: Weak<RefCell<Symbol>>, from_module: Option<Rc<RefCell<Symbol>>>, value: &Expr) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        if value.is_literal_expr() || matches!(value, Expr::Tuple(_)) {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03006, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(value.range().start().to_u32(), 0), Position::new(value.range().end().to_u32(), 0)),
                    ..diagnostic
                });
            }
            return diagnostics;
        }
        if !matches!(value, Expr::List(_)) {
            return diagnostics;
        }
        /*let from_module = None;
        let model = None;
        let domain = None;*/
        let mut need_tuple = 0;
        for item in value.as_list_expr().unwrap().elts.iter() {
            match item {
                Expr::Tuple(t) => {
                    need_tuple = max(need_tuple - 1, 0);
                    if t.elts.len() != 3 {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03007, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range::new(Position::new(t.range().start().to_u32(), 0), Position::new(t.range().end().to_u32(), 0)),
                                ..diagnostic
                            });
                        }
                    } else {
                        Evaluation::validate_tuple_search_domain(session, on_object.clone(), from_module.clone(), &t.elts[0], &t.elts[1], &t.elts[2], &mut diagnostics);
                    }
                },
                Expr::List(l) => {
                    need_tuple = max(need_tuple - 1, 0);
                    if l.elts.len() != 3 {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03007, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range::new(Position::new(l.range().start().to_u32(), 0), Position::new(l.range().end().to_u32(), 0)),
                                ..diagnostic
                            });
                        }
                    } else {
                        Evaluation::validate_tuple_search_domain(session, on_object.clone(), from_module.clone(), &l.elts[0], &l.elts[1], &l.elts[2], &mut diagnostics);
                    }
                },
                Expr::StringLiteral(s) => {
                    let value = s.value.to_string();
                    match value.as_str() {
                        "&" | "|" => {
                            if need_tuple == 0 {
                                need_tuple = 1;
                            }
                            need_tuple += 1;
                        },
                        "!"  => {
                            if need_tuple == 0 {
                                need_tuple = 1;
                            }
                        }
                        _ => {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03008, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(s.range().start().to_u32(), 0), Position::new(s.range().end().to_u32(), 0)),
                                    ..diagnostic
                                });
                            }
                        }
                    }
                },
                _ => {//do not handle for now
                }
            }
        }
        if need_tuple > 0 {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03010, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(value.range().start().to_u32(), 0), Position::new(value.range().end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        }
        diagnostics
    }

    fn validate_tuple_search_domain(session: &mut SessionInfo, on_object: Weak<RefCell<Symbol>>, from_module: Option<Rc<RefCell<Symbol>>>, elt1: &Expr, elt2: &Expr, _elt3: &Expr, diagnostics: &mut Vec<Diagnostic>) {
        //parameter 1
        if let Some(on_object) = on_object.upgrade() { //if weak is not set, we didn't manage to evalue base object. Do not validate in this case
            match elt1 {
                Expr::StringLiteral(s) => {
                    let value = s.value.to_string();
                    let split_expr = value.split(".");
                    let mut obj = Some(on_object);
                    let mut date_mode = false;
                    'split_name: for name in split_expr {
                        if date_mode {
                            if !["year_number", "quarter_number", "month_number", "iso_week_number", "day_of_week", "day_of_month", "day_of_year", "hour_number", "minute_number", "second_number"].contains(&name) {
                                if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03012, &[]) {
                                    diagnostics.push(Diagnostic {
                                        range: Range::new(Position::new(s.range().start().to_u32(), 0), Position::new(s.range().end().to_u32(), 0)),
                                        ..diagnostic_base
                                    });
                                }
                            }
                            date_mode = false;
                            continue;
                        }
                        if obj.is_none() {
                            if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03013, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(s.range().start().to_u32(), 0), Position::new(s.range().end().to_u32(), 0)),
                                    ..diagnostic_base
                                });
                            }
                            break;
                        }
                        if let Some(object) = &obj {
                            let (symbols, _diagnostics) = object.borrow().get_member_symbol(session,
                                &name.to_string(),
                                from_module.clone(),
                                false,
                                true,
                                false,
                                true,
                                false);
                            if symbols.is_empty() {
                                if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03011, &[&name, &object.borrow().name()]) {
                                    diagnostics.push(Diagnostic {
                                        range: Range::new(Position::new(s.range().start().to_u32(), 0), Position::new(s.range().end().to_u32(), 0)),
                                        ..diagnostic_base
                                    });
                                }
                                break;
                            }
                            obj = None;
                            for s in symbols.iter() {
                                if s.borrow().is_specific_field(session, &["Many2one", "One2many", "Many2many"]) {
                                    if s.borrow().typ() == SymType::VARIABLE {
                                        let models = s.borrow().as_variable().get_relational_model(session, from_module.clone());
                                        //only handle it if there is only one main symbol for this model
                                        if models.len() == 1 {
                                            obj = Some(models[0].clone());
                                        }
                                    }
                                }
                                if s.borrow().is_specific_field(session, &["Properties"]) {
                                    //TODO handle properties field
                                    //property field, not handled for now. Skip the parsing to not generate diagnostics
                                    break 'split_name
                                }
                                if s.borrow().is_specific_field(session, &["Date"]) {
                                    date_mode = true;
                                }
                            }
                        }
                    }
                },
                _ => {}
            }
            //parameter 2
            match elt2 {
                Expr::StringLiteral(s) => {
                    match s.value.to_str() {
                        "=" | "!=" | ">" | ">=" | "<" | "<=" | "=?" | "=like" | "like" | "not like" | "ilike" |
                        "not ilike" | "=ilike" | "in" | "not in" | "child_of" | "parent_of" | "any" | "not any" => {},
                        _ => {
                            if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03009, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(s.range().start().to_u32(), 0), Position::new(s.range().end().to_u32(), 0)),
                                    ..diagnostic_base.clone()
                                });
                            }
                        }
                    }
                },
                _ => {}
            }
        }
    }

    fn validate_func_arg(session: &mut SessionInfo<'_>, function_arg: &Argument, arg: &Expr, on_object: Weak<RefCell<Symbol>>, from_module: Option<Rc<RefCell<Symbol>>>) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        if let Some(symbol) = function_arg.symbol.upgrade() {
            if symbol.borrow().evaluations().unwrap_or(&vec![]).len() == 1 {
                match symbol.borrow().evaluations().unwrap()[0].symbol.sym.clone() {
                    EvaluationSymbolPtr::DOMAIN => {
                        diagnostics.extend(Evaluation::validate_domain(session, on_object, from_module, arg));
                    },
                    _ => {}
                }
            }
        }
        diagnostics
    }
}

impl EvaluationSymbol {

    pub fn new_with_symbol(symbol: Weak<RefCell<Symbol>>, instance: Option<bool>, context: Context, get_symbol_hook: Option<GetSymbolHook>) -> Self {
        Self { sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: symbol, context, instance: instance, is_super: false}), get_symbol_hook }
    }

    pub fn new_self(get_symbol_hook: Option<GetSymbolHook>) -> EvaluationSymbol {
        Self {
            sym: EvaluationSymbolPtr::SELF,
            get_symbol_hook,
        }
    }

    pub fn is_instance(&self) -> Option<bool> {
        match &self.sym {
            EvaluationSymbolPtr::ANY => None,
            EvaluationSymbolPtr::ARG(_) => None,
            EvaluationSymbolPtr::NONE => None,
            EvaluationSymbolPtr::UNBOUND(_) => None,
            EvaluationSymbolPtr::SELF => Some(true),
            EvaluationSymbolPtr::DOMAIN => Some(false), //domain is always used for types
            EvaluationSymbolPtr::WEAK(w) => w.instance
        }
    }

    pub fn get_weak(&self) -> &EvaluationSymbolWeak {
        match &self.sym {
            EvaluationSymbolPtr::WEAK(w) => w,
            _ => panic!("Not an EvaluationSymbolWeak")
        }
    }

    pub fn get_weak_mut(&mut self) -> &mut EvaluationSymbolWeak {
        match &mut self.sym {
            EvaluationSymbolPtr::WEAK(w) => w,
            _ => panic!("Not an EvaluationSymbolWeak")
        }
    }

    /* Execute the hook, then use context to return an EvaluationSymbolWeak if possible, else return an empty one */
    pub fn get_symbol_as_weak(&self, session: &mut SessionInfo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> EvaluationSymbolWeak {
        let eval = EvaluationSymbol::get_symbol(&self, session, context, diagnostics, scope);
        match eval {
            EvaluationSymbolPtr::WEAK(w) => {
                w
            },
            EvaluationSymbolPtr::ANY => EvaluationSymbolWeak{weak: Weak::new(), context: HashMap::new(), instance: Some(false), is_super: false},
            EvaluationSymbolPtr::ARG(_) => EvaluationSymbolWeak{weak: Weak::new(), context: HashMap::new(), instance: Some(false), is_super: false},
            EvaluationSymbolPtr::NONE => EvaluationSymbolWeak{weak: Weak::new(), context: HashMap::new(), instance: Some(false), is_super: false},
            EvaluationSymbolPtr::UNBOUND(_) => EvaluationSymbolWeak{weak: Weak::new(), context: HashMap::new(), instance: Some(false), is_super: false},
            EvaluationSymbolPtr::DOMAIN => EvaluationSymbolWeak{weak: Weak::new(), context: HashMap::new(), instance: Some(false), is_super: false},
            EvaluationSymbolPtr::SELF => {
                let class = context.as_ref().
                and_then(|context| context.get(&S!("parent_for")).or(context.get(&S!("base_attr"))))
                .unwrap_or(&ContextValue::BOOLEAN(false));
                match class {
                    ContextValue::SYMBOL(s) => EvaluationSymbolWeak{weak: s.clone(), context: HashMap::new(), instance: Some(true), is_super: false},
                    _ => EvaluationSymbolWeak{weak: Weak::new(), context: HashMap::new(), instance: Some(false), is_super: false}
                }
            }
        }
    }

    /* Execute Hook, then return the effective EvaluationSymbolPtr, but transformed as EvaluationSmbolWeak if possible */
    pub fn get_symbol_weak_transformed(&self, session: &mut SessionInfo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<Rc<RefCell<Symbol>>>) -> EvaluationSymbolPtr {
        let eval = EvaluationSymbol::get_symbol(&self, session, context, diagnostics, scope);
        match eval {
            EvaluationSymbolPtr::WEAK(_) => {
                eval
            },
            EvaluationSymbolPtr::ANY => eval,
            EvaluationSymbolPtr::ARG(_) => eval,
            EvaluationSymbolPtr::NONE => eval,
            EvaluationSymbolPtr::UNBOUND(_) => eval,
            EvaluationSymbolPtr::DOMAIN => eval,
            EvaluationSymbolPtr::SELF => {
                let class = context.as_ref().and_then(|context| context.get(&S!("base_call"))).unwrap_or(&ContextValue::BOOLEAN(false));
                match class {
                    ContextValue::SYMBOL(s) => EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: s.clone(), context: HashMap::new(), instance: Some(true), is_super: false}),
                    _ => EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: Weak::new(), context: HashMap::new(), instance: Some(false), is_super: false})
                }
            }
        }
    }

    /* Execute Hook, then return the effective EvaluationSymbolPtr */
    pub fn get_symbol(&self, session: &mut SessionInfo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<Rc<RefCell<Symbol>>>) -> EvaluationSymbolPtr {
        let mut custom_eval = None;
        if let Some(hook) = self.get_symbol_hook.as_ref() {
            custom_eval = (hook.callable)(session, self, context, diagnostics, file_symbol);
        }
        custom_eval.as_ref().unwrap_or(&self.sym).clone()
    }

    //Return the symbol ptr, if you need to know its type (domain, None, ...). If you need the symbol behind the pointer, use get_symbol however
    pub fn get_symbol_ptr(&self) -> &EvaluationSymbolPtr {
        &self.sym
    }
    //Return the symbol ptr, if you need to know its type (domain, None, ...). If you need the symbol behind the pointer, use get_symbol however
    pub fn get_mut_symbol_ptr(&mut self) -> &mut EvaluationSymbolPtr {
        &mut self.sym
    }
}

impl EvaluationSymbolPtr {

    pub fn is_expired_if_weak(&self) -> bool {
        match self {
            EvaluationSymbolPtr::WEAK(w) => w.weak.is_expired(),
            _ => false
        }
    }

    pub fn upgrade_weak(&self) -> Option<Rc<RefCell<Symbol>>> {
        match self {
            EvaluationSymbolPtr::WEAK(w) => w.weak.upgrade(),
            _ => None
        }
    }

    pub(crate) fn is_weak(&self) -> bool {
        match self {
            EvaluationSymbolPtr::WEAK(_) => true,
            _ => false
        }
    }

    pub(crate) fn as_weak(&self) -> &EvaluationSymbolWeak {
        match self {
            EvaluationSymbolPtr::WEAK(w) => &w,
            _ => panic!("Not an EvaluationSymbolWeak")
        }
    }

    pub(crate) fn as_mut_weak(&mut self) -> &mut EvaluationSymbolWeak {
        match self {
            EvaluationSymbolPtr::WEAK(w) => w,
            _ => panic!("Not an EvaluationSymbolWeak")
        }
    }
}
