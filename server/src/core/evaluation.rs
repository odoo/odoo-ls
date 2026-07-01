use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::evaluation_context::{Context, ContextKey, ContextValue};
use crate::core::evaluation_utils::DeepFieldEvalWalker;
use crate::core::odoo::SyncOdoo;
use crate::core::python_odoo_builder::ACCESS_OPERATOR_OPTIONS;
use crate::core::symbols::storage::xml::xml_field_symbol::XmlFieldName;
use crate::core::symbols::symbol_keys::{FunctionKey, KeyValidator, ModuleKey, SourceFileKey, SymbolKey, Wk};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::FunctionSymbol;
use crate::features::references::ReferenceTarget;
use crate::threads::SessionInfo;
use crate::{constants::*, Sy};
use itertools::FoldWhile::{Continue, Done};
use itertools::Itertools;
use lsp_types::{Diagnostic, Location, Position, Range};
use ruff_python_ast::{
    Expr, ExprCall, FStringPart, Identifier, Number, Parameter, UnaryOp,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use std::cmp::{max, min};
use crate::utils::{HashMap, HashSet};
use std::i32;

use super::file_mgr::FileMgr;
use super::symbols::function_symbol::{Argument, ArgumentType};
use super::symbols::symbol_mgr::SectionIndex;

#[derive(Debug, Clone, PartialEq)]
pub enum EvaluationValue {
    ANY(), //we don't know what it is, so it can be everything !
    CONSTANT(Box<ruff_python_ast::Expr>), //expr is a literal
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

    /// Returns the inner string literal if this is a `CONSTANT(Expr::StringLiteral(_))`.
    pub fn as_string_literal(&self) -> Option<&ruff_python_ast::ExprStringLiteral> {
        match self {
            EvaluationValue::CONSTANT(e) => match e.as_ref() {
                Expr::StringLiteral(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    /// Returns the boolean value if this is a `CONSTANT(Expr::BooleanLiteral(_))`.
    pub fn as_bool_literal(&self) -> Option<bool> {
        match self {
            EvaluationValue::CONSTANT(e) => match e.as_ref() {
                Expr::BooleanLiteral(b) => Some(b.value),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_dict(&self) -> &[(ruff_python_ast::Expr, ruff_python_ast::Expr)] {
        match self {
            EvaluationValue::DICT(d) => d,
            _ => panic!("Not a dict")
        }
    }

    pub fn as_list(&self) -> &[ruff_python_ast::Expr] {
        match self {
            EvaluationValue::LIST(l) => l,
            _ => panic!("Not a list")
        }
    }

    pub fn as_tuple(&self) -> &[ruff_python_ast::Expr] {
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

/**
 * A hook will receive:
 * session: current active session
 * eval: the evaluationSymbol the hook is executed on
 * context: if provided, can contains useful information
 * diagnostics: a vec the hook can fill to add diagnostics
 * file_symbol: if provided, can be used to add dependencies
 */
type GetSymbolHookCallable = fn (session: &mut SessionInfo, eval: &EvaluationSymbol, context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> Option<EvaluationSymbolPtr>;

#[derive(Debug, Clone)]
pub struct GetSymbolHook {
    pub callable: GetSymbolHookCallable,
    pub name: HookName,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HookName {
    EvalEnvGetItem,
    EvalRegistryGetItem,
    EvalInit,
    EvalGet,
    EvalRelational,
    EvalInitRelational,
    EvalInitRelationalOne2many,
    EvalEnvRef,
}


impl PartialEq for GetSymbolHook {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}


#[derive(Debug, Clone)]
pub struct EvaluationSymbolWeak {
    pub weak: Wk<SymbolKey>,
    pub context: Context,
    pub instance: Option<bool>,
    pub is_super: bool,
}

impl PartialEq for EvaluationSymbolWeak {
    fn eq(&self, other: &Self) -> bool {
        self.instance == other.instance
        && self.is_super == other.is_super
        && self.weak == other.weak
        && self.context == other.context
    }
}

impl EvaluationSymbolWeak {
    pub fn new(key: impl Into<Wk<SymbolKey>>, instance: Option<bool>, is_super: bool) -> Self {
        EvaluationSymbolWeak {
            weak: key.into(),
            context: Context::default(),
            instance,
            is_super
        }
    }

    pub fn is_instance(&self) -> Option<bool> {
        self.instance
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum EvaluationSymbolPtr {
    WEAK(EvaluationSymbolWeak),
    SELF(EvaluationSymbolWeak),
    // Weak symbol is the current symbol pointed to self. Under subclasses this would get overridden on evaluation
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

/// Push a hit for the active `evaluation_search` at `range` inside `parent`'s
/// file. Duplicates (e.g. a Name pushed by both its own analysis and the
/// enclosing Attribute that contains it) are removed once at end-of-walk
/// in `references_in_file`, which is cheaper than an O(N) scan per push.
/// Returns `true` if the hit was pushed; `false` when the file or its file_info
/// cannot be resolved, so callers can avoid claiming a match in that case.
fn record_evaluation_hit(session: &mut SessionInfo, parent: SymbolKey, range: TextRange) -> bool {
    let Some(file) = session.st().get_file(parent) else { return false };
    let file_path = session.st().path(file).to_string();
    let Some(file_info) = session.sync_odoo.get_file_mgr().borrow().get_file_info(&file_path) else { return false };
    let transformed_range = file_info.borrow().text_range_to_range(&range, session.sync_odoo.encoding);
    let uri = FileMgr::pathname2uri(&file_path);
    session.sync_odoo.evaluation_locations.push(Location { uri, range: transformed_range });
    true
}

impl Evaluation {

    pub fn new_list(odoo: &mut SyncOdoo, values: Option<Vec<Expr>>, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: odoo.get_ts_list(),
                    context: Context::default(),
                    instance: Some(true),
                    is_super: false,
                }),
                get_symbol_hook: None
            },
            value: values.map(EvaluationValue::LIST),
            range: Some(range),
        }
    }

    pub fn new_tuple(odoo: &mut SyncOdoo, values: Option<Vec<Expr>>, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: odoo.get_ts_tuple(),
                    context: Context::default(),
                    instance: Some(true),
                    is_super: false,
                }),
                get_symbol_hook: None
            },
            value: values.map(EvaluationValue::TUPLE),
            range: Some(range)
        }
    }

    pub fn new_dict(odoo: &mut SyncOdoo, values: Option<Vec<(Expr, Expr)>>, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: odoo.get_ts_dict(),
                    context: Context::default(),
                    instance: Some(true),
                    is_super: false,
                }),
                get_symbol_hook: None
            },
            value: values.map(EvaluationValue::DICT),
            range: Some(range)
        }
    }

    pub fn new_set(odoo: &mut SyncOdoo, range: TextRange) -> Evaluation {
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: odoo.get_ts_set(),
                    context: Context::default(),
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
        let symbol = match &values {
            Expr::StringLiteral(_s) => {
                odoo.get_ts_string()
            },
            Expr::BooleanLiteral(_b) => {
                odoo.get_ts_boolean()
            },
            Expr::NumberLiteral(_n) => {
                match _n.value {
                    Number::Float(_) => odoo.get_ts_float(),
                    Number::Int(_) => odoo.get_ts_int(),
                    Number::Complex { .. } => odoo.get_ts_complex(),
                }
            },
            Expr::BytesLiteral(_b) => {
                odoo.get_ts_bytes()
            },
            Expr::EllipsisLiteral(_e) => {
                odoo.get_ts_ellipsis()
            },
            Expr::NoneLiteral(_n) => {
                let mut eval = Evaluation::new_none();
                eval.range = Some(range);
                eval.value = Some(EvaluationValue::CONSTANT(Box::new(values)));
                return eval
            }
            _ => {
                odoo.get_ts_object()
            }
        };
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak {
                    weak: symbol,
                    context: Context::default(),
                    instance: Some(true),
                    is_super: false,
                }),
                get_symbol_hook: None
            },
            value: Some(EvaluationValue::CONSTANT(Box::new(values))),
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

    pub fn new_self(base: impl Into<Wk<SymbolKey>>, instance: Option<bool>) -> Self {
        let base = base.into();
        Self {
            symbol: EvaluationSymbol::new_self(None, base, instance),
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
    pub fn get_eval_out_of_function_scope(&self, session: &mut SessionInfo, function: FunctionKey) -> Vec<Evaluation> {
        let mut res = vec![];
        match self.symbol.sym {
            EvaluationSymbolPtr::WEAK(_) => {
                //take the weak by get_symbol instead of the match
                let symbol_eval = self.symbol.get_symbol(session, None, &mut vec![], Some(function.into()));
                let out_of_scope = SymbolTable::follow_ref(&symbol_eval, session, None, false, false, None, Some(function.into()));
                for sym in out_of_scope {
                    if !sym.is_expired_if_weak(&session.sync_odoo.symbol_table) {
                        res.push(Evaluation {
                            symbol: EvaluationSymbol {
                                sym,
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

    pub fn follow_ref_and_get_value(&self, session: &mut SessionInfo, context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>) -> Option<EvaluationValue> {
        if let Some(value) = &self.value {
            return Some(value.clone());
        }
        let eval_symbol = self.symbol.get_symbol(session, None, diagnostics, None);
        if eval_symbol.is_expired_if_weak(session.st()) {
            return None;
        }
        let evals = SymbolTable::follow_ref(&eval_symbol, session, context, false, true, None, None);
        if evals.len() != 1 { return None; }
        let eval = &evals[0];
        let EvaluationSymbolPtr::WEAK(w) = eval else { return None; };
        let eval_sym = w.weak.upgrade(session.st())?;
        let evals = session.st().evaluations(eval_sym)?;
        if evals.len() == 1 {
            return evals[0].value.clone();
        };
        None
    }

    ///Return a list of evaluations of the symbol that hold these sections.
    ///For example:
    /// if X:
    ///     i=5
    /// else:
    ///     i="test"
    /// It will return two evaluation for i, one with 5 and one for "test"
    pub fn from_sections(symbol_table: &SymbolTable, parent_key: SymbolKey, sections: &HashMap<u32, Vec<SymbolKey>>) -> Vec<Evaluation> {
        let parent_sym_mgr = symbol_table.as_symbol_mgr(parent_key);
        let mut res = vec![];
        let section = parent_sym_mgr.get_section_for(u32::MAX);
        let content_symbols = symbol_table.get_loc_symbol(parent_sym_mgr, sections, u32::MAX, &SectionIndex::INDEX(section.index), &mut HashSet::default());
        for sym_key in content_symbols.symbols {
            let mut is_instance = None;
            let sym_type = sym_key.typ();
            if matches!(sym_type, SymType::VARIABLE | SymType::FUNCTION) {
                for eval in symbol_table.evaluations(sym_key).unwrap().iter() {
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
            } else if matches!(sym_type, SymType::CLASS) {
                is_instance = Some(false);
            }
            res.push(Evaluation::eval_from_symbol(symbol_table, sym_key, is_instance));
        }
        res
    }

    /// Create an evaluation that is evaluating to the given symbol
    pub fn eval_from_symbol(symbol_table: &SymbolTable, symbol: impl Into<Wk<SymbolKey>>, instance: Option<bool>) -> Evaluation {
        let symbol: Wk<SymbolKey> = symbol.into();
        if symbol.is_expired(symbol_table) {
            return Evaluation::new_none();
        }
        Evaluation {
            symbol: EvaluationSymbol {
                sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                    weak: symbol,
                    context: Context::default(),
                    instance,
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
    pub fn eval_from_ast(session: &mut SessionInfo, ast: &Expr, parent: SymbolKey, max_infer: &TextSize, for_annotation: bool, required_dependencies: &mut Vec<Vec<SourceFileKey>>) -> (Vec<Evaluation>, Vec<Diagnostic>) {
        let from_module;
        if let Some(module) = session.sync_odoo.symbol_table.find_module(parent) {
            from_module = ContextValue::MODULE(module.into());
        } else {
            from_module = ContextValue::BOOLEAN(false);
        }
        let mut context = Context::from_iter([
            (ContextKey::Module, from_module),
            (ContextKey::Range, ContextValue::RANGE(ast.range()))
        ]);
        let analyze_result = Evaluation::analyze_ast(session, &ExprOrIdent::Expr(ast), parent, max_infer, &mut context, for_annotation, required_dependencies);
        (analyze_result.evaluations, analyze_result.diagnostics)
    }

    /* Given an Expr, try to return the represented String. None if it can't be achieved */
    pub fn expr_to_str(session: &mut SessionInfo, ast: &Expr, parent: SymbolKey, max_infer: &TextSize, for_annotation: bool, diagnostics: &mut Vec<Diagnostic>) -> (Option<String>, Vec<Diagnostic>) {
        let from_module;
        if let Some(module) = session.sync_odoo.symbol_table.find_module(parent) {
            from_module = ContextValue::MODULE(module.into());
        } else {
            from_module = ContextValue::BOOLEAN(false);
        }
        let mut context = Context::from_iter([
            (ContextKey::Module, from_module),
            (ContextKey::Range, ContextValue::RANGE(ast.range()))
        ]);
        let value = Evaluation::analyze_ast(session, &ExprOrIdent::Expr(ast), parent, max_infer, &mut context, for_annotation, &mut vec![]);
        if value.evaluations.len() == 1 { //only handle strict evaluations
            let eval = &value.evaluations[0];
            let v = eval.follow_ref_and_get_value(session, None, diagnostics);
            if let Some(v) = v
                && let EvaluationValue::CONSTANT(v) = v
                    && let Expr::StringLiteral(s) = *v {
                        return (Some(s.value.to_string()), value.diagnostics);
                    }
        }
        (None, value.diagnostics)
    }

    /* Given an Expr, try to return the represented Boolean. None if it can't be achieved */
    pub fn expr_to_bool(session: &mut SessionInfo, ast: &Expr, parent: SymbolKey, max_infer: &TextSize, for_annotation: bool, diagnostics: &mut Vec<Diagnostic>) -> (Option<bool>, Vec<Diagnostic>) {
        let from_module;
        if let Some(module) = session.sync_odoo.symbol_table.find_module(parent) {
            from_module = ContextValue::MODULE(module.into());
        } else {
            from_module = ContextValue::BOOLEAN(false);
        }
        let mut context = Context::from_iter([
            (ContextKey::Module, from_module),
            (ContextKey::Range, ContextValue::RANGE(ast.range()))
        ]);
        let value = Evaluation::analyze_ast(session, &ExprOrIdent::Expr(ast), parent, max_infer, &mut context, for_annotation, &mut vec![]);
        if value.evaluations.len() == 1 { //only handle strict evaluations
            let eval = &value.evaluations[0];
            let v = eval.follow_ref_and_get_value(session, None, diagnostics);
            if let Some(v) = v
                && let EvaluationValue::CONSTANT(v) = v
                    && let Expr::BooleanLiteral(s) = *v {
                        return (Some(s.value), value.diagnostics);
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
    pub fn analyze_ast(session: &mut SessionInfo, ast: &ExprOrIdent, parent: SymbolKey, max_infer: &TextSize, context: &mut Context, for_annotation: bool, required_dependencies: &mut Vec<Vec<SourceFileKey>>) -> AnalyzeAstResult {
        let mut evals = vec![];
        let mut diagnostics = vec![];
        let module = session.st().find_module(parent);
        let mut found_one_reference = false;

        let parent_file_or_func = session.st().parent_file_or_function(parent).unwrap();
        let is_in_validation = match parent_file_or_func.typ() {
            SymType::FILE | SymType::PACKAGE(_) | SymType::FUNCTION => {
                session.st().build_status(parent_file_or_func, BuildSteps::VALIDATION) == BuildStatus::IN_PROGRESS
            },
            _ => {false}
        };

        let odoo = &mut session.sync_odoo;

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
                let mut all_values = true;
                let mut values: Vec<ruff_python_ast::Expr> = Vec::new();
                for e in expr.elts.iter() {
                    if is_in_validation || session.sync_odoo.evaluation_search.is_some() {
                        let (_, diags) = Evaluation::eval_from_ast(session, e, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                    if all_values && e.is_literal_expr() {
                        values.push(e.clone());
                    } else {
                        values = Vec::new();
                        all_values = false;
                        if !is_in_validation && session.sync_odoo.evaluation_search.is_none() {
                            break;
                        }
                    }
                }
                evals.push(Evaluation::new_list(session.sync_odoo, Some(values), expr.range));
            },
            ExprOrIdent::Expr(Expr::Tuple(expr)) => {
                let mut all_values = true;
                let mut values: Vec<ruff_python_ast::Expr> = Vec::new();
                for e in expr.elts.iter() {
                    if is_in_validation || session.sync_odoo.evaluation_search.is_some() {
                        let (_, diags) = Evaluation::eval_from_ast(session, e, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                    if all_values && e.is_literal_expr() {
                        values.push(e.clone());
                    } else {
                        values = Vec::new();
                        all_values = false;
                        if !is_in_validation && session.sync_odoo.evaluation_search.is_none() {
                            break;
                        }
                    }
                }
                evals.push(Evaluation::new_tuple(session.sync_odoo, Some(values), expr.range));
            },
            ExprOrIdent::Expr(Expr::Set(expr)) => {
                evals.push(Evaluation::new_set(odoo, expr.range));
                if is_in_validation || session.sync_odoo.evaluation_search.is_some() {
                    for set_item in expr.elts.iter() {
                        let (_, diags) = Evaluation::eval_from_ast(session, set_item, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::Dict(expr)) => {
                let mut all_values = true;
                let mut values: Vec<(ruff_python_ast::Expr, ruff_python_ast::Expr)> = Vec::new();
                for dict_item in expr.iter() {
                    let dict_value = &dict_item.value;
                    if is_in_validation || session.sync_odoo.evaluation_search.is_some() {
                        let (_, diags) = Evaluation::eval_from_ast(session, dict_value, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                    match dict_item.key.as_ref() {
                        Some(key) => {
                            if is_in_validation || session.sync_odoo.evaluation_search.is_some() {
                                let (_, diags) = Evaluation::eval_from_ast(session, key, parent, max_infer, false, required_dependencies);
                                if is_in_validation {
                                    diagnostics.extend(diags);
                                }
                            }
                            if all_values && key.is_literal_expr() && dict_value.is_literal_expr() {
                                values.push((key.clone(), dict_value.clone()));
                            } else {
                                all_values = false;
                                if !is_in_validation && session.sync_odoo.evaluation_search.is_none() {
                                    break;
                                }
                            }
                        },
                        None => {
                            // do not handle dict unpacking
                            all_values = false;
                            if !is_in_validation && session.sync_odoo.evaluation_search.is_none() {
                                break;
                            }
                        }
                    }
                }
                evals.push(Evaluation::new_dict(session.sync_odoo, Some(values), expr.range));
            },
            ExprOrIdent::Expr(Expr::Call(expr)) => {
                // Check argument expressions for references and diagnostics
                if is_in_validation || odoo.evaluation_search.is_some() {
                    for param in expr.arguments.args.iter().chain(expr.arguments.keywords.iter().map(|k| &k.value)) {
                        let (_, diags) = Evaluation::eval_from_ast(session, param, parent, max_infer, false, required_dependencies);
                        diagnostics.extend(diags);
                    }
                }
                let (base_evals, diags) = Evaluation::eval_from_ast(session, &expr.func, parent, max_infer, false, required_dependencies);
                if is_in_validation {
                    diagnostics.extend(diags);
                }
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
                    let base_sym_weak_eval_base = base_eval.symbol.get_symbol(session, Some(context), &mut diagnostics, None);
                    SymbolTable::follow_ref(&base_sym_weak_eval_base, session, Some(context), true, false, None, None)
                }).flatten().collect();

                let mut call_argument_diagnostics = Vec::new();
                for base_eval_ptr in base_eval_ptrs.iter() {
                    call_argument_diagnostics.push(Vec::new()); //one list per evaluation
                    let EvaluationSymbolPtr::WEAK(base_sym_weak_eval) = base_eval_ptr else {continue};
                    let Some(base_sym) = base_sym_weak_eval.weak.upgrade(session.st()) else {continue};
                    if let SymbolKey::Class(_) = base_sym {
                        if base_sym_weak_eval.instance.unwrap_or(false) {
                            //TODO handle call on class instance
                        } else {
                            if session.sync_odoo.match_tree_from_any_entry(base_sym, (&["builtins"], &["super"])) {
                                //  - If 1st argument exists, we add that class with symbol_type Super
                                let super_class = if !expr.arguments.is_empty() {
                                    let (class_eval, diags) = Evaluation::eval_from_ast(session, &expr.arguments.args[0], parent, max_infer, false, required_dependencies);
                                    diagnostics.extend(diags);
                                    if class_eval.len() != 1 {
                                        return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                                    }
                                    let class_sym_weak_eval= class_eval[0].symbol.get_symbol_as_weak(session, Some(context), &mut diagnostics, None);
                                    let res = class_sym_weak_eval.weak.upgrade(session.st()).and_then(|class_sym|{
                                        let class_sym_weak_eval = &SymbolTable::follow_ref(&EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak::new(
                                            class_sym, None, false
                                        )), session, None, false, false, None, None)[0];
                                        if !matches!(class_sym_weak_eval.upgrade_weak(session.st()).unwrap(), SymbolKey::Class(_)) {
                                            return None;
                                        }
                                        let class_sym_weak_eval = class_sym_weak_eval.get_weak();
                                        if class_sym_weak_eval.instance.unwrap_or(false) {
                                            if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS01005, &[]) {
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
                                            if let SymbolKey::Function(f) = parent && session.st()[f].is_class_method {
                                                default_instance = false;
                                            }
                                            if expr.arguments.args.len() >= 2 {
                                                let (object_or_type_eval, diags) = Evaluation::eval_from_ast(session, &expr.arguments.args[1], parent, max_infer, false, required_dependencies);
                                                diagnostics.extend(diags);
                                                if object_or_type_eval.len() != 1 {
                                                    return Some((class_sym_weak_eval.weak, Some(default_instance)))
                                                }
                                                let object_or_type_weak_eval = &SymbolTable::follow_ref(
                                                    &object_or_type_eval[0].symbol.get_symbol(
                                                        session, Some(context), &mut diagnostics, Some(parent)),
                                                        session, None, false, false, None, None)[0];
                                                if object_or_type_weak_eval.has_weak() {
                                                    is_instance = Some(object_or_type_weak_eval.get_weak().instance.unwrap_or(default_instance));
                                                } else {
                                                    is_instance = Some(default_instance);
                                                }
                                            }
                                            Some((class_sym_weak_eval.weak, is_instance))
                                        }
                                    })
                                //  - Otherwise we get the encapsulating class
                                } else {
                                    match session.st().get_in_parents(parent, &[SymType::CLASS], true) {
                                        None => {
                                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01006, &[]) {
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
                                            if let SymbolKey::Function(f)  = parent {
                                                let func = &session.st()[f];
                                                if func.is_class_method {
                                                    instance = Some(false);
                                                }
                                                if func.is_static {
                                                    instance = None;
                                                }
                                            }
                                            Some((parent_class.into(), instance))
                                        }
                                    }
                                };
                                if let Some((super_class, instance)) = super_class{
                                    evals.push(Evaluation{
                                        symbol: EvaluationSymbol {
                                            sym: EvaluationSymbolPtr::SELF(EvaluationSymbolWeak{
                                                weak: super_class,
                                                context: Context::default(),
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
                                    let class_file = session.st().get_file(base_sym).unwrap();
                                    SyncOdoo::build_now(session, class_file, BuildSteps::ARCH_EVAL);
                                    if !session.st().is_external(class_file.into()) {
                                        required_dependencies[1].push(class_file);
                                    }
                                }
                                //1: find __init__ method
                                let init = SymbolTable::get_member_symbol(session, base_sym, "__init__", module, true, false, false, false, false);
                                let mut found_hook = false;
                                if let Some(&SymbolKey::Function(init)) = init.0.first() {
                                    SyncOdoo::ensure_func_evaluations(session, init);
                                    let init_eval = &session.st()[init].evaluations;
                                    //init will always return an instance of the class, so we are not searching the method to check its return type, but rather to check if there is
                                    //an hook on it. Hooks, can be used to use parameters for context (see relational fields for example).
                                    if init_eval.len() == 1 && init_eval[0].symbol.get_symbol_hook.is_some() {
                                        context.insert(ContextKey::ConstructingClass, ContextValue::SYMBOL(base_sym.into()));
                                        context.insert(ContextKey::Parameters, ContextValue::ARGUMENTS(expr.arguments.clone()));
                                        found_hook = true;
                                        let init_eval_sym = init_eval[0].symbol.clone();
                                        // We disable evaluation search during the get_symbol call to avoid duplicating references.
                                        // The references visitor and analyze_ast should visit the whole AST.
                                        // So any call in the hooks that calls analyze_ast will not contaminate the evaluation search.
                                        let cache_eval_search = session.sync_odoo.evaluation_search.clone();
                                        session.sync_odoo.evaluation_search = None;
                                        let init_result = init_eval_sym.get_symbol_as_weak(session, Some(context), &mut diagnostics, Some(session.st().get_file(parent).unwrap().into()));
                                        session.sync_odoo.evaluation_search = cache_eval_search;
                                        context.remove(ContextKey::ConstructingClass);
                                        context.remove(ContextKey::Parameters);
                                        evals.push(Evaluation {
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
                                if !found_hook {
                                    evals.push(Evaluation{
                                        symbol: EvaluationSymbol {
                                            sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak {
                                                weak: base_sym_weak_eval.weak,
                                                context: Context::default(),
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
                    } else if let SymbolKey::Function(f) = base_sym {
                        let base_sym_file = session.st().get_file(base_sym).unwrap();
                        let in_class = session.st().get_in_parents(base_sym, &[SymType::CLASS], true).is_some();
                        if required_dependencies.len() >= 2 && !in_class {
                            required_dependencies[1].push(base_sym_file);
                        }
                        // Ensure return-type evaluations are available: resolves type annotations
                        // (ARCH_EVAL) and, if still empty, infers from body (VALIDATION).
                        SyncOdoo::ensure_func_evaluations(session, f);


                        if required_dependencies.len() >= 3 && in_class {
                            required_dependencies[2].push(base_sym_file);
                        }
                        let (call_parent, base_is_self) = match (
                            base_sym_weak_eval.context.get(ContextKey::BaseAttr),
                            base_sym_weak_eval.context.get(ContextKey::BaseIsSelf),
                        )
                        {
                            (
                                Some(ContextValue::SYMBOL(s)),
                                Some(ContextValue::BOOLEAN(base_is_self)),
                            ) => (*s, *base_is_self),
                            _ => (Wk::null(), false)
                        };
                        if session.sync_odoo.evaluation_search.is_some() {
                            Evaluation::search_reference_in_arg(session, expr, parent);
                        }
                        if is_in_validation {
                            let on_instance = base_sym_weak_eval.context.get(ContextKey::IsAttrOfInstance).map(|v| v.as_bool());
                            call_argument_diagnostics.last_mut().unwrap().extend(Evaluation::validate_call_arguments(session,
                                f,
                                expr,
                                call_parent,
                                module,
                                on_instance,
                            ));
                        }
                        context.insert(ContextKey::BaseCall, ContextValue::SYMBOL(call_parent));
                        context.insert(ContextKey::BaseIsSelf, ContextValue::BOOLEAN(base_is_self));
                        context.insert(ContextKey::Parameters, ContextValue::ARGUMENTS(expr.arguments.clone()));
                        context.insert(ContextKey::IsInValidation, ContextValue::BOOLEAN(is_in_validation));
                        let evaluations = &session.st()[f].evaluations;
                        for eval in evaluations.clone() {
                            let eval_ptr = eval.symbol.get_symbol_weak_transformed(session, Some(context), &mut diagnostics, Some(session.st().get_file(parent).unwrap().into()));
                            evals.push(Evaluation{
                                symbol: EvaluationSymbol {
                                    sym: eval_ptr,
                                    get_symbol_hook: None,
                                },
                                value: None,
                                range: Some(expr.range)
                            });
                        }
                        // removing in reverse order is more efficient
                        context.remove(ContextKey::IsInValidation);
                        context.remove(ContextKey::Parameters);
                        context.remove(ContextKey::BaseIsSelf);
                        context.remove(ContextKey::BaseCall);
                    }
                }
                diagnostics.extend(Evaluation::process_argument_diagnostics(session, expr, call_argument_diagnostics, base_eval_ptrs.len()));
            },
            ExprOrIdent::Expr(Expr::Attribute(expr)) => {
                let (base_evals, diags) = Evaluation::eval_from_ast(session, &expr.value, parent, max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                if base_evals.is_empty() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                for base_eval in base_evals.iter() {
                    let base_ref = base_eval.symbol.get_symbol(session, Some(context), &mut diagnostics, Some(parent));
                    if base_ref.is_expired_if_weak(session.st()) {
                        return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                    }
                    let bases = SymbolTable::follow_ref(&base_ref, session, Some(context), false, false, None, None);
                    for ibase in bases.iter() {
                        let base_is_self = matches!(ibase, EvaluationSymbolPtr::SELF(_));
                        let base_loc = ibase.upgrade_weak(session.st());
                        if let Some(base_loc) = base_loc {
                            let file = session.st().get_file(base_loc);
                            if let Some(base_loc_file) = file {
                                SyncOdoo::build_now(session, base_loc_file, BuildSteps::ARCH_EVAL);
                                if session.st().in_workspace(base_loc_file.into()) {
                                    if required_dependencies.len() == 2 {
                                        required_dependencies[1].push(base_loc_file);
                                    } else if required_dependencies.len() == 3 {
                                        required_dependencies[2].push(base_loc_file);
                                    }
                                }
                            }
                            let is_super = ibase.has_weak() && ibase.get_weak().is_super;
                            let (attributes, mut attributes_diagnostics) = SymbolTable::get_member_symbol(session, base_loc, &expr.attr, module, false, false, false, true, is_super);
                            for diagnostic in attributes_diagnostics.iter_mut(){
                                diagnostic.range = FileMgr::textRange_to_temporary_Range(&expr.range())
                            }
                            diagnostics.extend(attributes_diagnostics);
                            if !attributes.is_empty() {
                                let is_instance = ibase.get_weak().instance.unwrap_or(false);
                                attributes.iter().for_each(|&attribute|{
                                    let instance = match attribute {
                                        SymbolKey::Class(_) => match for_annotation {
                                            true => Some(true),
                                            false => Some(false)
                                        },
                                        SymbolKey::Variable(_) => match for_annotation {
                                            // this is a variable, but a follow_ref would probably lead to a class,
                                            // and here, because of annotation, we know we want an instance
                                            true => Some(true),
                                            false => None
                                        }
                                        _ => None
                                    };
                                    let mut eval = Evaluation::eval_from_symbol(session.st(), attribute, instance);
                                    match eval.symbol.sym {
                                        EvaluationSymbolPtr::WEAK(ref mut weak) => {
                                            weak.context.insert(ContextKey::BaseAttr, ContextValue::SYMBOL(base_loc.into()));
                                            weak.context.insert(ContextKey::BaseIsSelf, ContextValue::BOOLEAN(base_is_self));
                                            weak.context.insert(ContextKey::IsAttrOfInstance, ContextValue::BOOLEAN(is_instance));
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
                        (SymbolTable::infer_name(odoo, parent, &name, Some(max_infer.to_u32())), name)
                    },
                    ExprOrIdent::Expr(Expr::Named(expr))  => {
                        match *expr.target {
                            Expr::Name(ref expr) => {
                                let name = expr.id.to_string();
                                (SymbolTable::infer_name(odoo, parent, &name, Some(expr.range.end().to_u32())), name)
                            },
                            _ => return AnalyzeAstResult::from_only_diagnostics(diagnostics)
                        }
                    },
                    ExprOrIdent::Ident(expr) => {
                        let name = expr.id.to_string();
                        (SymbolTable::infer_name(odoo, parent, &name, Some( max_infer.to_u32())), name)
                    },
                    ExprOrIdent::Parameter(expr) => {
                        let name = expr.name.id.to_string();
                        (SymbolTable::infer_name(odoo, parent, &name, Some( max_infer.to_u32())), name)
                    }
                    _ => {
                        unreachable!();
                    }
                };
                match ast {
                    ExprOrIdent::Expr(Expr::Named(expr))  => {
                        let (_, diags) = Evaluation::eval_from_ast(session, &expr.value, parent, max_infer, false, required_dependencies);
                        diagnostics.extend(diags.clone());
                    }
                    _ => {}
                }

                if inferred_syms.symbols.is_empty() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                for &inferred_sym in inferred_syms.symbols.iter() {
                    let instance = match inferred_sym {
                        SymbolKey::Class(_) => match for_annotation{
                            true => Some(true),
                            false => Some(false)
                        },
                        SymbolKey::Variable(_) => match for_annotation {
                            // this is a variable, but a follow_ref would probably lead to a class,
                            // and here, because of annotation, we know we want an instance
                            true => Some(true),
                            false => None
                        }
                        _ => None
                    };
                    evals.push(Evaluation::eval_from_symbol(session.st(), inferred_sym, instance));
                }
                if !inferred_syms.always_defined{
                    evals.push(Evaluation::new_unbound(name));
                }
            },
            ExprOrIdent::Expr(Expr::Subscript(sub)) => {
                let (eval_left, diags) = Evaluation::eval_from_ast(session, &sub.value, parent, max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                // TODO handle multiple eval_left
                if eval_left.is_empty() {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let base = &eval_left[0].symbol.get_symbol(session, Some(context), &mut diagnostics, Some(parent));
                if base.is_expired_if_weak(session.st()) {
                    return AnalyzeAstResult::from_only_diagnostics(diagnostics);
                }
                let bases = SymbolTable::follow_ref(base, session, None, false, false, None, None);
                let value = Evaluation::expr_to_str(session, &sub.slice, parent, max_infer, false, &mut diagnostics);
                diagnostics.extend(value.1);
                for base in bases.iter() {
                    match base {
                        EvaluationSymbolPtr::WEAK(base_sym_weak_eval) if base_sym_weak_eval.instance == Some(false) => {
                            if let Some(SymbolKey::Class(_)) = base.upgrade_weak(session.st()) {
                                // This is a Generic type (Field[int], or List[int]), for now we just return the main type/Class (Field/List)
                                // TODO: handle generic types
                                let mut new_base = base.clone();
                                if for_annotation {
                                    new_base.get_mut_weak().instance = Some(true);
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
                    if !base.has_weak() {
                        continue;
                    }
                    let base = base.upgrade_weak(session.st()).unwrap();
                    let get_item_symbols = SymbolTable::get_member_symbol(
                        session,
                        base,
                        "__getitem__",
                        session.st().find_module(parent),
                        false,
                        false,
                        true,
                        true,
                        false,
                    ).0;
                    for get_item in get_item_symbols {
                        let Some(evaluations) = session.st().evaluations(get_item) else {
                            continue;
                        };
                        for get_item_eval in evaluations.clone() {
                            if let Some(hook) = get_item_eval.symbol.get_symbol_hook.as_ref() {
                                if let Some(value) = &value.0 {
                                    context.insert(ContextKey::Args, ContextValue::STRING(value.clone()));
                                }
                                let old_range = context.remove(ContextKey::Range);
                                context.insert(ContextKey::Range, ContextValue::RANGE(sub.slice.range()));
                                context.insert(ContextKey::IsInValidation, ContextValue::BOOLEAN(is_in_validation));
                                let hook_result = (hook.callable)(session, &get_item_eval.symbol, Some(context), &mut diagnostics, Some(parent));
                                if let Some(hook_result) = hook_result {
                                    match hook_result {
                                        EvaluationSymbolPtr::WEAK(ref weak) => {
                                            if !weak.weak.is_expired(session.st()) {
                                                evals.push(Evaluation::eval_from_ptr(&hook_result));
                                            }
                                        },
                                        _ => {
                                            evals.push(Evaluation::eval_from_ptr(&hook_result));
                                        }
                                    }
                                }
                                context.remove(ContextKey::Args);
                                context.remove(ContextKey::IsInValidation);
                                context.insert(ContextKey::Range, old_range.unwrap());
                            }
                            if let EvaluationSymbolPtr::SELF(_) = get_item_eval.symbol.get_symbol_ptr() {
                                // Evaluate to the base itself
                                // For example for models, since you get the same type of recordset when subscripted
                                evals.push(Evaluation{
                                    symbol: EvaluationSymbol {
                                        sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: base.into(), context: Context::default(), instance: Some(true), is_super: false}),
                                        get_symbol_hook: None,
                                    },
                                    value: None,
                                    range: Some(sub.range())
                                });
                            }
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::BinOp(operator)) => {
                if is_in_validation || odoo.evaluation_search.is_some() {
                    let (_, diags) = Evaluation::eval_from_ast(session, &operator.left, parent, max_infer, false, required_dependencies);
                    diagnostics.extend(diags);
                    let (_, diags) = Evaluation::eval_from_ast(session, &operator.right, parent, max_infer, false, required_dependencies);
                    diagnostics.extend(diags);
                }
            },
            ExprOrIdent::Expr(Expr::If(if_expr)) => {
                let (_, diags) = Evaluation::eval_from_ast(session, &if_expr.test, parent, max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                let (body_evals, diags) = Evaluation::eval_from_ast(session, &if_expr.body, parent, max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                let (orelse_evals, diags) = Evaluation::eval_from_ast(session, &if_expr.orelse, parent, max_infer, false, required_dependencies);
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
                                    weak: odoo.get_ts_boolean(),
                                    context: Context::default(),
                                    instance: Some(true),
                                    is_super: false,
                                }),
                                get_symbol_hook: None
                            },
                            value: None,
                            range: Some(unary_operator.range()),
                        });
                        if is_in_validation || odoo.evaluation_search.is_some() { //Still evaluate if we are searching for something
                            let (_, diags) = Evaluation::eval_from_ast(session, &unary_operator.operand, parent, max_infer, for_annotation, required_dependencies);
                            if is_in_validation {
                                diagnostics.extend(diags);
                            }
                        }
                        break 'u_op_block
                    },
                };
                let (bases, diags) = Evaluation::eval_from_ast(session, &unary_operator.operand, parent, max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                for base in bases.into_iter(){
                    let base_sym_weak_eval= base.symbol.get_symbol_weak_transformed(session, Some(context), &mut diagnostics, None);
                    let base_eval_ptrs = SymbolTable::follow_ref(&base_sym_weak_eval, session, Some(context), true, false, None, None);
                    for base_eval_ptr in base_eval_ptrs.iter() {
                        let EvaluationSymbolPtr::WEAK(base_sym_weak_eval) = base_eval_ptr else {continue};
                        let Some(base_sym) = base_sym_weak_eval.weak.upgrade(session.st()) else {continue};
                        let (operator_functions, diags) = SymbolTable::get_member_symbol(session, base_sym, method, module, true, false, true, false, false);
                        diagnostics.extend(diags);
                        for operator_function in operator_functions.into_iter() {
                            for eval in session.st().evaluations(operator_function).unwrap_or(&vec![]).clone() {
                                let eval_ptr = eval.symbol.get_symbol_weak_transformed(session, Some(context), &mut diagnostics, Some(session.st().get_file(parent).unwrap().into()));
                                evals.push(Evaluation {
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
            ExprOrIdent::Expr(Expr::FString(f_string_expr)) => {
                evals.push(
                    Evaluation {
                        symbol: EvaluationSymbol {
                            sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{
                                weak: odoo.get_ts_string(),
                                context: Context::default(),
                                instance: Some(true),
                                is_super: false,
                            }),
                            get_symbol_hook: None
                        },
                        value: None,
                        range: None,
                    }
                );
                if is_in_validation || odoo.evaluation_search.is_some() {
                    let exprs = f_string_expr
                    .value
                    .iter()
                    .filter_map(|part|if let FStringPart::FString(expr) = part {Some(expr)} else {None})
                    .flat_map(|expr| expr.elements.interpolations())
                    .map(|i| &i.expression);
                    for expr in exprs {
                        let (_, diags) = Evaluation::eval_from_ast(session, expr, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::TString(t_string_expr)) => {
                if is_in_validation || odoo.evaluation_search.is_some() {
                    let exprs = t_string_expr
                    .value
                    .iter()
                    .flat_map(|expr| expr.elements.interpolations())
                    .map(|i| &i.expression);
                    for expr in exprs {
                        let (_, diags) = Evaluation::eval_from_ast(session, expr, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::BoolOp(bool_op_expr)) => {
                if is_in_validation || odoo.evaluation_search.is_some() {
                    for value in bool_op_expr.values.iter() {
                        let (_, diags) = Evaluation::eval_from_ast(session, value, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::Compare(compare_expr)) => {
                if is_in_validation || odoo.evaluation_search.is_some() {
                    let (_, diags) = Evaluation::eval_from_ast(session, &compare_expr.left, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    for value in compare_expr.comparators.iter() {
                        let (_, diags) = Evaluation::eval_from_ast(session, value, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                }
            },
            ExprOrIdent::Expr(Expr::Lambda(lambda_expr)) => {
                let lambda_sym = session.st().get_positioned_symbol(parent, "<lambda>", &lambda_expr.range);
                if let Some(lambda_sym) = lambda_sym {
                    if is_in_validation || session.sync_odoo.evaluation_search.is_some() {
                        if is_in_validation {
                            session.st_mut().set_build_status(lambda_sym, BuildSteps::VALIDATION, BuildStatus::IN_PROGRESS);
                        }
                        let (_, diags) = Evaluation::eval_from_ast(session, &lambda_expr.body, lambda_sym, &lambda_expr.body.range().start(), false, required_dependencies);
                        if is_in_validation {
                            session.st_mut().set_build_status(lambda_sym, BuildSteps::VALIDATION, BuildStatus::DONE);
                            diagnostics.extend(diags);
                        }
                    }
                    evals.push(Evaluation::eval_from_symbol(session.st(), lambda_sym, None));
                };
            },
            ExprOrIdent::Expr(Expr::Yield(yield_expr)) => {
                if let Some(ref expr) = yield_expr.value && (is_in_validation || odoo.evaluation_search.is_some()) {
                    let (_, diags) = Evaluation::eval_from_ast(session, expr, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                }
            },
            ExprOrIdent::Expr(Expr::YieldFrom(yield_from_expr)) =>{
                if is_in_validation || odoo.evaluation_search.is_some() {
                    let (_, diags) = Evaluation::eval_from_ast(session, &yield_from_expr.value, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                }},
            ExprOrIdent::Expr(Expr::Await(await_expr)) =>{
                let (evaluations, diags) = Evaluation::eval_from_ast(session, &await_expr.value, parent, max_infer, false, required_dependencies);
                diagnostics.extend(diags);
                evals.extend(evaluations.into_iter());
            },
            ExprOrIdent::Expr(Expr::Slice(slice_expr)) => {
                if is_in_validation || odoo.evaluation_search.is_some() {
                    if let Some(ref lower_expr) = slice_expr.lower {
                        let (_, diags) = Evaluation::eval_from_ast(session, lower_expr, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                    if let Some(ref upper_expr) = slice_expr.upper {
                        let (_, diags) = Evaluation::eval_from_ast(session, upper_expr, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                    if let Some(ref step_expr) = slice_expr.step {
                        let (_, diags) = Evaluation::eval_from_ast(session, step_expr, parent, max_infer, false, required_dependencies);
                        if is_in_validation {
                            diagnostics.extend(diags);
                        }
                    }
                }
            },
            // Todo: process comprehensions
            ExprOrIdent::Expr(Expr::ListComp(list_comp_expr)) => {
                evals.push(Evaluation::new_list(odoo, None, list_comp_expr.range));
                if is_in_validation || odoo.evaluation_search.is_some() {
                    let (_, diags) = Evaluation::eval_from_ast(session, &list_comp_expr.elt, parent, max_infer, false, required_dependencies);
                    if is_in_validation {
                        diagnostics.extend(diags);
                    }
                }
            },
            ExprOrIdent::Expr(Expr::SetComp(set_comp_expr)) => {
                evals.push(Evaluation::new_set(odoo, set_comp_expr.range));
                if is_in_validation || odoo.evaluation_search.is_some() {
                    let (_, diags) = Evaluation::eval_from_ast(session, &set_comp_expr.elt, parent, max_infer, false, required_dependencies);
                    if is_in_validation {
                        diagnostics.extend(diags);
                    }
                }
            },
            ExprOrIdent::Expr(Expr::Generator(generator_expr)) => {
                if is_in_validation || odoo.evaluation_search.is_some() {
                    let (_, diags) = Evaluation::eval_from_ast(session, &generator_expr.elt, parent, max_infer, false, required_dependencies);
                    if is_in_validation {
                        diagnostics.extend(diags);
                    }
                }
            },
            ExprOrIdent::Expr(Expr::DictComp(dict_comp_expr)) => {
                evals.push(Evaluation::new_dict(odoo, None, dict_comp_expr.range));
                if is_in_validation || odoo.evaluation_search.is_some() {
                    let (_, diags) = Evaluation::eval_from_ast(session, &dict_comp_expr.key, parent, max_infer, false, required_dependencies);
                    if is_in_validation {
                        diagnostics.extend(diags);
                    }
                    let (_, diags) = Evaluation::eval_from_ast(session, &dict_comp_expr.value, parent, max_infer, false, required_dependencies);
                    if is_in_validation {
                        diagnostics.extend(diags);
                    }
                }
            },
            // Nothing to do here
            ExprOrIdent::Expr(Expr::Starred(_starred_expr)) => {},
            ExprOrIdent::Expr(Expr::IpyEscapeCommand(_ipy_escape_command_expr)) =>{},
        }
        let evaluation_search = session.sync_odoo.evaluation_search.clone();
        if let Some(evaluation_search) = evaluation_search.as_ref() {
            for eval in evals.iter() {
                if found_one_reference {
                    //if we have multiple matches, it means that that ast can reference it multiple times, but we only want to know if that ast matches or not
                    break;
                }
                if eval.symbol.sym.has_weak() && let Some(weak) = eval.symbol.sym.get_weak().weak.upgrade(session.st())
                    && let Some(evaluation_search_sym) = evaluation_search.as_symbol() && weak == evaluation_search_sym {
                        found_one_reference |= record_evaluation_hit(session, parent, ast.range());
                    }
                if let Some(value) = eval.value.as_ref()
                    && let EvaluationValue::CONSTANT(c) = value
                        && let Expr::StringLiteral(constant) = c.as_ref() {
                            match evaluation_search {
                                ReferenceTarget::String(evaluation_search_string) => {
                                    if constant.value.to_str() == evaluation_search_string {
                                        found_one_reference |= record_evaluation_hit(session, parent, ast.range());
                                    }
                                },
                                ReferenceTarget::Symbol(evaluation_search_sym) => {
                                    if let SymbolKey::Class(class_key) = *evaluation_search_sym
                                        && let Some(model_data) = session.st()[class_key]._model.as_ref()
                                            && model_data.name == constant.value.to_str() {
                                                record_evaluation_hit(session, parent, constant.range);
                                            }
                                }
                            }
                        }
            }
        }
        AnalyzeAstResult { evaluations: evals, diagnostics }
    }

    /**
     * parameters:
     * object_instance: None if called on nothing, true on an instance, false on a class
     */
    fn validate_call_arguments(session: &mut SessionInfo, function_key: FunctionKey, expr_call: &ExprCall, on_object: Wk<SymbolKey>, from_module: Option<ModuleKey>, object_instance: Option<bool>) -> Vec<Diagnostic> {
        let function = &session.st()[function_key];
        if FunctionSymbol::is_func_overloaded(session.st(), function_key) || function.is_property {
            return vec![];
        }
        let mut diagnostics = vec![];
        let function_name = function.name.clone();
        //validate pos args first
        let mut arg_index = 0;
        let mut number_pos_arg = 0;
        let mut pos_only_args = HashSet::default();
        let mut kword_only_args = Vec::new();
        let mut vararg_index = i32::MAX;
        let mut kwarg_index = i32::MAX;
        for (index, arg) in function.args.iter().enumerate() {
            match arg.arg_type {
                ArgumentType::POS_ONLY => {
                    if arg.default_value.is_none() {
                        number_pos_arg += 1;
                        if let Some(arg_symbol) = arg.symbol.upgrade(session.st()) {
                            let func_arg_name = session.st()[arg_symbol].name.to_string();
                            pos_only_args.insert(func_arg_name);
                        }
                    }
                },
                ArgumentType::ARG => {
                    if arg.default_value.is_none() {
                        number_pos_arg += 1;
                    }
                }
                ArgumentType::VARARG => {
                    vararg_index = index as i32;
                },
                ArgumentType::KWORD_ONLY => {
                    kword_only_args.push(arg.clone());
                },
                ArgumentType::KWARG => {
                    kwarg_index = index as i32;
                }
            }
        }
        if !function.is_static
            && (object_instance.is_some_and(|x| x) || //on instance
             object_instance.is_some_and(|x| !x) && function.is_class_method) { //on classmethod
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
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01007, &[&function_name, &0.to_string(), &1.to_string()]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                    return diagnostics;
                }
                arg_index += 1;
            }
        for arg in expr_call.arguments.args.iter() {
            if arg.is_starred_expr() {
                //TODO try to unpack the starred
                return diagnostics;
            }
            //match arg with argument from function
            let function = &session.st()[function_key];
            let function_arg = function.args.get(min(arg_index, vararg_index) as usize);
            if function_arg.is_none() || function_arg.unwrap().arg_type == ArgumentType::KWORD_ONLY || function_arg.unwrap().arg_type == ArgumentType::KWARG {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01007, &[&function_name, &number_pos_arg.to_string(), &(arg_index + 1).to_string()]) {
                    diagnostics.push(Diagnostic {
                        range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                        ..diagnostic
                    });
                }
                return diagnostics;
            }
            if function_arg.unwrap().arg_type != ArgumentType::VARARG {
                //positional or arg
                diagnostics.extend(Evaluation::validate_func_arg(session, &function_arg.unwrap().clone(), arg, on_object, from_module));
            }
            arg_index += 1;
        }
        let min_arg_for_kword = arg_index;
        let mut found_pos_arg_with_kw = arg_index;
        let to_skip = min(min_arg_for_kword, vararg_index);
        for arg in expr_call.arguments.keywords.iter() {
            if let Some(arg_identifier) = &arg.arg { //if None, arg is a dictionary of keywords, like in self.func(a, b, **any_kwargs)
                // First, check if the keyword matches a positional-only parameter
                if pos_only_args.contains(&arg_identifier.id.to_string()) {
                    found_pos_arg_with_kw += 1; // We do not want to double report 1011 with 1007
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01011, &[&function_name, &arg_identifier.id]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                    continue;
                }
                let mut found_one = false;
                let function = &session.st()[function_key];
                for func_arg in function.args.iter().skip(to_skip as usize).cloned() {
                    let func_arg_name  = session.st().name(func_arg.symbol.upgrade(session.st()).unwrap()).to_string();
                    if func_arg_name == arg_identifier.id {
                        diagnostics.extend(Evaluation::validate_func_arg(session, &func_arg, &arg.value, on_object, from_module));
                        if func_arg.arg_type == ArgumentType::ARG {
                            found_pos_arg_with_kw += 1;
                        } else if func_arg.arg_type == ArgumentType::KWORD_ONLY {
                            kword_only_args.retain(|x| x.symbol != func_arg.symbol);
                        }
                        found_one = true;
                        break;
                    }
                }
                if !found_one && kwarg_index == i32::MAX
                    && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01008, &[&function_name, &arg_identifier.id]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
            } else {
                // if arg is None, it means that it is a **arg, which could replace all args (except pos-only args, of which some could have been set already)
                found_pos_arg_with_kw = number_pos_arg - (pos_only_args.len() as i32 - arg_index).max(0);
                kword_only_args.clear();
            }
        }
        if found_pos_arg_with_kw < number_pos_arg {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01007, &[&function_name, &number_pos_arg.to_string(), &arg_index.to_string()]) {
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
                let name = session.st().name(kword_only_arg.symbol.upgrade(session.st()).unwrap()).clone();
                kword_only_arg_missing.push(name);
            }
        }
        if !kword_only_arg_missing.is_empty()
            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS01010, &[&kword_only_arg_missing.join(", ")]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(expr_call.range().start().to_u32(), 0), Position::new(expr_call.range().end().to_u32(), 0)),
                    ..diagnostic
                });
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
                d.code == Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01010.to_string())) ||
                d.code == Some(lsp_types::NumberOrString::String(DiagnosticCode::OLS01011.to_string()))
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

    fn validate_domain(session: &mut SessionInfo, on_object: Wk<SymbolKey>, from_module: Option<ModuleKey>, value: &Expr) -> Vec<Diagnostic> {
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
                        Evaluation::validate_tuple_search_domain(session, on_object, from_module, &t.elts[0], &t.elts[1], &t.elts[2], &mut diagnostics);
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
                        Evaluation::validate_tuple_search_domain(session, on_object, from_module, &l.elts[0], &l.elts[1], &l.elts[2], &mut diagnostics);
                    }
                },
                Expr::StringLiteral(s) => {
                    match s.value.to_str() {
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
        if need_tuple > 0
            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03010, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(value.range().start().to_u32(), 0), Position::new(value.range().end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        diagnostics
    }

    fn validate_tuple_search_domain(session: &mut SessionInfo, on_object: Wk<SymbolKey>, from_module: Option<ModuleKey>, elt1: &Expr, elt2: &Expr, elt3: &Expr, diagnostics: &mut Vec<Diagnostic>) {
        //parameter 2
        let mut access_op = false;
        if let Expr::StringLiteral(s) = elt2 {
            match s.value.to_str() {
                "=" | "!=" | ">" | ">=" | "<" | "<=" | "=?" | "=like" | "like" | "not like" | "ilike" |
                "not ilike" | "=ilike" | "in" | "not in" | "child_of" | "parent_of" | "any" | "not any" => {},
                "access" => {
                    if session.sync_odoo.version < (19, 3) {
                        if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03025, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range::new(Position::new(s.range().start().to_u32(), 0), Position::new(s.range().end().to_u32(), 0)),
                                ..diagnostic_base
                            });
                        }
                    } else {
                        access_op = true;
                    }
                }
                _ => {
                    if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03009, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(s.range().start().to_u32(), 0), Position::new(s.range().end().to_u32(), 0)),
                            ..diagnostic_base
                        });
                    }
                }
            }
        }
        //parameter 1
        if let Some(on_object) = on_object.upgrade(session.st()) //if weak is not set, we didn't manage to evalue base object. Do not validate in this case
            && let Expr::StringLiteral(s) = elt1 {
            let value = s.value.to_str();
            let mut date_mode = false;
            let mut deep_field_walker = DeepFieldEvalWalker::new(on_object, from_module);
            let split_expr = value.split(".").collect::<Vec<_>>();
            'split_name: for (index, field_name) in split_expr.iter().enumerate() {
                if date_mode {
                    if !["year_number", "quarter_number", "month_number", "iso_week_number", "day_of_week", "day_of_month", "day_of_year", "hour_number", "minute_number", "second_number"].contains(field_name)
                        && let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03012, &[])
                    {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(s.range().start().to_u32(), 0), Position::new(s.range().end().to_u32(), 0)),
                            ..diagnostic_base
                        });
                    }
                    date_mode = false;
                    continue;
                }
                let Some(base_symbol) = deep_field_walker.get_model_symbol(session) else {
                    if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03013, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(s.range().start().to_u32(), 0), Position::new(s.range().end().to_u32(), 0)),
                            ..diagnostic_base
                        });
                    }
                    break;
                };
                let field_symbols =
                    deep_field_walker.get_model_fields(session, base_symbol, field_name);
                if field_symbols.is_empty() {
                    if let Some(diagnostic_base) = create_diagnostic(
                        session,
                        DiagnosticCode::OLS03011,
                        &[field_name, &session.st().repr(base_symbol)],
                    ) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(
                                Position::new(s.range().start().to_u32(), 0),
                                Position::new(s.range().end().to_u32(), 0),
                            ),
                            ..diagnostic_base
                        });
                    }
                    break;
                }
                let mut access_field_valid = *field_name == "id";
                for symbol in field_symbols {
                    match symbol {
                        SymbolKey::Variable(_) => {
                            if SymbolTable::is_specific_field(session, symbol, &["Properties"]) {
                                //TODO handle properties field
                                //property field, not handled for now. Skip the parsing to not generate diagnostics
                                break 'split_name
                            }
                            if SymbolTable::is_specific_field(session, symbol, &["Date", "Datetime"]) {
                                date_mode = true;
                            } else if !access_field_valid
                                && index == split_expr.len() - 1
                                && access_op
                                && SymbolTable::is_specific_field(session, symbol, &["Many2one"])
                            {
                                access_field_valid = true;
                            }
                        }
                        SymbolKey::XmlRecord(key) => {
                            let Some(ttype) =
                                session.st()[key].get_field_text(XmlFieldName::Type, session.st())
                            else {
                                continue;
                            };
                            if ["date", "datetime"].contains(&ttype.as_str()) {
                                date_mode = true;
                            } else if !access_field_valid
                                && index == split_expr.len() - 1
                                && access_op
                                && matches!(ttype.as_str(), "many2one")
                            {
                                access_field_valid = true;
                            }
                        }
                        _ => {}
                    }
                }
                if index == split_expr.len() - 1 && access_op && !date_mode && !access_field_valid
                    && let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03027, &[])
                    {
                        diagnostics.push(Diagnostic {
                            range: Range::new(
                                Position::new(s.range().start().to_u32(), 0),
                                Position::new(s.range().end().to_u32(), 0),
                            ),
                            ..diagnostic_base
                        });
                    }
            }
        }
        if access_op
          && let Expr::StringLiteral(str_expr) = elt3
          && !ACCESS_OPERATOR_OPTIONS.contains(&str_expr.value.to_str())
          && let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS03026, &[])
        {
            diagnostics.push(Diagnostic {
                range: Range::new(
                    Position::new(str_expr.range().start().to_u32(), 0),
                    Position::new(str_expr.range().end().to_u32(), 0),
                ),
                ..diagnostic_base
            });
        }
    }

    fn validate_func_arg(session: &mut SessionInfo<'_>, function_arg: &Argument, arg: &Expr, on_object: Wk<SymbolKey>, from_module: Option<ModuleKey>) -> Vec<Diagnostic> {
        let st = &session.sync_odoo.symbol_table;
        let mut diagnostics = vec![];
        let Some(symbol) = function_arg.symbol.upgrade(st) else { return diagnostics; };
        let evaluations = &st[symbol].evaluations;
        if evaluations.len() == 1
            && let EvaluationSymbolPtr::DOMAIN = evaluations[0].symbol.sym {
                diagnostics.extend(Evaluation::validate_domain(session, on_object, from_module, arg));
            }
        diagnostics
    }

    fn search_reference_in_arg(session: &mut SessionInfo, expr_call: &ExprCall, parent: SymbolKey) {
        for arg in expr_call.arguments.args.iter() {
            if arg.is_starred_expr() {
                continue;
            }
            Evaluation::eval_from_ast(session, arg, parent, &expr_call.range.start(), false, &mut vec![]);
        }
        for arg in expr_call.arguments.keywords.iter() {
            Evaluation::eval_from_ast(session, &arg.value, parent, &expr_call.range.start(), false, &mut vec![]);
        }
    }
}

impl EvaluationSymbol {

    pub fn new_with_symbol(symbol: Wk<SymbolKey>, instance: Option<bool>, context: Context, get_symbol_hook: Option<GetSymbolHook>) -> Self {
        Self { sym: EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: symbol, context, instance, is_super: false}), get_symbol_hook }
    }

    pub fn new_self(get_symbol_hook: Option<GetSymbolHook>, base: Wk<SymbolKey>, instance: Option<bool>) -> EvaluationSymbol {
        Self {
            sym: EvaluationSymbolPtr::SELF(EvaluationSymbolWeak{weak: base, context: Context::default(), instance, is_super: false}),
            get_symbol_hook,
        }
    }

    pub fn is_instance(&self) -> Option<bool> {
        match &self.sym {
            EvaluationSymbolPtr::ANY => None,
            EvaluationSymbolPtr::ARG(_) => None,
            EvaluationSymbolPtr::NONE => None,
            EvaluationSymbolPtr::UNBOUND(_) => None,
            EvaluationSymbolPtr::DOMAIN => Some(false), //domain is always used for types
            EvaluationSymbolPtr::SELF(w) | EvaluationSymbolPtr::WEAK(w) => w.instance,
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
    pub fn get_symbol_as_weak(&self, session: &mut SessionInfo, context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> EvaluationSymbolWeak {
        let eval = self.get_symbol(session, context, diagnostics, scope);
        match eval {
            EvaluationSymbolPtr::WEAK(w) => {
                w
            },
            EvaluationSymbolPtr::ANY
            | EvaluationSymbolPtr::ARG(_)
            | EvaluationSymbolPtr::NONE
            | EvaluationSymbolPtr::UNBOUND(_)
            | EvaluationSymbolPtr::DOMAIN => EvaluationSymbolWeak{ weak: Wk::null(), context: Context::default(), instance: Some(false), is_super: false },
            EvaluationSymbolPtr::SELF(_) => {
                let class = context.
                and_then(|context| context.get(ContextKey::ParentFor).or(context.get(ContextKey::BaseAttr)))
                .unwrap_or(&ContextValue::BOOLEAN(false));
                match class {
                    ContextValue::SYMBOL(s) => EvaluationSymbolWeak{weak: *s, context: Context::default(), instance: Some(true), is_super: false},
                    _ => EvaluationSymbolWeak{weak: Wk::null(), context: Context::default(), instance: Some(false), is_super: false}
                }
            }
        }
    }

    /* Execute Hook, then return the effective EvaluationSymbolPtr, but transformed as EvaluationSmbolWeak if possible */
    pub fn get_symbol_weak_transformed(&self, session: &mut SessionInfo, context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>, scope: Option<SymbolKey>) -> EvaluationSymbolPtr {
        let eval = self.get_symbol(session, context, diagnostics, scope);
        match eval {
            EvaluationSymbolPtr::WEAK(_) => {
                eval
            },
            EvaluationSymbolPtr::ANY => eval,
            EvaluationSymbolPtr::ARG(_) => eval,
            EvaluationSymbolPtr::NONE => eval,
            EvaluationSymbolPtr::UNBOUND(_) => eval,
            EvaluationSymbolPtr::DOMAIN => eval,
            EvaluationSymbolPtr::SELF(_) => {
                let default = EvaluationSymbolPtr::WEAK(EvaluationSymbolWeak{weak: Wk::null(), context: Context::default(), instance: Some(false), is_super: false});
                let class = context.as_ref().and_then(|context| context.get(ContextKey::BaseCall)).unwrap_or(&ContextValue::BOOLEAN(false));
                let class_sym = match class {
                    ContextValue::SYMBOL(s) => match s.upgrade(&session.sync_odoo.symbol_table) {
                        Some(sym) => sym,
                        None => {return default;}
                    },
                    _ => {return default;}
                };
                let eval_symbol_weak = EvaluationSymbolWeak{weak: class_sym.into(), context: Context::default(), instance: Some(true), is_super: false};
                let base_is_self = context
                    .as_ref()
                    .and_then(|context| context.get(ContextKey::BaseIsSelf))
                    .unwrap_or(&ContextValue::BOOLEAN(false))
                    .as_bool();
                if base_is_self {
                    EvaluationSymbolPtr::SELF(eval_symbol_weak)
                } else {
                    EvaluationSymbolPtr::WEAK(eval_symbol_weak)
                }
            }
        }
    }

    /* Execute Hook, then return the effective EvaluationSymbolPtr */
    pub fn get_symbol(&self, session: &mut SessionInfo, context: Option<&Context>, diagnostics: &mut Vec<Diagnostic>, file_symbol: Option<SymbolKey>) -> EvaluationSymbolPtr {
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

    pub fn is_expired_if_weak(&self, table: &impl KeyValidator<SymbolKey>) -> bool {
        match self {
            EvaluationSymbolPtr::WEAK(w) | EvaluationSymbolPtr::SELF(w) => w.weak.is_expired(table),
            _ => false
        }
    }

    pub fn upgrade_weak(&self, table: &impl KeyValidator<SymbolKey>) -> Option<SymbolKey> {
        match self {
            EvaluationSymbolPtr::WEAK(w) | EvaluationSymbolPtr::SELF(w) => w.weak.upgrade(table),
            _ => None
        }
    }

    pub(crate) fn has_weak(&self) -> bool {
        match self {
            EvaluationSymbolPtr::WEAK(_) | EvaluationSymbolPtr::SELF(_) => true,
            _ => false
        }
    }

    pub(crate) fn get_weak(&self) -> &EvaluationSymbolWeak {
        match self {
            EvaluationSymbolPtr::WEAK(w) | EvaluationSymbolPtr::SELF(w) => &w,
            _ => panic!("Not an EvaluationSymbolWeak")
        }
    }

    pub(crate) fn get_mut_weak(&mut self) -> &mut EvaluationSymbolWeak {
        match self {
            EvaluationSymbolPtr::WEAK(w) | EvaluationSymbolPtr::SELF(w) => w,
            _ => panic!("Not an EvaluationSymbolWeak")
        }
    }
}
