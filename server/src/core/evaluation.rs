use ruff_python_ast::{Expr, ExprCall, Identifier, Operator, Parameter};
use ruff_text_size::{Ranged, TextRange, TextSize};
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
use weak_table::traits::WeakElement;
use std::cmp::min;
use std::collections::HashMap;
use std::i32;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::constants::*;
use crate::core::odoo::SyncOdoo;
use crate::threads::SessionInfo;
use crate::S;

use super::file_mgr::FileMgr;
use super::python_validator::PythonValidator;
use super::symbols::function_symbol::{Argument, ArgumentType, FunctionSymbol};
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
    pub symbol: EvaluationSymbol, // int
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
type GetSymbolHook = fn (session: &mut SessionInfo, eval: &EvaluationSymbol, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<Rc<RefCell<Symbol>>>) -> EvaluationSymbolWeak;


#[derive(Debug, Clone)]
pub struct EvaluationSymbolWeak {
    pub weak: Weak<RefCell<Symbol>>,
    pub instance: Option<bool>,
    pub is_super: bool,
}

impl EvaluationSymbolWeak {
    pub fn new(weak: Weak<RefCell<Symbol>>, instance: Option<bool>, is_super: bool) -> Self {
        EvaluationSymbolWeak {
            weak,
            instance,
            is_super
        }
    }

    pub fn is_instance(&self) -> Option<bool> {
        return self.instance;
    }
}

#[derive(Debug, Default, Clone)]
enum EvaluationSymbolPtr {
    WEAK(EvaluationSymbolWeak),
    SELF,
    ARG(u32),
    DOMAIN,
    NONE,
    #[default]
    ANY
}

#[derive(Debug, Default, Clone)]
pub struct EvaluationSymbol {
    sym: EvaluationSymbolPtr,
    pub context: Context,
    pub factory: Option<Weak<RefCell<Symbol>>>,
    pub get_symbol_hook: Option<GetSymbolHook>,
}

#[derive(Default)]
pub struct AnalyzeAstResult {
    pub evaluations: Vec<Evaluation>,
    pub effective_sym: Option<Weak<RefCell<Symbol>>>,
    pub factory: Option<Weak<RefCell<Symbol>>>,
    pub diagnostics: Vec<Diagnostic>
}

impl AnalyzeAstResult {
    pub fn from_only_diagnostics(diags: Vec<Diagnostic>) -> Self {
        AnalyzeAstResult { evaluations: vec![], effective_sym: None, factory: None, diagnostics: diags }
    }
}

impl Evaluation {

