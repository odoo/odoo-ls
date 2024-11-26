use ruff_python_ast::{Expr, Identifier, Operator, Parameter};
use ruff_text_size::{Ranged, TextRange, TextSize};
use lsp_types::Diagnostic;
use weak_table::traits::WeakElement;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use crate::constants::*;
use crate::core::odoo::SyncOdoo;
use crate::threads::SessionInfo;
use crate::S;

use super::file_mgr::FileMgr;
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

#[derive(Debug, Clone, Copy)]
pub enum EvaluationSymbolType {
    Instance,
    Class,
    Super
}

#[derive(Debug, Clone)]
pub struct EvaluationSymbolWeak {
    pub weak: Weak<RefCell<Symbol>>,
    pub symbol_type: EvaluationSymbolType,
}

#[derive(Debug, Default, Clone)]
enum EvaluationSymbolPtr {
    WEAK(EvaluationSymbolWeak),
    SELF,
    ARG(u32),
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
                    symbol_type: EvaluationSymbolType::Instance,
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
                    symbol_type: EvaluationSymbolType::Instance,
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
                    symbol_type: EvaluationSymbolType::Instance,
                }),
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
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: symbol,
                    symbol_type: EvaluationSymbolType::Instance
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

    pub fn get_eval_out_of_function_scope(&self, session: &mut SessionInfo, function: &Rc<RefCell<Symbol>>) -> Vec<Evaluation> {
        let mut res = vec![];
        match self.symbol.sym {
            EvaluationSymbolPtr::WEAK(_) => {
                //take the weak by get_symbol instead of the match
                let symbol_eval_weak = self.symbol.get_symbol(session, &mut None, &mut vec![], None);
                if let Some(sym_up) = symbol_eval_weak.weak.upgrade() {
                    let out_of_scope = Symbol::follow_ref(&sym_up, session, &mut None, true, false, Some(function.clone()), &mut vec![]);
                    for weak_sym in out_of_scope {
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
            EvaluationSymbolPtr::SELF | EvaluationSymbolPtr::ARG(_) | EvaluationSymbolPtr::NONE | EvaluationSymbolPtr::ANY => {
                res.push(self.clone());
            },
        }
        res
    }

    pub fn follow_ref_and_get_value(&self, session: &mut SessionInfo, context: &mut Option<Context>, diagnostics: &mut Vec<Diagnostic>) -> Option<EvaluationValue> {
        if self.value.is_some() {
            Some(self.value.as_ref().unwrap().clone())
        } else {
            let symbol = self.symbol.get_symbol(session, &mut None, diagnostics, None).weak;
            if symbol.is_expired() {
                return None;
            }
            let evals = Symbol::follow_ref(&symbol.upgrade().unwrap(), session, context, false, true, None, diagnostics);
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
            res.push(Evaluation::eval_from_symbol(&Rc::downgrade(&sym)));
        }
        res
    }

    //create an evaluation that is evaluating to the given symbol
    pub fn eval_from_symbol(symbol: &Weak<RefCell<Symbol>>) -> Evaluation{
        if symbol.is_expired() {
            return Evaluation::new_none();
        }
        let symbol_type = match symbol.upgrade().unwrap().borrow().typ(){
            SymType::VARIABLE => EvaluationSymbolType::Instance,
            _ => EvaluationSymbolType::Class
        };
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: symbol.clone(),
                    symbol_type,
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
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let mut context = Some(base_eval[0].symbol.context.clone());
                //TODO context should give params
                let base_sym_weak_eval= base_eval[0].symbol.get_symbol(session, &mut context, &mut diagnostics, None);
                let base_sym = base_sym_weak_eval.weak.upgrade();
                if let Some(base_sym) = base_sym {
                    if base_sym.borrow().typ() == SymType::CLASS {
                        if matches!(base_sym_weak_eval.symbol_type, EvaluationSymbolType::Instance) {
                            //TODO handle call on class instance
                        } else {
                            if base_sym.borrow().get_tree() == (vec![S!("builtins")], vec![S!("super")]){
                                match parent.borrow().get_in_parents(&vec![SymType::CLASS], true){
                                    None => (), // TODO, diagnostic? or just leave empty
                                    Some(parent_class) =>
                                        evals.push(Evaluation{
                                            symbol: EvaluationSymbol {
                                                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                                                    weak: parent_class.clone(),
                                                    symbol_type: EvaluationSymbolType::Super,
                                                }),
                                                context: HashMap::new(),
                                                factory: None,
                                                get_symbol_hook: None,
                                            },
                                            value: None,
                                            range: Some(expr.range)
                                        })
                                }
                            } else {
                                //TODO diagnostic __new__ call parameters
                                evals.push(Evaluation{
                                    symbol: EvaluationSymbol {
                                        sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                                            weak: base_sym_weak_eval.weak.clone(),
                                            symbol_type: EvaluationSymbolType::Instance,
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
                let bases = Symbol::follow_ref(&base_ref.weak.upgrade().unwrap(), session, &mut None, false, false, None, &mut diagnostics);
                for ibase in bases.iter() {
                    let base_loc = ibase.weak.upgrade();
                    if let Some(base_loc) = base_loc {
                        let (attributes, mut attributes_diagnostics) = base_loc.borrow().get_member_symbol(session, &expr.attr.to_string(), module.clone(), false, true, matches!(base_ref.symbol_type, EvaluationSymbolType::Super));
                        for diagnostic in attributes_diagnostics.iter_mut(){
                            diagnostic.range = FileMgr::textRange_to_temporary_Range(&expr.range())
                        }
                        diagnostics.extend(attributes_diagnostics);
                        if !attributes.is_empty() {
                            let mut eval = Evaluation::eval_from_symbol(&Rc::downgrade(attributes.first().unwrap()));
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
                    evals.push(Evaluation::eval_from_symbol(&Rc::downgrade(infered_sym)));
                }
            },
            ExprOrIdent::Expr(Expr::Subscript(sub)) => {
                let (eval_left, diags) = Evaluation::eval_from_ast(session, &sub.value, parent.clone(), max_infer);
                diagnostics.extend(diags);
                if eval_left.len() != 1 || eval_left[0].symbol.get_symbol(session, &mut None, &mut diagnostics, None).weak.is_expired() { //TODO set context?
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base = &eval_left[0].symbol.get_symbol(session, &mut None, &mut diagnostics, None).weak; //TODO set context?
                let bases = Symbol::follow_ref(&base.upgrade().unwrap(), session, &mut None, false, false, None, &mut diagnostics);
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
                                    evals.push(Evaluation::eval_from_symbol(&hook_result.weak));
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
}

impl EvaluationSymbol {

    pub fn new_with_symbol(symbol: Weak<RefCell<Symbol>>, symbol_type: EvaluationSymbolType, context: Context, factory: Option<Weak<RefCell<Symbol>>>, get_symbol_hook: Option<GetSymbolHook>) -> Self {
        Self { sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: symbol, symbol_type}), context, factory, get_symbol_hook }
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
            EvaluationSymbolPtr::WEAK(w) => Some(matches!(w.symbol_type, EvaluationSymbolType::Instance))
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
            EvaluationSymbolPtr::ANY | EvaluationSymbolPtr::ARG(_) | EvaluationSymbolPtr::NONE => EvaluationSymbolWeak{weak: Weak::new(), symbol_type: EvaluationSymbolType::Class},
            EvaluationSymbolPtr::SELF => {
                match full_context.get(&S!("parent")) {
                    Some(p) => {
                        match p {
                            ContextValue::SYMBOL(s) => EvaluationSymbolWeak{weak: s.clone(), symbol_type: EvaluationSymbolType::Instance},
                            _ => EvaluationSymbolWeak{weak: Weak::new(), symbol_type: EvaluationSymbolType::Class}
                        }
                    },
                    None => EvaluationSymbolWeak{weak: Weak::new(), symbol_type: EvaluationSymbolType::Class}
                }
            }
        }
    }
}