    pub fn new_list(odoo: &mut SyncOdoo, values: Vec<Expr>, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("list")]), u32::MAX).last().expect("builtins list not found")),
                    instance: Some(true),
                    is_super: false,
                }),
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
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("tuple")]), u32::MAX).last().expect("builtins list not found")),
                    instance: Some(true),
                    is_super: false,
                }),
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
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: Rc::downgrade(&odoo.get_symbol(&(vec![S!("builtins")], vec![S!("dict")]), u32::MAX).last().expect("builtins list not found")),
                    instance: Some(true),
                    is_super: false,
                }),
                context: HashMap::new(),
                factory: None,
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::DICT(values)),
            range: Some(range)
        }
    }

    pub fn new_domain(odoo: &mut SyncOdoo) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::DOMAIN,
                context: HashMap::new(),
                factory: None,
                get_symbol_hook: None
            },
            value: None,
            range: None
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
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: symbol,
                    instance: Some(true),
                    is_super: false,
                }),
                context: HashMap::new(),
                factory: None,
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
                context: HashMap::new(),
                factory: None,
                get_symbol_hook: None,
            },
            value: None,
            range: None
        }
    }

    //return the evaluation but valid outside of the given function scope
    pub fn get_eval_out_of_function_scope(&self, session: &mut SessionInfo, function: &Rc<RefCell<Symbol>>) -> Vec<Evaluation> {
        let mut res = vec![];
        match self.symbol.sym {
            EvaluationSymbolPtr::WEAK(_) => {
                //take the weak by get_symbol instead of the match
                let symbol_eval_weak = self.symbol.get_symbol(session, &mut None, &mut vec![], None);
                let out_of_scope = Symbol::follow_ref(&symbol_eval_weak, session, &mut None, true, false, Some(function.clone()), &mut vec![]);
                for weak_sym in out_of_scope {
                    if !weak_sym.weak.is_expired() {
                        res.push(Evaluation {
                            symbol: EvaluationSymbol {
                                sym: EvaluationSymbolPtr::WEAK(weak_sym),
                                context: HashMap::new(),
                                factory: None,
                                get_symbol_hook: None,
                            },
                            value: None,
                            range: None
                        })
                    }
                }
            },
            EvaluationSymbolPtr::SELF | EvaluationSymbolPtr::ARG(_) | EvaluationSymbolPtr::NONE | EvaluationSymbolPtr::ANY |EvaluationSymbolPtr::DOMAIN => {
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
            if eval_symbol.weak.is_expired() {
                return None;
            }
            let evals = Symbol::follow_ref(&eval_symbol, session, context, false, true, None, diagnostics);
            if evals.len() == 1 {
                let eval = &evals[0];
                let eval_sym = eval.weak.upgrade();
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

    //return true if both evalution lead to the same final type
    pub fn eq_type(&self, other_eval: &Evaluation) -> bool {
        false //TODO
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

    //create an evaluation that is evaluating to the given symbol
    pub fn eval_from_symbol(symbol: &Weak<RefCell<Symbol>>, instance: Option<bool>) -> Evaluation{
        if symbol.is_expired() {
            return Evaluation::new_none();
        }
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: symbol.clone(),
                    instance: instance,
                    is_super: false,
                }),
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
        let analyze_result = Evaluation::analyze_ast(session, &ExprOrIdent::Expr(ast), parent, max_infer, &mut context);
        return (analyze_result.evaluations, analyze_result.diagnostics)
    }

    /* Given an Expr, try to return the represented String. None if it can't be achieved */
    fn expr_to_str(session: &mut SessionInfo, ast: &Expr, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize, diagnostics: &mut Vec<Diagnostic>) -> (Option<String>, Vec<Diagnostic>) {
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
        let value = Evaluation::analyze_ast(session, &ExprOrIdent::Expr(ast), parent, max_infer, &mut context);
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
    pub fn analyze_ast(session: &mut SessionInfo, ast: &ExprOrIdent, parent: Rc<RefCell<Symbol>>, max_infer: &TextSize, context: &mut Option<Context>) -> AnalyzeAstResult {
        let odoo = &mut session.sync_odoo;
        let mut evals = vec![];
        let effective_sym = None;
        let factory = None;
        let mut diagnostics = vec![];
        let module: Option<Rc<RefCell<Symbol>>> = parent.borrow().find_module();

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
                let (base_eval, diags) = Evaluation::eval_from_ast(session, &expr.func, parent.clone(), max_infer);
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
                    /*TODO if multiple evals are found, we could maybe try to validate that they all have the same signature in case of diamond inheritance?
                    However, other cases should be handled by arch step or syntax? */
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let mut context = Some(base_eval[0].symbol.context.clone());
                //TODO context should give params
                let base_sym_weak_eval= base_eval[0].symbol.get_symbol(session, &mut context, &mut diagnostics, None);
                let base_sym = base_sym_weak_eval.weak.upgrade();
                if let Some(base_sym) = base_sym {
                    if base_sym.borrow().typ() == SymType::CLASS {
                        if base_sym_weak_eval.instance.unwrap_or(false) {
                            //TODO handle call on class instance
                        } else {
                            if base_sym.borrow().get_tree() == (vec![S!("builtins")], vec![S!("super")]){
                                //  - If 1st argument exists, we add that class with symbol_type Super
                                let super_class = if !expr.arguments.is_empty(){
                                    let (class_eval, diags) = Evaluation::eval_from_ast(session, &expr.arguments.args[0], parent.clone(), max_infer);
                                    diagnostics.extend(diags);
                                    if class_eval.len() != 1 {
                                        return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                                    }
                                    let class_sym_weak_eval= class_eval[0].symbol.get_symbol(session, &mut context, &mut diagnostics, None);
                                    class_sym_weak_eval.weak.upgrade().and_then(|class_sym|{
                                        let class_sym_weak_eval = &Symbol::follow_ref(&EvaluationSymbolWeak::new(
                                            Rc::downgrade(&class_sym), None, false
                                        ), session, &mut None, false, false, None, &mut diagnostics)[0];
                                        if class_sym_weak_eval.weak.upgrade().unwrap().borrow().typ() != SymType::CLASS{
                                            return None;
                                        }
                                        if class_sym_weak_eval.instance.unwrap_or(false) {
                                            diagnostics.push(Diagnostic::new(
                                                Range::new(Position::new(expr.arguments.args[0].range().start().to_u32(), 0),
                                                Position::new(expr.arguments.args[0].range().end().to_u32(), 0)),
                                                Some(DiagnosticSeverity::ERROR),
                                                Some(NumberOrString::String(S!("OLS30311"))),
                                                Some(EXTENSION_NAME.to_string()),
                                                S!("First Argument to super must be a class"),
                                                None,
                                                None
                                                )
                                            );
                                            None
                                        } else {
                                            let mut is_instance = None;
                                            if expr.arguments.args.len() >= 2 {
                                                let (object_or_type_eval, diags) = Evaluation::eval_from_ast(session, &expr.arguments.args[1], parent.clone(), max_infer);
                                                diagnostics.extend(diags);
                                                if object_or_type_eval.len() != 1 {
                                                    return Some((class_sym_weak_eval.weak.clone(), is_instance))
                                                }
                                                let object_or_type_weak_eval = &Symbol::follow_ref(
                                                    &object_or_type_eval[0].symbol.get_symbol(
                                                        session, &mut context, &mut diagnostics, None),
                                                        session, &mut None, false, false, None, &mut diagnostics)[0];
                                                is_instance = object_or_type_weak_eval.instance;
                                            }
                                            Some((class_sym_weak_eval.weak.clone(), is_instance))
                                        }
                                    })
                                //  - Otherwise we get the encapsulating class
                                } else {
                                    match parent.borrow().get_in_parents(&vec![SymType::CLASS], true){
                                        None => {
                                            diagnostics.push(Diagnostic::new(
                                                Range::new(Position::new(expr.range().start().to_u32(), 0),
                                                Position::new(expr.range().end().to_u32(), 0)),
                                                Some(DiagnosticSeverity::ERROR),
                                                Some(NumberOrString::String(S!("OLS30312"))),
                                                Some(EXTENSION_NAME.to_string()),
                                                S!("Super calls outside a class scope must have at least one argument"),
                                                None,
                                                None
                                                )
                                            );
                                            None
                                        },
                                        Some(parent_class) => Some((parent_class.clone(), Some(true)))
                                    }
                                };
                                if let Some((super_class, instance)) = super_class{
                                    evals.push(Evaluation{
                                        symbol: EvaluationSymbol {
                                            sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                                                weak: super_class,
                                                instance,
                                                is_super: true,
                                            }),
                                            context: HashMap::new(),
                                            factory: None,
                                            get_symbol_hook: None,
                                        },
                                        value: None,
                                        range: Some(expr.range)
                                    });
                                }
                            } else {
                                //TODO diagnostic __new__ call parameters
                                evals.push(Evaluation{
                                    symbol: EvaluationSymbol {
                                        sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                                            weak: base_sym_weak_eval.weak.clone(),
                                            instance: Some(true),
                                            is_super: false,
                                        }),
                                        context: HashMap::new(),
                                        factory: None,
                                        get_symbol_hook: None,
                                    },
                                    value: None,
                                    range: Some(expr.range)
                                });
                            }
                        }
                    } else if base_sym.borrow().typ() == SymType::FUNCTION {
                        //function return evaluation can come from:
                        //  - type annotation parsing (ARCH_EVAL step)
                        //  - documentation parsing (Arch_eval and VALIDATION step)
                        //  - function body inference (VALIDATION step)
                        // Therefore, the actual version of the algorithm will trigger build from the different steps if this one has already been reached.
                        // We don't want to launch validation step while Arch evaluating the code.
                        if base_sym.borrow().evaluations().is_some() && base_sym.borrow().evaluations().unwrap().len() == 0 {
                            if base_sym.borrow().parent_file_or_function().as_ref().unwrap().upgrade().unwrap().borrow().build_status(BuildSteps::ODOO) == BuildStatus::DONE &&
                            base_sym.borrow().build_status(BuildSteps::VALIDATION) == BuildStatus::PENDING { //TODO update with new step validation to lower it to localized level
                                let mut v = PythonValidator::new(base_sym.clone());
                                v.validate(session);
                            }
                        }
                        if base_sym.borrow().evaluations().is_some() {
                            let parent_file_or_func = parent.borrow().parent_file_or_function().as_ref().unwrap().upgrade().unwrap();
                            let is_in_validation = match parent_file_or_func.borrow().typ().clone() {
                                SymType::FILE | SymType::PACKAGE(_) => {
                                    parent_file_or_func.borrow().build_status(BuildSteps::VALIDATION) == BuildStatus::IN_PROGRESS
                                },
                                SymType::FUNCTION => {
                                    true //functions are always evaluated at validation step
                                }
                                _ => {false}
                            };
                            if is_in_validation {
                                let mut on_instance = !base_sym.borrow().as_func().is_static;
                                if on_instance {
                                    //check that the call is indeed done on an instance
                                    on_instance = context.as_ref().unwrap().get_key_value(&S!("is_attr_of_instance"))
                                    .unwrap_or((&S!("is_attr"), &ContextValue::BOOLEAN(false))).1.as_bool();
                                }
                                diagnostics.extend(Evaluation::validate_call_arguments(session,
                                    &base_sym.borrow().as_func(),
                                    expr,
                                    on_instance));
                            }
                            for eval in base_sym.borrow().evaluations().unwrap().iter() {
                                let mut e = eval.clone();
                                e.symbol.context.extend(context.as_mut().unwrap().clone());
                                e.range = Some(expr.range.clone());
                                evals.push(e);
                            }
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::Attribute(expr)) => {
                let (base_evals, diags) = Evaluation::eval_from_ast(session, &expr.value, parent.clone(), max_infer);
                diagnostics.extend(diags);
                if base_evals.len() != 1 || base_evals[0].symbol.get_symbol(session, &mut None, &mut diagnostics, None).weak.is_expired() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base_ref = base_evals[0].symbol.get_symbol(session, &mut None, &mut diagnostics, Some(parent.borrow().get_file().unwrap().upgrade().unwrap().clone()));
                let bases = Symbol::follow_ref(&base_ref, session, &mut None, false, false, None, &mut diagnostics);
                for ibase in bases.iter() {
                    let base_loc = ibase.weak.upgrade();
                    if let Some(base_loc) = base_loc {
                        let (attributes, mut attributes_diagnostics) = base_loc.borrow().get_member_symbol(session, &expr.attr.to_string(), module.clone(), false, true, base_ref.is_super);
                        for diagnostic in attributes_diagnostics.iter_mut(){
                            diagnostic.range = FileMgr::textRange_to_temporary_Range(&expr.range())
                        }
                        diagnostics.extend(attributes_diagnostics);
                        if !attributes.is_empty() {
                            let mut eval = Evaluation::eval_from_symbol(&Rc::downgrade(attributes.first().unwrap()), None);
                            if ibase.instance.unwrap_or(false) {
                                context.as_mut().unwrap().insert(S!("is_attr_of_instance"), ContextValue::BOOLEAN(true));
                            }
                            eval.symbol.context = context.as_ref().unwrap().clone();
                            eval.symbol.context.insert(S!("parent"), ContextValue::SYMBOL(Rc::downgrade(&base_loc)));
                            evals.push(eval);
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::Name(_)) | ExprOrIdent::Ident(_) | ExprOrIdent::Parameter(_) => {
                let infered_syms = match ast {
                    ExprOrIdent::Expr(Expr::Name(expr))  =>  {
                        Symbol::infer_name(odoo, & parent, & expr.id.to_string(), Some( max_infer.to_u32()))
                    },
                    ExprOrIdent::Ident(expr) => {
                        Symbol::infer_name(odoo, & parent, & expr.id.to_string(), Some( max_infer.to_u32()))
                    },
                    ExprOrIdent::Parameter(expr) => {
                        Symbol::infer_name(odoo, & parent, & expr.name.id.to_string(), Some( max_infer.to_u32()))
                    }
                    _ => {
                        unreachable!();
                    }
                };

                if infered_syms.is_empty() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                for infered_sym in infered_syms.iter() {
                    evals.push(Evaluation::eval_from_symbol(&Rc::downgrade(infered_sym), None));
                }
            },
            ExprOrIdent::Expr(Expr::Subscript(sub)) => {
                let (eval_left, diags) = Evaluation::eval_from_ast(session, &sub.value, parent.clone(), max_infer);
                diagnostics.extend(diags);
                if eval_left.len() != 1 || eval_left[0].symbol.get_symbol(session, &mut None, &mut diagnostics, None).weak.is_expired() { //TODO set context?
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base = &eval_left[0].symbol.get_symbol(session, &mut None, &mut diagnostics, None); //TODO set context?
                let bases = Symbol::follow_ref(&base, session, &mut None, false, false, None, &mut diagnostics);
                if bases.len() != 1 {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base = &bases[0];
                let base = base.weak.upgrade().unwrap();
                let value = Evaluation::expr_to_str(session, &sub.slice, parent.clone(), max_infer, &mut diagnostics);
                diagnostics.extend(value.1);
                if let Some(value) = value.0 {
                    let get_item = base.borrow().get_content_symbol("__getitem__", u32::MAX);
                    if get_item.len() == 1 {
                        let get_item = &get_item[0];
                        let get_item = get_item.borrow();
                        if get_item.evaluations().is_some() && get_item.evaluations().unwrap().len() == 1 {
                            let get_item_eval = &get_item.evaluations().unwrap()[0];
                            if let Some(hook) = get_item_eval.symbol.get_symbol_hook {
                                context.as_mut().unwrap().insert(S!("args"), ContextValue::STRING(value));
                                let old_range = context.as_mut().unwrap().remove(&S!("range"));
                                context.as_mut().unwrap().insert(S!("range"), ContextValue::RANGE(sub.slice.range()));
                                let hook_result = hook(session, &get_item_eval.symbol, context, &mut diagnostics, Some(parent.borrow().get_file().unwrap().upgrade().unwrap().clone()));
                                if !hook_result.weak.is_expired() {
                                    evals.push(Evaluation::eval_from_symbol(&hook_result.weak, hook_result.instance));
                                }
                                context.as_mut().unwrap().remove(&S!("args"));
                                context.as_mut().unwrap().insert(S!("range"), old_range.unwrap());
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
        AnalyzeAstResult { evaluations: evals, effective_sym, factory, diagnostics }
    }

    fn validate_call_arguments(session: &mut SessionInfo, function: &FunctionSymbol, exprCall: &ExprCall, is_on_instance: bool) -> Vec<Diagnostic> {
        if function.is_overloaded() {
            return vec![];
        }
        let mut diagnostics = vec![];
        //validate pos args first
        let mut arg_index = 0;
        let mut number_pos_arg = 0;
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
                ArgumentType::KWARG => {
                    kwarg_index = index as i32;
                },
                _ => {}
            }
        }
        if is_on_instance {
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
                diagnostics.push(Diagnostic::new(
                    Range::new(Position::new(exprCall.range().start().to_u32(), 0), Position::new(exprCall.range().end().to_u32(), 0)),
                    Some(DiagnosticSeverity::ERROR),
                    Some(NumberOrString::String(S!("OLS30315"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("{} takes 0 positional arguments, but at least 1 is given", function.name),
                    None,
                    None,
                ));
                return diagnostics;
            }
            arg_index += 1;
        }
        for arg in exprCall.arguments.args.iter() {
            //match arg with argument from function
            let function_arg = function.args.get(min(arg_index, vararg_index) as usize);
            if function_arg.is_none() || function_arg.unwrap().arg_type == ArgumentType::KWORD_ONLY || function_arg.unwrap().arg_type == ArgumentType::KWARG {
                diagnostics.push(Diagnostic::new(
                    Range::new(Position::new(exprCall.range().start().to_u32(), 0), Position::new(exprCall.range().end().to_u32(), 0)),
                    Some(DiagnosticSeverity::ERROR),
                    Some(NumberOrString::String(S!("OLS30315"))),
                    Some(EXTENSION_NAME.to_string()),
                    format!("{} takes {} positional arguments, but at least {} is given", function.name, number_pos_arg, arg_index + 1),
                    None,
                    None,
                ));
                return diagnostics;
            }
            if function_arg.unwrap().arg_type != ArgumentType::VARARG {
                //positional or arg
                diagnostics.extend(Evaluation::validate_func_arg(session, function_arg.unwrap(), arg));
            }
            arg_index += 1;
        }
        let min_arg_for_kword = arg_index;
        let mut min_index_called_arg_with_kw = arg_index;
        let to_skip = min(min_arg_for_kword, vararg_index);
        for arg in exprCall.arguments.keywords.iter() {
            if let Some(arg_identifier) = &arg.arg { //if None, arg is a dictionnary of keywords, like in self.func(a, b, **any_kwargs)
                let mut found_one = false;
                for (arg_index, func_arg) in function.args.iter().skip(to_skip as usize).enumerate() {
                    if func_arg.symbol.upgrade().unwrap().borrow().name() == arg_identifier.id {
                        diagnostics.extend(Evaluation::validate_func_arg(session, func_arg, &arg.value));
                        min_index_called_arg_with_kw = arg_index as i32 + to_skip;
                        found_one = true;
                        break;
                    }
                }
                if !found_one && kwarg_index == i32::MAX {
                    diagnostics.push(Diagnostic::new(
                        Range::new(Position::new(exprCall.range().start().to_u32(), 0), Position::new(exprCall.range().end().to_u32(), 0)),
                        Some(DiagnosticSeverity::ERROR),
                        Some(NumberOrString::String(S!("OLS30316"))),
                        Some(EXTENSION_NAME.to_string()),
                        format!("{} got an unexpected keyword argument '{}'", function.name, arg_identifier.id),
                        None,
                        None,
                    ))
                }
            }
        }
        if min_index_called_arg_with_kw + 1 < number_pos_arg {
            diagnostics.push(Diagnostic::new(
                Range::new(Position::new(exprCall.range().start().to_u32(), 0), Position::new(exprCall.range().end().to_u32(), 0)),
                Some(DiagnosticSeverity::ERROR),
                Some(NumberOrString::String(S!("OLS30315"))),
                Some(EXTENSION_NAME.to_string()),
                format!("{} takes {} positional arguments, but only {} is given", function.name, number_pos_arg, arg_index),
                None,
                None,
            ));
            return diagnostics;
        }
        diagnostics
    }

    fn validate_domain(session: &mut SessionInfo, value: &Expr) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        if !matches!(value, Expr::List(_)) {
            return diagnostics;
        }
        /*let from_module = None;
        let model = None;
        let domain = None;*/
        let need_tuple = 0;
        for item in value.as_list_expr().unwrap().elts.iter() {
            match item {
                Expr::Tuple(t) => {
                    if t.elts.len() != 3 {
                        diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(t.range().start().to_u32(), 0), Position::new(t.range().end().to_u32(), 0)),
                            Some(DiagnosticSeverity::ERROR),
                            Some(NumberOrString::String(S!("OLS30314"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("Domain tuple should have 3 elements"),
                            None,
                            None,
                        ));
                    } else {
                        Evaluation::validate_tuple_search_domain(session, &t.elts[0], &t.elts[1], &t.elts[2], &mut diagnostics);
                    }
                },
                Expr::List(l) => {
                    if l.elts.len() != 3 {
                        diagnostics.push(Diagnostic::new(
                            Range::new(Position::new(l.range().start().to_u32(), 0), Position::new(l.range().end().to_u32(), 0)),
                            Some(DiagnosticSeverity::ERROR),
                            Some(NumberOrString::String(S!("OLS30314"))),
                            Some(EXTENSION_NAME.to_string()),
                            format!("Domain tuple should have 3 elements"),
                            None,
                            None,
                        ));
                    } else {
                        Evaluation::validate_tuple_search_domain(session, &l.elts[0], &l.elts[1], &l.elts[2], &mut diagnostics);
                    }
                },
                Expr::StringLiteral(s) => {

                },
                _ => {//do not handle for now
                }
            }
        }
        diagnostics
    }

    fn validate_tuple_search_domain(session: &mut SessionInfo, elt1: &Expr, elt2: &Expr, elt3: &Expr, diagnostics: &mut Vec<Diagnostic>) {
        
    }

    fn validate_func_arg(session: &mut SessionInfo<'_>, function_arg: &Argument, arg: &Expr) -> Vec<Diagnostic> {
        let mut diagnostics = vec![];
        if let Some(symbol) = function_arg.symbol.upgrade() {
            if symbol.borrow().evaluations().unwrap_or(&vec![]).len() == 1 {
                match symbol.borrow().evaluations().unwrap()[0].symbol.sym.clone() {
                    EvaluationSymbolPtr::DOMAIN => {
                        diagnostics.extend(Evaluation::validate_domain(session, arg));
                    },
                    _ => {}
                }
            }
        }
        diagnostics
    }
}

impl EvaluationSymbol {

    pub fn new_with_symbol(symbol: Weak<RefCell<Symbol>>, instance: bool, context: Context, factory: Option<Weak<RefCell<Symbol>>>, get_symbol_hook: Option<GetSymbolHook>) -> Self {
        Self { sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: symbol, instance: Some(instance), is_super: false}), context, factory, get_symbol_hook }
    }

    pub fn new_self(context: Context, factory: Option<Weak<RefCell<Symbol>>>, get_symbol_hook: Option<GetSymbolHook>) -> EvaluationSymbol {
        Self {
            sym: EvaluationSymbolPtr::SELF,
            context,
            factory,
            get_symbol_hook,
        }
    }

    pub fn is_instance(&self) -> Option<bool> {
        match &self.sym {
            EvaluationSymbolPtr::ANY => None,
            EvaluationSymbolPtr::ARG(_) => None,
            EvaluationSymbolPtr::NONE => None,
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
            EvaluationSymbolPtr::WEAK(ref mut w) => w,
            _ => panic!("Not an EvaluationSymbolWeak")
        }
    }

    pub fn get_symbol(&self, session: &mut SessionInfo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<Rc<RefCell<Symbol>>>) -> EvaluationSymbolWeak {
        let mut full_context = self.context.clone();
        //extend with local elements
        if let Some(context) = context {
            full_context.extend(context.clone());
        }
        if self.get_symbol_hook.is_some() {
            let hook = self.get_symbol_hook.unwrap();
            return hook(session, self, &mut Some(full_context), diagnostics, file_symbol);
        }
        match &self.sym {
            EvaluationSymbolPtr::WEAK(w) => {
                w.clone()
            },
            EvaluationSymbolPtr::ANY => EvaluationSymbolWeak{weak: Weak::new(), instance: Some(false), is_super: false},
            EvaluationSymbolPtr::ARG(_) => EvaluationSymbolWeak{weak: Weak::new(), instance: Some(false), is_super: false},
            EvaluationSymbolPtr::NONE => EvaluationSymbolWeak{weak: Weak::new(), instance: Some(false), is_super: false},
            EvaluationSymbolPtr::DOMAIN => EvaluationSymbolWeak{weak: Weak::new(), instance: Some(false), is_super: false},
            EvaluationSymbolPtr::SELF => {
                match full_context.get(&S!("parent")) {
                    Some(p) => {
                        match p {
                            ContextValue::SYMBOL(s) => EvaluationSymbolWeak{weak: s.clone(), instance: Some(true), is_super: false},
                            _ => EvaluationSymbolWeak{weak: Weak::new(), instance: Some(false), is_super: false}
                        }
                    },
                    None => EvaluationSymbolWeak{weak: Weak::new(), instance: Some(false), is_super: false}
                }
            }
        }
    }
}
