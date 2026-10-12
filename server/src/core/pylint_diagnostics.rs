//! PyLint-compatible diagnostics.
//!
//! This module re-implements, on top of the ruff AST, the PyLint messages that Odoo enables in
//! `test_lint/tests/test_pylint.py`, including Odoo's own checkers (`_odoo_checker_sql_injection`,
//! `_odoo_checker_gettext` and `_odoo_checker_unlink_override`).
//! Each PyLint message is exposed as the diagnostic code `pylint_<msgid>`.
//!
//! Everything PyLint-specific lives here. The rest of the server only uses:
//! - [`with_pylint_diagnostic_codes!`](crate::with_pylint_diagnostic_codes) to register the codes,
//! - [`parse_pylint_pragma`] to honor `# pylint: disable=...` comments,
//! - [`check_python_file`] to compute the diagnostics of a validated file.

use lsp_types::{Diagnostic, DiagnosticTag, Position, Range};
use ruff_python_ast::visitor::{walk_expr, walk_pattern, Visitor};
use ruff_python_ast::{
    CmpOp, Comprehension, ExceptHandler, Expr, ExprCall, ExprContext, ExprFString, ExprLambda, ExprName,
    FStringPart, InterpolatedStringElement, Operator, Parameters, Pattern, Stmt, StmtClassDef, StmtFunctionDef,
    TypeParam, TypeParams, UnaryOp,
};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::core::diagnostics::{create_diagnostic, DiagnosticCode};
use crate::core::file_mgr::{combine_noqa_info, FileInfo, FileInfoAst, NoqaInfo};
use crate::core::symbols::symbol_keys::{ClassKey, SourceFileKey, SymbolKey};
use crate::core::symbols::ClassSymbol;
use crate::threads::SessionInfo;
use crate::tree::TreeStrSlice;
use crate::utils::{HashMap, HashSet};

/// Appends the PyLint diagnostic codes to a `diagnostic_codes!` invocation:
/// `with_pylint_diagnostic_codes! { diagnostic_codes! { <other codes> } }`
#[macro_export]
macro_rules! with_pylint_diagnostic_codes {
    ($callback:ident ! { $($codes:tt)* }) => {
        $callback! {
            $($codes)*
            /**
             * PyLint `used-before-assignment`: a variable is used before it is assigned, either because
             * it is assigned later in the same scope, because it is only assigned in the `try` block of
             * the `except` handler using it, or because it is only imported under `if TYPE_CHECKING:`.
             */
            #[allow(non_camel_case_types)]
            pylint_E0601, DiagnosticSetting::Error, "Using variable '{0}' before assignment",
            /**
             * PyLint `undefined-variable`: the name is not defined in any accessible scope, nor a builtin.
             */
            #[allow(non_camel_case_types)]
            pylint_E0602, DiagnosticSetting::Error, "Undefined variable '{0}'",
            /**
             * PyLint `function-redefined`: a function, method or class is defined twice in the same scope.
             */
            #[allow(non_camel_case_types)]
            pylint_E0102, DiagnosticSetting::Error, "{0} already defined line {1}",
            /**
             * PyLint `unreachable`: the code follows a `return`, `raise`, `break`, `continue` or a call
             * to a function that never returns (`sys.exit()`, ...).
             */
            #[allow(non_camel_case_types)]
            pylint_W0101, DiagnosticSetting::Warning, "Unreachable code",
            /**
             * PyLint `eval-used`: the builtin `eval` is used. Use `ast.literal_eval` or Odoo's `safe_eval`.
             */
            #[allow(non_camel_case_types)]
            pylint_W0123, DiagnosticSetting::Warning, "Use of eval",
            /**
             * Odoo `sql-injection`: the query given to `cr.execute()` or `SQL()` is built from values that
             * may come from the user. See http://www.bobby-tables.com and use `execute(query, tuple(params))`
             * or `SQL()` parameters.
             */
            #[allow(non_camel_case_types)]
            pylint_E8501, DiagnosticSetting::Error, "Possible SQL injection risk {0}.",
            /**
             * Odoo `gettext-variable`: `_()` and `_lt()` must be called with a string literal, otherwise the
             * string can not be extracted for translation.
             * See https://www.odoo.com/documentation/latest/developer/howtos/translations.html#variables
             */
            #[allow(non_camel_case_types)]
            pylint_E8502, DiagnosticSetting::Error, "Bad usage of _, _lt function.",
            /**
             * Odoo `raise-unlink-override`: raising errors is not allowed inside `unlink` overrides.
             * Create a method decorated with `@api.ondelete(at_uninstall=False)` instead, and only use
             * `at_uninstall=True` if you know what you are doing.
             */
            #[allow(non_camel_case_types)]
            pylint_E8503, DiagnosticSetting::Error, "Raise inside unlink override.",
            /**
             * Odoo `gettext-placeholders`: use named placeholders (e.g. `%(name)s`) when a translated string
             * has several of them. This allows translators to reorder them if needed.
             */
            #[allow(non_camel_case_types)]
            pylint_E8505, DiagnosticSetting::Error, "Usage of _, _lt function with multiple unnamed placeholders",
            /**
             * Odoo `gettext-repr`: don't use `%r` to insert quotes in translated strings. Quotes differ between
             * languages: they must be part of the translated string.
             */
            #[allow(non_camel_case_types)]
            pylint_E8506, DiagnosticSetting::Error, "Usage of %r in _, _lt function",
            /**
             * Odoo `missing-gettext`: the static string given to an Odoo exception (`UserError`,
             * `ValidationError`, ...) must be wrapped in a gettext call.
             */
            #[allow(non_camel_case_types)]
            pylint_E8507, DiagnosticSetting::Error, "Static string passed to {0} without gettext call.",
        }
    };
}

/// The PyLint messages implemented by this module, as `(msgid, symbol, code)`.
const PYLINT_MESSAGES: &[(&str, &str, DiagnosticCode)] = &[
    ("E0601", "used-before-assignment", DiagnosticCode::pylint_E0601),
    ("E0602", "undefined-variable", DiagnosticCode::pylint_E0602),
    ("E0102", "function-redefined", DiagnosticCode::pylint_E0102),
    ("W0101", "unreachable", DiagnosticCode::pylint_W0101),
    ("W0123", "eval-used", DiagnosticCode::pylint_W0123),
    ("E8501", "sql-injection", DiagnosticCode::pylint_E8501),
    ("E8502", "gettext-variable", DiagnosticCode::pylint_E8502),
    ("E8503", "raise-unlink-override", DiagnosticCode::pylint_E8503),
    ("E8505", "gettext-placeholders", DiagnosticCode::pylint_E8505),
    ("E8506", "gettext-repr", DiagnosticCode::pylint_E8506),
    ("E8507", "missing-gettext", DiagnosticCode::pylint_E8507),
];

/*
 * PyLint pragmas
 */

/// Where a `# pylint:` pragma applies.
pub enum PylintPragmaScope {
    /// `# pylint: disable=...`: handled exactly like a `# noqa` comment on the same position.
    LikeNoqa,
    /// `# pylint: disable-next=...`: the next line only.
    NextLine,
    /// `# pylint: skip-file`: the whole file.
    File,
}

pub struct PylintPragma {
    pub scope: PylintPragmaScope,
    pub noqa: NoqaInfo,
}

/// Parse a `# pylint: disable=...` comment into the OdooLS codes it disables.
/// Both message ids (`W0123`) and symbols (`eval-used`) are accepted, as is `all`.
/// Returns None if the comment is not a pragma disabling a code implemented here.
pub fn parse_pylint_pragma(comment: &str) -> Option<PylintPragma> {
    let idx = comment.find("pylint:")?;
    if comment[..idx].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let content = comment[idx + "pylint:".len()..].split('#').next().unwrap_or("").trim();
    let (key, value) = content.split_once('=').map_or((content, ""), |(k, v)| (k.trim(), v.trim()));
    let all_codes = || PYLINT_MESSAGES.iter().map(|(_, _, code)| code.to_string()).collect::<Vec<_>>();
    let (scope, codes) = match key {
        "skip-file" => (PylintPragmaScope::File, all_codes()),
        "disable" | "disable-next" => {
            let mut codes = vec![];
            for item in value.split(',').map(str::trim) {
                if item == "all" {
                    codes = all_codes();
                    break;
                }
                if let Some((_, _, code)) = PYLINT_MESSAGES.iter().find(|(msgid, symbol, _)| msgid.eq_ignore_ascii_case(item) || *symbol == item) {
                    codes.push(code.to_string());
                }
            }
            let scope = if key == "disable" { PylintPragmaScope::LikeNoqa } else { PylintPragmaScope::NextLine };
            (scope, codes)
        }
        _ => return None,
    };
    if codes.is_empty() {
        return None;
    }
    Some(PylintPragma { scope, noqa: NoqaInfo::Codes(codes) })
}

/*
 * Entry point
 */

/// Compute the PyLint diagnostics of a Python file of the workspace.
/// The noqa of the file must be set in `session.current_noqa`; the ones of classes and functions are
/// read from `file_info`.
pub fn check_python_file(session: &mut SessionInfo, file: SourceFileKey, file_info: &FileInfo, file_info_ast: &FileInfoAst) -> Vec<Diagnostic> {
    let file_symbol: SymbolKey = file.into();
    if !session.st().in_workspace(file_symbol) {
        return vec![];
    }
    let (Some(stmts), Some(text_document)) = (file_info_ast.get_stmts(), file_info_ast.text_document.as_ref()) else {
        return vec![];
    };
    let file_path = session.sync_odoo.symbol_table.file_path(file).replace('\\', "/");
    let model = SemanticModel::build(stmts, &file_info.noqas_blocs);
    let line_of = |offset: TextSize| text_document.index().line_index(offset).get();
    let mut checker = Checker { session, file_symbol, model: &model, findings: vec![] };
    checker.check_names();
    checker.check_eval_used();
    checker.check_unreachable();
    checker.check_function_redefined(&line_of);
    checker.check_raise_unlink_override();
    let file_name = file_path.rsplit('/').next().unwrap_or("");
    if !(file_path.contains("/test_") || file_path.contains("/tests/")) {
        checker.check_gettext();
    }
    if !file_name.starts_with("test_") {
        let mut sql_checker = SqlInjectionChecker::new(&model, &file_path, &line_of);
        sql_checker.run();
        checker.findings.extend(sql_checker.findings);
    }
    checker.into_diagnostics()
}

/*
 * Semantic model: scopes, bindings and uses of names
 */

type ScopeId = usize;
const MODULE_SCOPE: ScopeId = 0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScopeKind {
    Module,
    Class,
    Function,
    Lambda,
    Comprehension,
    /// Scope of PEP 695 type parameters
    TypeParams,
}

struct Scope<'a> {
    kind: ScopeKind,
    parent: Option<ScopeId>,
    name: &'a str,
    /// Start of the node creating the scope
    start: TextSize,
    is_generator: bool,
    bindings: HashMap<&'a str, Vec<usize>>,
    globals: HashSet<&'a str>,
    nonlocals: HashSet<&'a str>,
    /// Names used in an assert statement of this scope or of a nested one
    asserted_names: HashSet<&'a str>,
}

impl Scope<'_> {
    /// Frames hold the locals of astroid: comprehensions and type parameters do not count.
    fn is_frame(&self) -> bool {
        !matches!(self.kind, ScopeKind::Comprehension | ScopeKind::TypeParams)
    }
}

/// Branch of a compound statement containing a node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Branch {
    IfBody,
    IfClause(usize),
    TryBody,
    TryHandler(usize),
    TryElse,
    TryFinally,
    LoopBody,
    LoopElse,
    With,
    MatchCase(usize),
}

/// One level of the statement nesting of a node inside its scope: the compound statement
/// (identified by its start offset) and the branch of it containing the node.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct PathElem {
    stmt: TextSize,
    branch: Branch,
}

fn branches_are_exclusive(a: Branch, b: Branch) -> bool {
    use Branch::*;
    matches!(
        (a, b),
        (IfBody, IfClause(_)) | (IfClause(_), IfBody) | (IfClause(_), IfClause(_))
            | (TryBody, TryHandler(_)) | (TryHandler(_), TryBody) | (TryHandler(_), TryHandler(_))
            | (TryElse, TryHandler(_)) | (TryHandler(_), TryElse)
    )
}

/// True if the two nodes can not be executed together (different branches of an if, or of a try).
fn are_exclusive(a: &[PathElem], b: &[PathElem]) -> bool {
    let common = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    match (a.get(common), b.get(common)) {
        (Some(x), Some(y)) if x.stmt == y.stmt => branches_are_exclusive(x.branch, y.branch),
        _ => false,
    }
}

enum BindingKind<'a> {
    Param,
    /// Simple target of an assignment, an annotated assignment or a named expression
    Assign { value: &'a Expr, is_assign_stmt: bool },
    /// Element of a tuple target of an assignment
    Unpack { value: &'a Expr, index: usize },
    For { iter: &'a Expr },
    AugAssign { target: &'a ExprName, value: &'a Expr },
    /// Annotated name without value
    AnnOnly,
    Comprehension { iter: &'a Expr },
    Import { qualname: String },
    FunctionDef(&'a StmtFunctionDef),
    ClassDef(&'a StmtClassDef),
    Other,
}

struct Binding<'a> {
    scope: ScopeId,
    kind: BindingKind<'a>,
    /// Range of the bound name
    range: TextRange,
    /// Offset used to report the line of the binding
    line_offset: TextSize,
    /// Offset from which the name is bound
    pos: TextSize,
    /// Start of the statement creating the binding
    stmt_start: TextSize,
    path: Vec<PathElem>,
    type_checking: bool,
    simple_ann_assign: bool,
    /// Test of the `if` statement directly containing a function or class definition
    parent_if_test: Option<&'a Expr>,
}

impl<'a> Binding<'a> {
    fn ctx(&self) -> Ctx<'_> {
        Ctx { scope: self.scope, pos: self.stmt_start, path: &self.path }
    }
}

struct Use<'a> {
    name: &'a str,
    range: TextRange,
    scope: ScopeId,
    path: Vec<PathElem>,
    in_annotation: bool,
    /// The use is under `if TYPE_CHECKING:`
    type_checking: bool,
    /// The use is in a `try` block catching `NameError`
    name_error_guarded: bool,
}

struct CallRecord<'a> {
    call: &'a ExprCall,
    scope: ScopeId,
    path: Vec<PathElem>,
}

struct ReturnRecord<'a> {
    value: Option<&'a Expr>,
    scope: ScopeId,
    pos: TextSize,
    path: Vec<PathElem>,
}

struct FunctionRecord<'a> {
    def: &'a StmtFunctionDef,
    body_scope: ScopeId,
}

/// A function or class definition enclosing a node
#[derive(Clone)]
struct DefFrame<'a> {
    name: &'a str,
    range: TextRange,
    is_class: bool,
    /// The definition is directly in the body of a class
    direct_in_class: bool,
}

struct RaiseRecord<'a> {
    range: TextRange,
    defs: Vec<DefFrame<'a>>,
}

enum Terminator<'a> {
    /// return, raise, break or continue
    Statement,
    /// Calls of a statement, that could be calls to functions that never return
    Calls(Vec<(&'a ExprCall, ScopeId)>),
}

struct UnreachableCandidate<'a> {
    terminator: Terminator<'a>,
    next: &'a Stmt,
}

struct SemanticModel<'a> {
    scopes: Vec<Scope<'a>>,
    bindings: Vec<Binding<'a>>,
    uses: Vec<Use<'a>>,
    calls: Vec<CallRecord<'a>>,
    returns: Vec<ReturnRecord<'a>>,
    functions: Vec<FunctionRecord<'a>>,
    raises: Vec<RaiseRecord<'a>>,
    unreachable_candidates: Vec<UnreachableCandidate<'a>>,
    /// Ranges of the functions and classes having a noqa comment
    noqa_ranges: Vec<(TextRange, NoqaInfo)>,
}

impl<'a> SemanticModel<'a> {
    fn build(stmts: &'a [Stmt], noqas_blocs: &HashMap<u32, NoqaInfo>) -> Self {
        let mut collector = Collector {
            model: SemanticModel {
                scopes: vec![Scope::new(ScopeKind::Module, None, "", TextSize::default())],
                bindings: vec![],
                uses: vec![],
                calls: vec![],
                returns: vec![],
                functions: vec![],
                raises: vec![],
                unreachable_candidates: vec![],
                noqa_ranges: vec![],
            },
            noqas_blocs,
            scope: MODULE_SCOPE,
            path: vec![],
            def_stack: vec![],
            stmt_calls: vec![],
            direct_if_test: None,
            in_annotation: false,
            type_checking: false,
            name_error_guard: 0,
            lambda_body_call: None,
            if_exp_test_start: None,
        };
        collector.visit_block(stmts);
        collector.model
    }

    fn binding(&self, id: usize) -> &Binding<'a> {
        &self.bindings[id]
    }

    fn parent_scope_skipping_classes(&self, scope: ScopeId) -> Option<ScopeId> {
        let mut parent = self.scopes[scope].parent?;
        while self.scopes[parent].kind == ScopeKind::Class {
            parent = self.scopes[parent].parent?;
        }
        Some(parent)
    }

    fn is_descendant(&self, mut scope: ScopeId, ancestor: ScopeId) -> bool {
        loop {
            if scope == ancestor {
                return true;
            }
            match self.scopes[scope].parent {
                Some(parent) => scope = parent,
                None => return false,
            }
        }
    }

    /// The scope where `name`, used in `from`, is bound, following Python rules
    fn resolve_scope(&self, name: &str, from: ScopeId) -> Option<ScopeId> {
        let mut current = from;
        loop {
            let scope = &self.scopes[current];
            if scope.kind != ScopeKind::Module && scope.globals.contains(name) {
                return self.scopes[MODULE_SCOPE].bindings.contains_key(name).then_some(MODULE_SCOPE);
            }
            if !scope.nonlocals.contains(name) && scope.bindings.contains_key(name) {
                return Some(current);
            }
            current = self.parent_scope_skipping_classes(current)?;
        }
    }

    /// The binding of `name` reaching `pos` in `scope`, if any
    fn resolve_binding(&self, name: &str, scope: ScopeId, pos: TextSize) -> Option<&Binding<'a>> {
        let found = self.resolve_scope(name, scope)?;
        let ids = &self.scopes[found].bindings[name];
        let id = ids.iter().rev().find(|&&id| self.bindings[id].pos <= pos).or(ids.last())?;
        Some(&self.bindings[*id])
    }

    /// Nearest scope holding the locals of `scope` (see [`Scope::is_frame`])
    fn frame(&self, mut scope: ScopeId) -> ScopeId {
        while !self.scopes[scope].is_frame() {
            scope = self.scopes[scope].parent.unwrap_or(MODULE_SCOPE);
        }
        scope
    }
}

impl<'a> Scope<'a> {
    fn new(kind: ScopeKind, parent: Option<ScopeId>, name: &'a str, start: TextSize) -> Self {
        Scope {
            kind,
            parent,
            name,
            start,
            is_generator: false,
            bindings: HashMap::default(),
            globals: HashSet::default(),
            nonlocals: HashSet::default(),
            asserted_names: HashSet::default(),
        }
    }
}

/// How the names of an assignment target are bound
#[derive(Clone, Copy)]
enum TargetCtx<'a> {
    Assign { value: &'a Expr, is_assign_stmt: bool },
    For { iter: &'a Expr },
    Comprehension { iter: &'a Expr },
    AnnOnly,
    Other,
}

impl<'a> TargetCtx<'a> {
    fn simple_kind(self) -> BindingKind<'a> {
        match self {
            TargetCtx::Assign { value, is_assign_stmt } => BindingKind::Assign { value, is_assign_stmt },
            TargetCtx::For { iter } => BindingKind::For { iter },
            TargetCtx::Comprehension { iter } => BindingKind::Comprehension { iter },
            TargetCtx::AnnOnly => BindingKind::AnnOnly,
            TargetCtx::Other => BindingKind::Other,
        }
    }

    fn unpack_kind(self, index: usize) -> BindingKind<'a> {
        match self {
            TargetCtx::Assign { value, is_assign_stmt: true } => BindingKind::Unpack { value, index },
            TargetCtx::For { iter } => BindingKind::For { iter },
            TargetCtx::Comprehension { iter } => BindingKind::Comprehension { iter },
            _ => BindingKind::Other,
        }
    }
}

struct SavedScope<'a> {
    scope: ScopeId,
    path: Option<Vec<PathElem>>,
    direct_if_test: Option<&'a Expr>,
    in_annotation: bool,
    type_checking: bool,
}

/// Builds the [`SemanticModel`] of a file in a single traversal
struct Collector<'a, 'n> {
    model: SemanticModel<'a>,
    noqas_blocs: &'n HashMap<u32, NoqaInfo>,
    scope: ScopeId,
    path: Vec<PathElem>,
    def_stack: Vec<DefFrame<'a>>,
    /// Calls found in the expressions of the statements being visited
    stmt_calls: Vec<Vec<(&'a ExprCall, ScopeId)>>,
    direct_if_test: Option<&'a Expr>,
    in_annotation: bool,
    type_checking: bool,
    name_error_guard: u32,
    lambda_body_call: Option<TextRange>,
    /// Start of the conditional expression whose test is being visited: it is evaluated first
    if_exp_test_start: Option<TextSize>,
}

impl<'a> Collector<'a, '_> {
    fn enter_scope(&mut self, kind: ScopeKind, name: &'a str, start: TextSize) -> SavedScope<'a> {
        let new_scope = self.model.scopes.len();
        self.model.scopes.push(Scope::new(kind, Some(self.scope), name, start));
        // Statement paths are relative to functions and classes, that contain statements
        let resets_statements = matches!(kind, ScopeKind::Function | ScopeKind::Class);
        let saved = SavedScope {
            scope: self.scope,
            path: resets_statements.then(|| std::mem::take(&mut self.path)),
            direct_if_test: self.direct_if_test.take(),
            in_annotation: std::mem::replace(&mut self.in_annotation, false),
            type_checking: if resets_statements { std::mem::replace(&mut self.type_checking, false) } else { self.type_checking },
        };
        self.scope = new_scope;
        saved
    }

    fn leave_scope(&mut self, saved: SavedScope<'a>) {
        self.scope = saved.scope;
        if let Some(path) = saved.path {
            self.path = path;
        }
        self.direct_if_test = saved.direct_if_test;
        self.in_annotation = saved.in_annotation;
        self.type_checking = saved.type_checking;
    }

    fn add_binding(&mut self, name: &'a str, range: TextRange, kind: BindingKind<'a>, pos: TextSize, stmt_start: TextSize) -> usize {
        let (scope, kind, pos, path) = if self.scope != MODULE_SCOPE && self.model.scopes[self.scope].globals.contains(name) {
            (MODULE_SCOPE, BindingKind::Other, TextSize::default(), vec![])
        } else {
            (self.scope, kind, pos, self.path.clone())
        };
        let id = self.model.bindings.len();
        self.model.bindings.push(Binding {
            scope,
            kind,
            range,
            line_offset: range.start(),
            pos,
            stmt_start,
            path,
            type_checking: self.type_checking,
            simple_ann_assign: false,
            parent_if_test: self.direct_if_test,
        });
        self.model.scopes[scope].bindings.entry(name).or_default().push(id);
        id
    }

    fn add_use(&mut self, name: &'a str, range: TextRange) {
        self.model.uses.push(Use {
            name,
            range,
            scope: self.scope,
            path: self.path.clone(),
            in_annotation: self.in_annotation,
            type_checking: self.type_checking,
            name_error_guarded: self.name_error_guard > 0,
        });
    }

    fn bind_target(&mut self, target: &'a Expr, ctx: TargetCtx<'a>, pos: TextSize, stmt_start: TextSize) {
        match target {
            Expr::Name(name) => {
                self.add_binding(name.id.as_str(), name.range, ctx.simple_kind(), pos, stmt_start);
            }
            Expr::Tuple(tuple) => self.bind_elements(&tuple.elts, ctx, pos, stmt_start),
            Expr::List(list) => self.bind_elements(&list.elts, ctx, pos, stmt_start),
            Expr::Starred(starred) => self.bind_target(&starred.value, TargetCtx::Other, pos, stmt_start),
            _ => self.visit_expr(target),
        }
    }

    fn bind_elements(&mut self, elts: &'a [Expr], ctx: TargetCtx<'a>, pos: TextSize, stmt_start: TextSize) {
        for (index, elt) in elts.iter().enumerate() {
            match elt {
                Expr::Name(name) => {
                    self.add_binding(name.id.as_str(), name.range, ctx.unpack_kind(index), pos, stmt_start);
                }
                Expr::Tuple(_) | Expr::List(_) => self.bind_target(elt, ctx, pos, stmt_start),
                _ => self.bind_target(elt, TargetCtx::Other, pos, stmt_start),
            }
        }
    }

    fn bind_parameters(&mut self, parameters: &'a Parameters, pos: TextSize) {
        for parameter in parameters.iter_non_variadic_params().map(|p| &p.parameter)
            .chain(parameters.vararg.as_deref())
            .chain(parameters.kwarg.as_deref())
        {
            self.add_binding(parameter.name.id.as_str(), parameter.name.range, BindingKind::Param, pos, pos);
        }
    }

    fn visit_parameter_defaults(&mut self, parameters: &'a Parameters) {
        for parameter in parameters.iter_non_variadic_params() {
            if let Some(default) = &parameter.default {
                self.visit_expr(default);
            }
        }
    }

    fn visit_parameter_annotations(&mut self, parameters: &'a Parameters) {
        for parameter in parameters.iter_non_variadic_params().map(|p| &p.parameter)
            .chain(parameters.vararg.as_deref())
            .chain(parameters.kwarg.as_deref())
        {
            if let Some(annotation) = &parameter.annotation {
                self.visit_annotation(annotation);
            }
        }
    }

    fn enter_type_params(&mut self, type_params: Option<&'a TypeParams>) -> Option<SavedScope<'a>> {
        let type_params = type_params?;
        let saved = self.enter_scope(ScopeKind::TypeParams, "", type_params.range.start());
        for type_param in type_params.type_params.iter() {
            let name = match type_param {
                TypeParam::TypeVar(t) => &t.name,
                TypeParam::TypeVarTuple(t) => &t.name,
                TypeParam::ParamSpec(t) => &t.name,
            };
            self.add_binding(name.id.as_str(), name.range, BindingKind::Other, type_params.range.start(), type_params.range.start());
        }
        Some(saved)
    }

    fn with_branch(&mut self, stmt: TextSize, branch: Branch, f: impl FnOnce(&mut Self)) {
        self.path.push(PathElem { stmt, branch });
        f(self);
        self.path.pop();
    }

    fn visit_branch(&mut self, stmt: TextSize, branch: Branch, body: &'a [Stmt], if_test: Option<&'a Expr>) {
        self.with_branch(stmt, branch, |s| {
            let old = std::mem::replace(&mut s.direct_if_test, if_test);
            s.visit_block(body);
            s.direct_if_test = old;
        });
    }

    fn visit_block(&mut self, body: &'a [Stmt]) {
        for (i, stmt) in body.iter().enumerate() {
            self.stmt_calls.push(vec![]);
            self.visit_statement(stmt);
            let calls = self.stmt_calls.pop().unwrap_or_default();
            let terminator = match stmt {
                Stmt::Return(_) | Stmt::Raise(_) | Stmt::Break(_) | Stmt::Continue(_) => Terminator::Statement,
                _ if !calls.is_empty() => Terminator::Calls(calls),
                _ => continue,
            };
            let mut next = body.get(i + 1);
            // `return` followed by `yield` is the idiom of empty generators
            if stmt.is_return_stmt() && next.is_some_and(|n| matches!(n, Stmt::Expr(e) if e.value.is_yield_expr())) {
                next = body.get(i + 2);
            }
            if let Some(next) = next {
                self.model.unreachable_candidates.push(UnreachableCandidate { terminator, next });
            }
        }
    }

    fn register_noqa(&mut self, range: TextRange) {
        if let Some(noqa) = self.noqas_blocs.get(&range.start().to_u32()) {
            self.model.noqa_ranges.push((range, noqa.clone()));
        }
    }

    fn is_direct_in_class(&self) -> bool {
        self.path.is_empty() && self.model.scopes[self.scope].kind == ScopeKind::Class
    }

    fn visit_function_def(&mut self, func: &'a StmtFunctionDef) {
        for decorator in func.decorator_list.iter() {
            self.visit_expr(&decorator.expression);
        }
        self.visit_parameter_defaults(&func.parameters);
        let direct_in_class = self.is_direct_in_class();
        let type_params_scope = self.enter_type_params(func.type_params.as_deref());
        self.visit_parameter_annotations(&func.parameters);
        if let Some(returns) = &func.returns {
            self.visit_annotation(returns);
        }
        if let Some(saved) = type_params_scope {
            self.leave_scope(saved);
        }
        self.add_binding(func.name.id.as_str(), func.name.range, BindingKind::FunctionDef(func), func.range.end(), func.range.start());
        self.register_noqa(func.range);
        let type_params_scope = self.enter_type_params(func.type_params.as_deref());
        self.def_stack.push(DefFrame { name: func.name.id.as_str(), range: func.range, is_class: false, direct_in_class });
        let saved = self.enter_scope(ScopeKind::Function, func.name.id.as_str(), func.range.start());
        self.model.functions.push(FunctionRecord { def: func, body_scope: self.scope });
        self.bind_parameters(&func.parameters, func.range.start());
        self.visit_block(&func.body);
        self.leave_scope(saved);
        self.def_stack.pop();
        if let Some(saved) = type_params_scope {
            self.leave_scope(saved);
        }
    }

    fn visit_class_def(&mut self, class: &'a StmtClassDef) {
        for decorator in class.decorator_list.iter() {
            self.visit_expr(&decorator.expression);
        }
        let direct_in_class = self.is_direct_in_class();
        let type_params_scope = self.enter_type_params(class.type_params.as_deref());
        if let Some(arguments) = &class.arguments {
            for arg in arguments.args.iter() {
                self.visit_expr(arg);
            }
            for keyword in arguments.keywords.iter() {
                self.visit_expr(&keyword.value);
            }
        }
        if let Some(saved) = type_params_scope {
            self.leave_scope(saved);
        }
        self.add_binding(class.name.id.as_str(), class.name.range, BindingKind::ClassDef(class), class.range.end(), class.range.start());
        self.register_noqa(class.range);
        let type_params_scope = self.enter_type_params(class.type_params.as_deref());
        self.def_stack.push(DefFrame { name: class.name.id.as_str(), range: class.range, is_class: true, direct_in_class });
        let saved = self.enter_scope(ScopeKind::Class, class.name.id.as_str(), class.range.start());
        self.visit_block(&class.body);
        self.leave_scope(saved);
        self.def_stack.pop();
        if let Some(saved) = type_params_scope {
            self.leave_scope(saved);
        }
    }

    fn visit_lambda(&mut self, lambda: &'a ExprLambda) {
        if let Some(parameters) = &lambda.parameters {
            self.visit_parameter_defaults(parameters);
        }
        let saved = self.enter_scope(ScopeKind::Lambda, "<lambda>", lambda.range.start());
        if let Some(parameters) = &lambda.parameters {
            self.bind_parameters(parameters, lambda.range.start());
        }
        let body_call = match lambda.body.as_ref() {
            Expr::Call(call) => Some(call.range),
            _ => None,
        };
        let old = std::mem::replace(&mut self.lambda_body_call, body_call);
        self.visit_expr(&lambda.body);
        self.lambda_body_call = old;
        self.leave_scope(saved);
    }

    fn visit_comprehension_expr(&mut self, range: TextRange, generators: &'a [Comprehension], elts: &[&'a Expr], is_generator: bool) {
        // The first iterable is evaluated in the enclosing scope
        if let Some(first) = generators.first() {
            self.visit_expr(&first.iter);
        }
        let saved = self.enter_scope(ScopeKind::Comprehension, "", range.start());
        self.model.scopes[self.scope].is_generator = is_generator;
        for (i, generator) in generators.iter().enumerate() {
            if i > 0 {
                self.visit_expr(&generator.iter);
            }
            self.bind_target(&generator.target, TargetCtx::Comprehension { iter: &generator.iter }, range.start(), range.start());
            for condition in generator.ifs.iter() {
                self.visit_expr(condition);
            }
        }
        for elt in elts {
            self.visit_expr(elt);
        }
        self.leave_scope(saved);
    }

    fn visit_statement(&mut self, stmt: &'a Stmt) {
        let start = stmt.start();
        match stmt {
            Stmt::FunctionDef(func) => self.visit_function_def(func),
            Stmt::ClassDef(class) => self.visit_class_def(class),
            Stmt::Return(ret) => {
                if let Some(value) = &ret.value {
                    self.visit_expr(value);
                }
                self.model.returns.push(ReturnRecord { value: ret.value.as_deref(), scope: self.scope, pos: start, path: self.path.clone() });
            }
            Stmt::Delete(delete) => {
                for target in delete.targets.iter() {
                    self.visit_expr(target);
                }
            }
            Stmt::Assign(assign) => {
                self.visit_expr(&assign.value);
                for target in assign.targets.iter() {
                    self.bind_target(target, TargetCtx::Assign { value: &assign.value, is_assign_stmt: true }, assign.range.end(), start);
                }
            }
            Stmt::AugAssign(aug_assign) => {
                self.visit_expr(&aug_assign.value);
                if let Expr::Name(name) = aug_assign.target.as_ref() {
                    self.add_use(name.id.as_str(), name.range);
                    let kind = BindingKind::AugAssign { target: name, value: &aug_assign.value };
                    self.add_binding(name.id.as_str(), name.range, kind, aug_assign.range.end(), start);
                } else {
                    self.visit_expr(&aug_assign.target);
                }
            }
            Stmt::AnnAssign(ann_assign) => {
                self.visit_annotation(&ann_assign.annotation);
                if let Some(value) = &ann_assign.value {
                    self.visit_expr(value);
                }
                let ctx = match &ann_assign.value {
                    Some(value) => TargetCtx::Assign { value, is_assign_stmt: false },
                    None => TargetCtx::AnnOnly,
                };
                match ann_assign.target.as_ref() {
                    Expr::Name(name) => {
                        let id = self.add_binding(name.id.as_str(), name.range, ctx.simple_kind(), ann_assign.range.end(), start);
                        self.model.bindings[id].simple_ann_assign = ann_assign.simple;
                    }
                    target => self.bind_target(target, ctx, ann_assign.range.end(), start),
                }
            }
            Stmt::TypeAlias(type_alias) => {
                if let Expr::Name(name) = type_alias.name.as_ref() {
                    self.add_binding(name.id.as_str(), name.range, BindingKind::Other, type_alias.range.end(), start);
                }
                let saved = self.enter_type_params(type_alias.type_params.as_deref());
                self.visit_annotation(&type_alias.value);
                if let Some(saved) = saved {
                    self.leave_scope(saved);
                }
            }
            Stmt::For(for_stmt) => {
                self.visit_expr(&for_stmt.iter);
                self.with_branch(start, Branch::LoopBody, |s| {
                    s.bind_target(&for_stmt.target, TargetCtx::For { iter: &for_stmt.iter }, for_stmt.iter.end(), start);
                });
                self.visit_branch(start, Branch::LoopBody, &for_stmt.body, None);
                self.visit_branch(start, Branch::LoopElse, &for_stmt.orelse, None);
            }
            Stmt::While(while_stmt) => {
                self.with_branch(start, Branch::LoopBody, |s| s.visit_expr(&while_stmt.test));
                self.visit_branch(start, Branch::LoopBody, &while_stmt.body, None);
                self.visit_branch(start, Branch::LoopElse, &while_stmt.orelse, None);
            }
            Stmt::If(if_stmt) => {
                self.visit_expr(&if_stmt.test);
                let old_type_checking = self.type_checking;
                self.type_checking |= is_type_checking_test(&if_stmt.test);
                self.visit_branch(start, Branch::IfBody, &if_stmt.body, Some(&if_stmt.test));
                self.type_checking = old_type_checking;
                let mut last_test: &Expr = &if_stmt.test;
                for (i, clause) in if_stmt.elif_else_clauses.iter().enumerate() {
                    if let Some(test) = &clause.test {
                        self.with_branch(start, Branch::IfClause(i), |s| s.visit_expr(test));
                        last_test = test;
                    }
                    self.visit_branch(start, Branch::IfClause(i), &clause.body, Some(last_test));
                }
            }
            Stmt::With(with_stmt) => {
                for item in with_stmt.items.iter() {
                    self.visit_expr(&item.context_expr);
                    if let Some(vars) = &item.optional_vars {
                        self.bind_target(vars, TargetCtx::Other, item.range.end(), start);
                    }
                }
                self.visit_branch(start, Branch::With, &with_stmt.body, None);
            }
            Stmt::Match(match_stmt) => {
                self.visit_expr(&match_stmt.subject);
                for (i, case) in match_stmt.cases.iter().enumerate() {
                    self.with_branch(start, Branch::MatchCase(i), |s| {
                        s.visit_pattern(&case.pattern);
                        if let Some(guard) = &case.guard {
                            s.visit_expr(guard);
                        }
                    });
                    self.visit_branch(start, Branch::MatchCase(i), &case.body, None);
                }
            }
            Stmt::Raise(raise) => {
                if let Some(exc) = &raise.exc {
                    self.visit_expr(exc);
                }
                if let Some(cause) = &raise.cause {
                    self.visit_expr(cause);
                }
                self.model.raises.push(RaiseRecord { range: raise.range, defs: self.def_stack.clone() });
            }
            Stmt::Try(try_stmt) => {
                let guards_name_error = try_stmt.handlers.iter().any(|handler| {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    handler.type_.as_deref().is_some_and(catches_name_error)
                });
                self.name_error_guard += guards_name_error as u32;
                self.visit_branch(start, Branch::TryBody, &try_stmt.body, None);
                self.name_error_guard -= guards_name_error as u32;
                for (i, handler) in try_stmt.handlers.iter().enumerate() {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    self.with_branch(start, Branch::TryHandler(i), |s| {
                        if let Some(type_) = &handler.type_ {
                            s.visit_expr(type_);
                        }
                        if let Some(name) = &handler.name {
                            s.add_binding(name.id.as_str(), name.range, BindingKind::Other, handler.range.start(), handler.range.start());
                        }
                        let old = s.direct_if_test.take();
                        s.visit_block(&handler.body);
                        s.direct_if_test = old;
                    });
                }
                self.visit_branch(start, Branch::TryElse, &try_stmt.orelse, None);
                self.visit_branch(start, Branch::TryFinally, &try_stmt.finalbody, None);
            }
            Stmt::Assert(assert) => {
                self.visit_expr(&assert.test);
                if let Some(msg) = &assert.msg {
                    self.visit_expr(msg);
                }
                let mut names = NameCollector::default();
                names.visit_expr(&assert.test);
                let mut scope = Some(self.scope);
                while let Some(s) = scope {
                    self.model.scopes[s].asserted_names.extend(names.names.iter().copied());
                    scope = self.model.scopes[s].parent;
                }
            }
            Stmt::Import(import) => {
                for alias in import.names.iter() {
                    let (name, qualname) = match &alias.asname {
                        Some(asname) => (asname.id.as_str(), alias.name.id.to_string()),
                        None => {
                            let first = alias.name.id.as_str().split('.').next().unwrap_or_default();
                            (first, first.to_string())
                        }
                    };
                    let id = self.add_binding(name, alias.range, BindingKind::Import { qualname }, import.range.end(), start);
                    self.model.bindings[id].line_offset = start;
                }
            }
            Stmt::ImportFrom(import) => {
                let module = format!("{}{}", ".".repeat(import.level as usize), import.module.as_ref().map_or("", |m| m.id.as_str()));
                for alias in import.names.iter() {
                    if alias.name.id.as_str() == "*" {
                        continue;
                    }
                    let name = alias.asname.as_ref().unwrap_or(&alias.name).id.as_str();
                    let qualname = format!("{}.{}", module, alias.name.id);
                    let id = self.add_binding(name, alias.range, BindingKind::Import { qualname }, import.range.end(), start);
                    self.model.bindings[id].line_offset = start;
                }
            }
            Stmt::Global(global) => {
                let scope = &mut self.model.scopes[self.scope];
                scope.globals.extend(global.names.iter().map(|n| n.id.as_str()));
            }
            Stmt::Nonlocal(nonlocal) => {
                let scope = &mut self.model.scopes[self.scope];
                scope.nonlocals.extend(nonlocal.names.iter().map(|n| n.id.as_str()));
            }
            Stmt::Expr(expr) => self.visit_expr(&expr.value),
            Stmt::Pass(_) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::IpyEscapeCommand(_) => {}
        }
    }
}

impl<'a> Visitor<'a> for Collector<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        self.visit_statement(stmt);
    }

    fn visit_annotation(&mut self, expr: &'a Expr) {
        let old = std::mem::replace(&mut self.in_annotation, true);
        self.visit_expr(expr);
        self.in_annotation = old;
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        let name = match pattern {
            Pattern::MatchAs(p) => p.name.as_ref(),
            Pattern::MatchStar(p) => p.name.as_ref(),
            Pattern::MatchMapping(p) => p.rest.as_ref(),
            _ => None,
        };
        if let Some(name) = name {
            self.add_binding(name.id.as_str(), name.range, BindingKind::Other, pattern.end(), pattern.start());
        }
        walk_pattern(self, pattern);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => match name.ctx {
                ExprContext::Load | ExprContext::Del => self.add_use(name.id.as_str(), name.range),
                ExprContext::Store => {
                    self.add_binding(name.id.as_str(), name.range, BindingKind::Other, name.range.end(), name.range.start());
                }
                ExprContext::Invalid => {}
            },
            Expr::Lambda(lambda) => self.visit_lambda(lambda),
            Expr::ListComp(comp) => self.visit_comprehension_expr(comp.range, &comp.generators, &[&comp.elt], false),
            Expr::SetComp(comp) => self.visit_comprehension_expr(comp.range, &comp.generators, &[&comp.elt], false),
            Expr::DictComp(comp) => self.visit_comprehension_expr(comp.range, &comp.generators, &[&comp.key, &comp.value], false),
            Expr::Generator(comp) => self.visit_comprehension_expr(comp.range, &comp.generators, &[&comp.elt], true),
            Expr::Named(named) => {
                self.visit_expr(&named.value);
                match named.target.as_ref() {
                    Expr::Name(name) => {
                        // Named expressions bind in the nearest scope that is not a comprehension. In a
                        // comprehension, they can be used anywhere in it, as in its element.
                        let current = self.scope;
                        let mut pos = self.if_exp_test_start.unwrap_or(named.range.end());
                        while self.model.scopes[self.scope].kind == ScopeKind::Comprehension {
                            pos = self.model.scopes[self.scope].start;
                            self.scope = self.model.scopes[self.scope].parent.unwrap_or(MODULE_SCOPE);
                        }
                        let kind = BindingKind::Assign { value: &named.value, is_assign_stmt: false };
                        self.add_binding(name.id.as_str(), name.range, kind, pos, named.range.start());
                        self.scope = current;
                    }
                    target => self.visit_expr(target),
                }
            }
            Expr::If(if_exp) => {
                let old = self.if_exp_test_start;
                self.if_exp_test_start = Some(old.unwrap_or(if_exp.range.start()).min(if_exp.range.start()));
                self.visit_expr(&if_exp.test);
                self.if_exp_test_start = old;
                self.visit_expr(&if_exp.body);
                self.visit_expr(&if_exp.orelse);
            }
            Expr::Call(call) => {
                self.model.calls.push(CallRecord { call, scope: self.scope, path: self.path.clone() });
                if self.lambda_body_call != Some(call.range) && let Some(calls) = self.stmt_calls.last_mut() {
                    calls.push((call, self.scope));
                }
                walk_expr(self, expr);
            }
            _ => walk_expr(self, expr),
        }
    }
}

/// Collects all the names used in an expression
#[derive(Default)]
struct NameCollector<'a> {
    names: Vec<&'a str>,
}

impl<'a> Visitor<'a> for NameCollector<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Name(name) = expr {
            self.names.push(name.id.as_str());
        }
        walk_expr(self, expr);
    }
}

fn is_type_checking_test(test: &Expr) -> bool {
    match test {
        Expr::Name(name) => name.id.as_str() == "TYPE_CHECKING",
        Expr::Attribute(attr) => attr.attr.id.as_str() == "TYPE_CHECKING",
        _ => false,
    }
}

fn catches_name_error(type_: &Expr) -> bool {
    match type_ {
        Expr::Name(name) => name.id.as_str() == "NameError",
        Expr::Attribute(attr) => attr.attr.id.as_str() == "NameError",
        Expr::Tuple(tuple) => tuple.elts.iter().any(catches_name_error),
        _ => false,
    }
}

/// Last identifier of a Name or an Attribute
fn expr_last_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(name) => Some(name.id.as_str()),
        Expr::Attribute(attr) => Some(attr.attr.id.as_str()),
        _ => None,
    }
}

/// Interpolated expressions of an f-string. With `nested`, the ones of format specs are included.
fn fstring_interpolations<'a>(fstring: &'a ExprFString, nested: bool) -> Vec<&'a Expr> {
    fn collect<'a>(elements: &'a [InterpolatedStringElement], nested: bool, result: &mut Vec<&'a Expr>) {
        for element in elements {
            if let InterpolatedStringElement::Interpolation(interpolation) = element {
                result.push(&interpolation.expression);
                if nested && let Some(spec) = &interpolation.format_spec {
                    collect(&spec.elements, nested, result);
                }
            }
        }
    }
    let mut result = vec![];
    for part in fstring.value.iter() {
        if let FStringPart::FString(f) = part {
            collect(&f.elements, nested, &mut result);
        }
    }
    result
}

/*
 * Checks
 */

const BUILTINS: &[&str] = &[
    "ArithmeticError", "AssertionError", "AttributeError", "BaseException", "BaseExceptionGroup", "BlockingIOError",
    "BrokenPipeError", "BufferError", "BytesWarning", "ChildProcessError", "ConnectionAbortedError", "ConnectionError",
    "ConnectionRefusedError", "ConnectionResetError", "DeprecationWarning", "EOFError", "Ellipsis", "EncodingWarning",
    "EnvironmentError", "Exception", "ExceptionGroup", "False", "FileExistsError", "FileNotFoundError",
    "FloatingPointError", "FutureWarning", "GeneratorExit", "IOError", "ImportError", "ImportWarning",
    "IndentationError", "IndexError", "InterruptedError", "IsADirectoryError", "KeyError", "KeyboardInterrupt",
    "LookupError", "MemoryError", "ModuleNotFoundError", "NameError", "None", "NotADirectoryError", "NotImplemented",
    "NotImplementedError", "OSError", "OverflowError", "PendingDeprecationWarning", "PermissionError",
    "ProcessLookupError", "PythonFinalizationError", "RecursionError", "ReferenceError", "ResourceWarning",
    "RuntimeError", "RuntimeWarning", "StopAsyncIteration", "StopIteration", "SyntaxError", "SyntaxWarning",
    "SystemError", "SystemExit", "TabError", "TimeoutError", "True", "TypeError", "UnboundLocalError",
    "UnicodeDecodeError", "UnicodeEncodeError", "UnicodeError", "UnicodeTranslateError", "UnicodeWarning",
    "UserWarning", "ValueError", "Warning", "WindowsError", "ZeroDivisionError", "__build_class__", "__debug__",
    "__doc__", "__import__", "__loader__", "__name__", "__package__", "__spec__", "abs", "aiter", "all", "anext",
    "any", "ascii", "bin", "bool", "breakpoint", "bytearray", "bytes", "callable", "chr", "classmethod", "compile",
    "complex", "copyright", "credits", "delattr", "dict", "dir", "divmod", "enumerate", "eval", "exec", "exit",
    "filter", "float", "format", "frozenset", "getattr", "globals", "hasattr", "hash", "help", "hex", "id", "input",
    "int", "isinstance", "issubclass", "iter", "len", "license", "list", "locals", "map", "max", "memoryview", "min",
    "next", "object", "oct", "open", "ord", "pow", "print", "property", "quit", "range", "repr", "reversed", "round",
    "set", "setattr", "slice", "sorted", "staticmethod", "str", "sum", "super", "tuple", "type", "vars", "zip",
];

const MODULE_DUNDERS: &[&str] = &["__file__", "__builtins__", "__path__", "__cached__", "__annotations__", "__dict__"];

const TERMINATING_FUNCTIONS: &[&str] = &["sys.exit", "os._exit", "posix._exit", "nt._exit"];

const ERRORS_REQUIRING_GETTEXT: &[&str] = &["UserError", "ValidationError", "AccessError", "AccessDenied", "MissingError"];

enum NameStatus {
    Defined,
    UsedBeforeAssignment,
    Undefined,
}

struct Finding {
    code: DiagnosticCode,
    range: TextRange,
    params: Vec<String>,
    unnecessary: bool,
}

struct Checker<'s, 'm, 'a> {
    session: &'s mut SessionInfo<'a>,
    file_symbol: SymbolKey,
    model: &'m SemanticModel<'m>,
    findings: Vec<Finding>,
}

impl Checker<'_, '_, '_> {
    fn add(&mut self, code: DiagnosticCode, range: TextRange, params: Vec<String>) {
        self.findings.push(Finding { code, range, params, unnecessary: false });
    }

    /// Names defined through the symbol table, e.g. by a wildcard import
    fn is_defined_in_file_symbols(&self, name: &str) -> bool {
        !self.session.st().get_symbol(self.file_symbol, (&[], &[name]), u32::MAX).is_empty()
    }

    /// used-before-assignment (E0601) and undefined-variable (E0602)
    fn check_names(&mut self) {
        let model = self.model;
        for name_use in model.uses.iter() {
            match self.name_status(name_use) {
                NameStatus::Defined => {}
                NameStatus::UsedBeforeAssignment => {
                    self.add(DiagnosticCode::pylint_E0601, name_use.range, vec![name_use.name.to_string()]);
                }
                NameStatus::Undefined => {
                    if !name_use.name_error_guarded && !self.is_defined_in_file_symbols(name_use.name) {
                        self.add(DiagnosticCode::pylint_E0602, name_use.range, vec![name_use.name.to_string()]);
                    }
                }
            }
        }
    }

    fn name_status(&self, name_use: &Use) -> NameStatus {
        let model = self.model;
        let name = name_use.name;
        let use_pos = name_use.range.start();
        let mut current = name_use.scope;
        // Order of definitions only matters for the code executed right away
        let mut ordered = !name_use.in_annotation;
        let mut bound_later: Option<&Binding> = None;
        loop {
            let scope = &model.scopes[current];
            if current == name_use.scope && scope.kind == ScopeKind::Class && matches!(name, "__module__" | "__qualname__") {
                return NameStatus::Defined;
            }
            if scope.kind != ScopeKind::Module && scope.globals.contains(name) {
                current = MODULE_SCOPE;
                ordered = false;
                continue;
            }
            if !scope.nonlocals.contains(name) && let Some(ids) = scope.bindings.get(name) {
                let bindings: Vec<&Binding> = ids.iter().map(|&id| model.binding(id)).collect();
                if !name_use.in_annotation && !name_use.type_checking && bindings.iter().all(|b| b.type_checking) {
                    return NameStatus::UsedBeforeAssignment;
                }
                if !ordered {
                    return NameStatus::Defined;
                }
                let before: Vec<&Binding> = bindings.iter().copied().filter(|b| b.pos <= use_pos).collect();
                if !before.is_empty() {
                    return NameStatus::Defined;
                }
                if current == name_use.scope && Self::shares_loop(name_use, &bindings) {
                    return NameStatus::Defined;
                }
                match scope.kind {
                    ScopeKind::Function | ScopeKind::Lambda => return NameStatus::UsedBeforeAssignment,
                    ScopeKind::Module | ScopeKind::Class => {
                        // These scopes fall back to the globals and builtins
                        bound_later.get_or_insert(bindings[0]);
                    }
                    ScopeKind::Comprehension | ScopeKind::TypeParams => return NameStatus::Defined,
                }
            }
            if scope.kind == ScopeKind::Module {
                break;
            }
            if matches!(scope.kind, ScopeKind::Function | ScopeKind::Lambda) {
                ordered = false;
            }
            match model.parent_scope_skipping_classes(current) {
                Some(parent) => current = parent,
                None => break,
            }
        }
        if name == "__class__" {
            // Implicit closure of the methods
            let mut scope = model.scopes[name_use.scope].parent;
            while let Some(s) = scope {
                if model.scopes[s].kind == ScopeKind::Class {
                    return NameStatus::Defined;
                }
                scope = model.scopes[s].parent;
            }
        }
        if BUILTINS.contains(&name) || MODULE_DUNDERS.contains(&name) {
            return NameStatus::Defined;
        }
        match bound_later {
            // A class can not refer to itself in its body
            Some(Binding { kind: BindingKind::ClassDef(class), .. }) if class.range.contains(use_pos) => NameStatus::Undefined,
            Some(_) => NameStatus::UsedBeforeAssignment,
            None => NameStatus::Undefined,
        }
    }

    /// The use and a binding are in the same loop: the binding may come from a previous iteration
    fn shares_loop(name_use: &Use, bindings: &[&Binding]) -> bool {
        name_use.path.iter()
            .filter(|elem| elem.branch == Branch::LoopBody)
            .any(|elem| bindings.iter().any(|b| b.path.contains(elem)))
    }

    /// eval-used (W0123)
    fn check_eval_used(&mut self) {
        let model = self.model;
        for record in model.calls.iter() {
            let Expr::Name(func) = record.call.func.as_ref() else { continue };
            if func.id.as_str() != "eval" {
                continue;
            }
            let frame = model.frame(record.scope);
            if model.scopes[frame].bindings.contains_key("eval")
                || model.scopes[MODULE_SCOPE].bindings.contains_key("eval")
                || self.is_defined_in_file_symbols("eval")
            {
                continue;
            }
            self.add(DiagnosticCode::pylint_W0123, record.call.range, vec![]);
        }
    }

    /// unreachable (W0101)
    fn check_unreachable(&mut self) {
        let model = self.model;
        let mut reported = HashSet::default();
        for candidate in model.unreachable_candidates.iter() {
            let terminates = match &candidate.terminator {
                Terminator::Statement => true,
                Terminator::Calls(calls) => calls.iter().any(|(call, scope)| self.is_terminating_call(call, *scope)),
            };
            if terminates && reported.insert(candidate.next.range()) {
                self.findings.push(Finding { code: DiagnosticCode::pylint_W0101, range: candidate.next.range(), params: vec![], unnecessary: true });
            }
        }
    }

    /// Call to `exit()`, `quit()`, `sys.exit()`, `os._exit()` or to a function returning `NoReturn`/`Never`
    fn is_terminating_call(&self, call: &ExprCall, scope: ScopeId) -> bool {
        let model = self.model;
        let pos = call.range.start();
        match call.func.as_ref() {
            Expr::Name(func) => match model.resolve_binding(func.id.as_str(), scope, pos) {
                None => matches!(func.id.as_str(), "exit" | "quit"),
                Some(binding) => match &binding.kind {
                    BindingKind::Import { qualname } => TERMINATING_FUNCTIONS.contains(&qualname.as_str()),
                    BindingKind::FunctionDef(def) => !def.is_async && self.returns_never(def, binding.scope),
                    _ => false,
                },
            },
            Expr::Attribute(attr) => {
                let Expr::Name(module) = attr.value.as_ref() else { return false };
                match model.resolve_binding(module.id.as_str(), scope, pos) {
                    Some(Binding { kind: BindingKind::Import { qualname }, .. }) => {
                        TERMINATING_FUNCTIONS.contains(&format!("{}.{}", qualname, attr.attr.id).as_str())
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn returns_never(&self, def: &StmtFunctionDef, scope: ScopeId) -> bool {
        let Some(Expr::Name(returns)) = def.returns.as_deref() else { return false };
        match self.model.resolve_binding(returns.id.as_str(), scope, def.range.start()) {
            Some(Binding { kind: BindingKind::Import { qualname }, .. }) => matches!(
                qualname.as_str(),
                "typing.NoReturn" | "typing.Never" | "typing_extensions.NoReturn" | "typing_extensions.Never"
            ),
            _ => false,
        }
    }

    /// function-redefined (E0102)
    fn check_function_redefined(&mut self, line_of: &dyn Fn(TextSize) -> usize) {
        let model = self.model;
        for scope in model.scopes.iter().filter(|s| s.is_frame()) {
            for (name, ids) in scope.bindings.iter() {
                let redefinitions: Vec<&Binding> = ids.iter().map(|&id| model.binding(id)).filter(|b| !b.simple_ann_assign).collect();
                if redefinitions.len() < 2 {
                    continue;
                }
                let defined_self = redefinitions.iter().copied().find(|b| !is_overload_stub(b));
                for &binding in redefinitions.iter() {
                    let redefinition_type = match &binding.kind {
                        BindingKind::FunctionDef(def) => {
                            if is_redefined_by_decorator(def) || self.is_registered_in_singledispatch(def, binding.scope) {
                                continue;
                            }
                            if scope.kind == ScopeKind::Class { "method" } else { "function" }
                        }
                        BindingKind::ClassDef(_) => "class",
                        _ => continue,
                    };
                    let defined_self = defined_self.unwrap_or(binding);
                    if std::ptr::eq(defined_self, binding)
                        || are_exclusive(&binding.path, &defined_self.path)
                        || (scope.kind == ScopeKind::Class && *name == "__module__")
                        || is_overload_stub(binding)
                        || binding.parent_if_test.is_some_and(|test| is_redefinition_guard(test, name))
                        || is_dummy_name(name)
                    {
                        continue;
                    }
                    let line = line_of(defined_self.line_offset).to_string();
                    self.add(DiagnosticCode::pylint_E0102, binding.range, vec![redefinition_type.to_string(), line]);
                }
            }
        }
    }

    /// `@f.register` where `f` is decorated with `functools.singledispatch`
    fn is_registered_in_singledispatch(&self, def: &StmtFunctionDef, scope: ScopeId) -> bool {
        def.decorator_list.iter().any(|decorator| {
            let func = match &decorator.expression {
                Expr::Call(call) => call.func.as_ref(),
                expr => expr,
            };
            let Expr::Attribute(attr) = func else { return false };
            let Expr::Name(registry) = attr.value.as_ref() else { return false };
            if attr.attr.id.as_str() != "register" {
                return false;
            }
            match self.model.resolve_binding(registry.id.as_str(), scope, def.range.start()) {
                Some(Binding { kind: BindingKind::FunctionDef(registry_def), .. }) => registry_def.decorator_list.iter()
                    .any(|d| expr_last_name(&d.expression) == Some("singledispatch")),
                _ => false,
            }
        })
    }

    /// raise-unlink-override (E8503)
    fn check_raise_unlink_override(&mut self) {
        let model = self.model;
        let mut base_model: Option<Option<ClassKey>> = None;
        let mut is_model_cache: HashMap<TextSize, bool> = HashMap::default();
        for raise in model.raises.iter() {
            for (i, frame) in raise.defs.iter().enumerate().rev() {
                if frame.is_class || frame.name != "unlink" || !frame.direct_in_class {
                    continue;
                }
                let class_frame = &raise.defs[i - 1];
                let base = *base_model.get_or_insert_with(|| self.base_model_class());
                let inherits = *is_model_cache.entry(class_frame.range.start())
                    .or_insert_with(|| base.is_some_and(|base| self.inherits_base_model(&raise.defs[..i], base)));
                if inherits {
                    self.add(DiagnosticCode::pylint_E8503, raise.range, vec![]);
                    break;
                }
            }
        }
    }

    fn base_model_class(&self) -> Option<ClassKey> {
        let tree: TreeStrSlice = if self.session.sync_odoo.version >= (18, 1) {
            (&["odoo", "orm", "models"], &["BaseModel"])
        } else {
            (&["odoo", "models"], &["BaseModel"])
        };
        let odoo_path = self.session.sync_odoo.config.odoo_path()?;
        match self.session.sync_odoo.get_symbol(&odoo_path, tree, u32::MAX).first() {
            Some(&SymbolKey::Class(base)) => Some(base),
            _ => None,
        }
    }

    /// `defs` is the chain of definitions leading to the class, from the module level
    fn inherits_base_model(&mut self, defs: &[DefFrame], base: ClassKey) -> bool {
        let mut symbol = self.file_symbol;
        for def in defs {
            match self.session.st().get_positioned_symbol(symbol, def.name, &def.range) {
                Some(s) => symbol = s,
                None => return false,
            }
        }
        match symbol {
            SymbolKey::Class(class) => ClassSymbol::inherits(self.session, class, base, &mut None),
            _ => false,
        }
    }

    /// gettext-variable (E8502), gettext-placeholders (E8505), gettext-repr (E8506) and missing-gettext (E8507)
    fn check_gettext(&mut self) {
        let model = self.model;
        for record in model.calls.iter() {
            let call = record.call;
            let Some(func_name) = expr_last_name(&call.func) else { continue };
            let first_arg = call.arguments.args.first();
            if ERRORS_REQUIRING_GETTEXT.contains(&func_name) && let Some(first_arg) = first_arg {
                if !is_gettext_whitelisted_argument(first_arg) {
                    self.add(DiagnosticCode::pylint_E8507, call.range, vec![func_name.to_string()]);
                }
                continue;
            }
            if !matches!(func_name, "_" | "_lt") {
                continue;
            }
            let Some(Expr::StringLiteral(string)) = first_arg else {
                self.add(DiagnosticCode::pylint_E8502, call.range, vec![]);
                continue;
            };
            let value = string.value.to_str();
            if count_printf_placeholders(value) >= 2 {
                self.add(DiagnosticCode::pylint_E8505, call.range, vec![]);
            }
            if has_repr_placeholder(value) {
                self.add(DiagnosticCode::pylint_E8506, call.range, vec![]);
            }
        }
    }

    fn into_diagnostics(mut self) -> Vec<Diagnostic> {
        self.findings.sort_by_key(|f| (f.range.start(), f.range.end(), f.code.to_string()));
        let file_noqa = self.session.current_noqa.clone();
        let mut diagnostics = vec![];
        for finding in self.findings.iter() {
            let mut noqas = vec![file_noqa.clone()];
            noqas.extend(self.model.noqa_ranges.iter()
                .filter(|(range, _)| range.contains_inclusive(finding.range.start()))
                .map(|(_, noqa)| noqa.clone()));
            self.session.current_noqa = combine_noqa_info(&noqas);
            let params: Vec<&str> = finding.params.iter().map(String::as_str).collect();
            if let Some(diagnostic) = create_diagnostic(self.session, finding.code, &params) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(finding.range.start().to_u32(), 0), Position::new(finding.range.end().to_u32(), 0)),
                    tags: finding.unnecessary.then(|| vec![DiagnosticTag::UNNECESSARY]),
                    ..diagnostic
                });
            }
        }
        self.session.current_noqa = file_noqa;
        diagnostics
    }
}

fn is_overload_stub(binding: &Binding) -> bool {
    match &binding.kind {
        BindingKind::FunctionDef(def) => def.decorator_list.iter().any(|d| expr_last_name(&d.expression) == Some("overload")),
        _ => false,
    }
}

/// Property setters and deleters: `@name.setter`
fn is_redefined_by_decorator(def: &StmtFunctionDef) -> bool {
    def.decorator_list.iter().any(|d| matches!(
        &d.expression,
        Expr::Attribute(attr) if matches!(attr.value.as_ref(), Expr::Name(n) if n.id == def.name.id)
    ))
}

/// `if not name:` and `if name is None:`
fn is_redefinition_guard(test: &Expr, name: &str) -> bool {
    match test {
        Expr::UnaryOp(unary) => unary.op == UnaryOp::Not && matches!(unary.operand.as_ref(), Expr::Name(n) if n.id.as_str() == name),
        Expr::Compare(compare) => {
            matches!(compare.left.as_ref(), Expr::Name(n) if n.id.as_str() == name)
                && compare.ops.first() == Some(&CmpOp::Is)
                && compare.comparators.first().is_some_and(|c| c.is_none_literal_expr())
        }
        _ => false,
    }
}

/// PyLint's default `dummy-variables-rgx`: `_+$|(_[a-zA-Z0-9_]*[a-zA-Z0-9]+?$)|dummy|^ignored_|^unused_`
fn is_dummy_name(name: &str) -> bool {
    if !name.is_empty() && name.chars().all(|c| c == '_') {
        return true;
    }
    if name.len() >= 2 && name.starts_with('_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.chars().last().is_some_and(|c| c.is_ascii_alphanumeric())
    {
        return true;
    }
    name.starts_with("dummy") || name.starts_with("ignored_") || name.starts_with("unused_")
}

fn is_gettext_whitelisted_argument(arg: &Expr) -> bool {
    match arg {
        Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) | Expr::Call(_) => true,
        Expr::If(if_exp) => is_gettext_whitelisted_argument(&if_exp.body) && is_gettext_whitelisted_argument(&if_exp.orelse),
        Expr::BoolOp(bool_op) => bool_op.values.iter().all(is_gettext_whitelisted_argument),
        Expr::BinOp(bin_op) => is_gettext_whitelisted_argument(&bin_op.right) || is_gettext_whitelisted_argument(&bin_op.left),
        _ => false,
    }
}

/// Number of matches of the printf-style placeholder regex used by Odoo's gettext checker:
/// `(?<!%)%[#0\- +]*(?:\d+|\*)?(?:\.(?:\d+|\*))?[hlL]?[bcdeEfFgGnorsxX]`
fn count_printf_placeholders(value: &str) -> usize {
    fn match_conversion(b: &[u8], mut j: usize) -> Option<usize> {
        while j < b.len() && b"#0- +".contains(&b[j]) {
            j += 1;
        }
        if j < b.len() && b[j] == b'*' {
            j += 1;
        } else {
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
        }
        if j + 1 < b.len() && b[j] == b'.' {
            if b[j + 1] == b'*' {
                j += 2;
            } else if b[j + 1].is_ascii_digit() {
                j += 1;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
            }
        }
        if j < b.len() && b"hlL".contains(&b[j]) {
            j += 1;
        }
        (j < b.len() && b"bcdeEfFgGnorsxX".contains(&b[j])).then_some(j + 1)
    }
    let bytes = value.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && (i == 0 || bytes[i - 1] != b'%') && let Some(end) = match_conversion(bytes, i + 1) {
            count += 1;
            i = end;
            continue;
        }
        i += 1;
    }
    count
}

/// Search of the regex `%(?:\(\w+\))?r` used by Odoo's gettext checker
fn has_repr_placeholder(value: &str) -> bool {
    let chars: Vec<char> = value.chars().collect();
    (0..chars.len()).filter(|&i| chars[i] == '%').any(|i| {
        match chars.get(i + 1) {
            Some('r') => true,
            Some('(') => {
                let mut k = i + 2;
                while k < chars.len() && (chars[k].is_alphanumeric() || chars[k] == '_') {
                    k += 1;
                }
                k > i + 2 && chars.get(k) == Some(&')') && chars.get(k + 1) == Some(&'r')
            }
            _ => false,
        }
    })
}

/*
 * sql-injection (E8501), port of Odoo's `_odoo_checker_sql_injection`
 */

const CURSOR_EXPRESSIONS: &[&str] = &["self.env.cr", "self.cr", "cr", "odoo.tools", "tools"];
const ATTRIBUTE_WHITELIST: &[&str] = &["_table", "name", "lang", "id", "get_lang.code"];
const FUNCTION_WHITELIST: &[&str] = &[
    "create", "read", "write", "browse", "select", "get", "strip", "items", "_select", "_from", "_where",
    "any", "join", "split", "tuple", "get_sql", "search", "list", "set", "next", "SQL",
];
/// Guard against the infinite recursions PyLint would crash on
const MAX_SQL_EVALUATION_DEPTH: u32 = 64;

/// Evaluation context of an expression: where it is, to look up the names it uses
#[derive(Clone, Copy)]
struct Ctx<'m> {
    scope: ScopeId,
    pos: TextSize,
    path: &'m [PathElem],
}

#[derive(Clone)]
struct Callsite {
    position: Option<usize>,
    const_args: bool,
    root_call: TextSize,
}

struct SqlInjectionChecker<'m, 'a, 'f> {
    model: &'m SemanticModel<'a>,
    file_path: &'f str,
    line_of: &'f dyn Fn(TextSize) -> usize,
    /// Functions already visited, by name
    known_functions: HashMap<&'a str, Vec<usize>>,
    /// Calls of functions used to build queries, by function name
    callsites: HashMap<&'a str, Vec<Callsite>>,
    const_def_cache: HashMap<(usize, Option<usize>, bool), bool>,
    in_progress_defs: HashSet<(usize, Option<usize>, bool)>,
    in_progress_bindings: HashSet<usize>,
    root_call: Option<TextSize>,
    depth: u32,
    findings: Vec<Finding>,
}

enum SqlEvent {
    FunctionDef(usize),
    Call(usize),
}

impl<'m, 'a: 'm, 'f> SqlInjectionChecker<'m, 'a, 'f> {
    fn new(model: &'m SemanticModel<'a>, file_path: &'f str, line_of: &'f dyn Fn(TextSize) -> usize) -> Self {
        SqlInjectionChecker {
            model,
            file_path,
            line_of,
            known_functions: HashMap::default(),
            callsites: HashMap::default(),
            const_def_cache: HashMap::default(),
            in_progress_defs: HashSet::default(),
            in_progress_bindings: HashSet::default(),
            root_call: None,
            depth: 0,
            findings: vec![],
        }
    }

    /// Visit the functions and the query executions in source order, as PyLint does
    fn run(&mut self) {
        let model = self.model;
        let mut events: Vec<(TextSize, SqlEvent)> = model.functions.iter().enumerate()
            .map(|(i, f)| (f.def.range.start(), SqlEvent::FunctionDef(i)))
            .chain(model.calls.iter().enumerate()
                .filter(|(_, record)| Self::is_query_call(record.call))
                .map(|(i, record)| (record.call.range.start(), SqlEvent::Call(i))))
            .collect();
        events.sort_by_key(|(offset, _)| *offset);
        for (_, event) in events {
            match event {
                SqlEvent::FunctionDef(i) => self.visit_function_def(i),
                SqlEvent::Call(i) => {
                    let record = &model.calls[i];
                    let ctx = Ctx { scope: record.scope, pos: record.call.range.start(), path: &record.path };
                    let first_arg = &record.call.arguments.args[0];
                    if self.check_concatenation(first_arg, ctx).unwrap_or(true) {
                        self.add(record.call.range, String::new());
                    }
                }
            }
        }
    }

    fn add(&mut self, range: TextRange, detail: String) {
        self.findings.push(Finding { code: DiagnosticCode::pylint_E8501, range, params: vec![detail], unnecessary: false });
    }

    /// Store the function and check the calls made before its definition
    fn visit_function_def(&mut self, function: usize) {
        let def = self.model.functions[function].def;
        let name = def.name.id.as_str();
        self.known_functions.entry(name).or_default().push(function);
        let mut i = 0;
        while let Some(callsite) = self.callsites.get(name).and_then(|c| c.get(i)).cloned() {
            if !self.is_const_def(function, callsite.position, callsite.const_args) {
                let detail = format!("because it is used to build a query in file {}:{}", self.file_path, (self.line_of)(callsite.root_call));
                self.add(def.name.range, detail);
            }
            i += 1;
        }
    }

    /// `cr.execute(...)`, `cr.executemany(...)` or `SQL(...)`
    fn is_query_call(call: &ExprCall) -> bool {
        if call.arguments.args.is_empty() {
            return false;
        }
        match call.func.as_ref() {
            Expr::Attribute(attr) => {
                matches!(attr.attr.id.as_str(), "execute" | "executemany" | "SQL")
                    && CURSOR_EXPRESSIONS.contains(&Self::cursor_name(&attr.value).as_str())
            }
            Expr::Name(name) => name.id.as_str() == "SQL",
            _ => false,
        }
    }

    fn cursor_name(mut expr: &Expr) -> String {
        let mut parts = vec![];
        while let Expr::Attribute(attr) = expr {
            parts.insert(0, attr.attr.id.as_str());
            expr = &attr.value;
        }
        if let Expr::Name(name) = expr {
            parts.insert(0, name.id.as_str());
        }
        parts.join(".")
    }

    fn attribute_chain(expr: &Expr) -> String {
        match expr {
            Expr::Attribute(attr) => format!("{}.{}", Self::attribute_chain(&attr.value), attr.attr.id),
            Expr::Name(name) => name.id.to_string(),
            Expr::Call(call) => Self::attribute_chain(&call.func),
            _ => String::new(),
        }
    }

    /// Bindings of `name` reaching the position of `ctx`, as astroid's `lookup` would return them
    fn lookup(&self, name: &str, ctx: Ctx) -> Vec<usize> {
        let model = self.model;
        let Some(found) = model.resolve_scope(name, ctx.scope) else { return vec![] };
        let ids = &model.scopes[found].bindings[name];
        let binding = |id: usize| model.binding(id);
        if found == ctx.scope {
            let mut candidates: Vec<usize> = ids.iter().copied().filter(|&id| binding(id).pos <= ctx.pos).collect();
            if candidates.is_empty() {
                candidates = ids.clone();
            }
            // An unconditional binding before the use hides the previous ones
            candidates.iter().copied().filter(|&b1| !candidates.iter().any(|&b2| {
                binding(b2).pos > binding(b1).pos && binding(b2).pos <= ctx.pos && ctx.path.starts_with(&binding(b2).path)
            })).collect()
        } else {
            ids.iter().copied().filter(|&b1| !ids.iter().any(|&b2| {
                binding(b2).pos > binding(b1).pos && binding(b2).path.is_empty()
            })).collect()
        }
    }

    fn all_const(&mut self, exprs: &'a [Expr], ctx: Ctx<'m>, args_allowed: bool, position: Option<usize>) -> bool {
        exprs.iter().all(|e| self.is_constexpr(e, ctx, args_allowed, position))
    }

    fn is_constexpr(&mut self, expr: &'a Expr, ctx: Ctx<'m>, args_allowed: bool, position: Option<usize>) -> bool {
        if self.depth >= MAX_SQL_EVALUATION_DEPTH {
            return true;
        }
        self.depth += 1;
        let result = self.is_constexpr_inner(expr, ctx, args_allowed, position);
        self.depth -= 1;
        result
    }

    fn is_constexpr_inner(&mut self, expr: &'a Expr, ctx: Ctx<'m>, args_allowed: bool, position: Option<usize>) -> bool {
        match expr {
            Expr::StringLiteral(_) | Expr::BytesLiteral(_) | Expr::NumberLiteral(_) | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_) | Expr::EllipsisLiteral(_) => true,
            Expr::List(list) => self.all_const(&list.elts, ctx, args_allowed, None),
            Expr::Set(set) => self.all_const(&set.elts, ctx, args_allowed, None),
            Expr::Tuple(tuple) => match position {
                None => self.all_const(&tuple.elts, ctx, args_allowed, None),
                Some(position) => tuple.elts.get(position).is_some_and(|e| self.is_constexpr(e, ctx, args_allowed, None)),
            },
            Expr::Dict(dict) => dict.items.iter().all(|item| match &item.key {
                Some(key) => self.is_constexpr(key, ctx, args_allowed, None) && self.is_constexpr(&item.value, ctx, args_allowed, None),
                None => false,
            }),
            Expr::Starred(starred) => self.is_constexpr(&starred.value, ctx, args_allowed, position),
            Expr::BinOp(bin_op) => {
                let left = self.is_constexpr(&bin_op.left, ctx, args_allowed, None);
                // A string formatted with %d only is always safe
                if bin_op.op == Operator::Mod && let Expr::StringLiteral(string) = bin_op.left.as_ref() {
                    let value = string.value.to_str();
                    if value.contains("%d") && !value.contains("%s") {
                        return true;
                    }
                }
                let right = self.is_constexpr(&bin_op.right, ctx, args_allowed, None);
                left && right
            }
            Expr::Name(name) => self.is_name_const(name, ctx, args_allowed),
            Expr::FString(fstring) => self.is_fstring_const(fstring, ctx, args_allowed),
            Expr::Call(call) => {
                if let Expr::Attribute(attr) = call.func.as_ref() {
                    match attr.attr.id.as_str() {
                        "append" => {
                            return call.arguments.args.first().is_some_and(|arg| self.is_constexpr(arg, ctx, false, None));
                        }
                        "format" => {
                            return self.is_constexpr(&attr.value, ctx, args_allowed, None)
                                && self.all_const(&call.arguments.args, ctx, args_allowed, None)
                                && call.arguments.keywords.iter().all(|k| self.is_constexpr(&k.value, ctx, args_allowed, None));
                        }
                        _ => {}
                    }
                }
                let is_root = self.root_call.is_none();
                if is_root {
                    self.root_call = Some(call.range.start());
                }
                let result = self.evaluate_function_call(call, ctx, args_allowed, position);
                if is_root {
                    self.root_call = None;
                }
                result
            }
            Expr::If(if_exp) => {
                let body = self.is_constexpr(&if_exp.body, ctx, args_allowed, None);
                let orelse = self.is_constexpr(&if_exp.orelse, ctx, args_allowed, None);
                body && orelse
            }
            Expr::Subscript(subscript) => self.is_constexpr(&subscript.value, ctx, args_allowed, None),
            Expr::BoolOp(bool_op) => self.all_const(&bool_op.values, ctx, args_allowed, None),
            Expr::Attribute(_) => {
                let mut chain = Self::attribute_chain(expr);
                while !chain.is_empty() {
                    if ATTRIBUTE_WHITELIST.contains(&chain.as_str()) || chain.starts_with('_') {
                        return true;
                    }
                    match chain.split_once('.') {
                        Some((_, rest)) => chain = rest.to_string(),
                        None => break,
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn is_name_const(&mut self, name: &'a ExprName, ctx: Ctx<'m>, args_allowed: bool) -> bool {
        let model = self.model;
        let mut results = vec![];
        for id in self.lookup(name.id.as_str(), ctx) {
            let binding = model.binding(id);
            if !self.in_progress_bindings.insert(id) {
                results.push(true);
                continue;
            }
            let direct_scope_kind = binding.path.is_empty().then(|| model.scopes[binding.scope].kind);
            let result = match &binding.kind {
                BindingKind::Param => args_allowed,
                BindingKind::Unpack { value, index } => self.is_constexpr(value, binding.ctx(), args_allowed, Some(*index)),
                BindingKind::For { iter } | BindingKind::Comprehension { iter } => self.is_constexpr(iter, binding.ctx(), args_allowed, None),
                BindingKind::AugAssign { target, value } => {
                    let left = self.is_name_const(target, binding.ctx(), args_allowed);
                    let right = self.is_constexpr(value, binding.ctx(), args_allowed, None);
                    left && right
                }
                BindingKind::Import { .. } | BindingKind::FunctionDef(_) | BindingKind::ClassDef(_) => match direct_scope_kind {
                    Some(ScopeKind::Module) => {
                        self.in_progress_bindings.remove(&id);
                        return true;
                    }
                    Some(ScopeKind::Function) => args_allowed,
                    _ => false,
                },
                BindingKind::Assign { value, .. } => self.is_constexpr(value, binding.ctx(), args_allowed, None),
                BindingKind::AnnOnly | BindingKind::Other => false,
            };
            self.in_progress_bindings.remove(&id);
            results.push(result);
        }
        if !results.is_empty() && results.iter().all(|r| *r) {
            return true;
        }
        // If there is an assert on the value, it is very likely to be safe
        model.scopes[ctx.scope].asserted_names.contains(name.id.as_str())
    }

    /// An f-string is constant if all its values are constant or private attributes
    fn is_fstring_const(&mut self, fstring: &'a ExprFString, ctx: Ctx<'m>, args_allowed: bool) -> bool {
        fstring_interpolations(fstring, false).into_iter()
            .filter(|e| !matches!(e, Expr::Attribute(attr) if attr.attr.id.as_str().starts_with('_')))
            .all(|e| self.is_constexpr(e, ctx, args_allowed, None))
    }

    fn evaluate_function_call(&mut self, call: &'a ExprCall, ctx: Ctx<'m>, args_allowed: bool, position: Option<usize>) -> bool {
        let model = self.model;
        let name = match call.func.as_ref() {
            Expr::Attribute(attr) => attr.attr.id.as_str(),
            Expr::Name(name) => name.id.as_str(),
            _ => return true,
        };
        let scope = &model.scopes[ctx.scope];
        if name == "SQL" || scope.is_generator || name == scope.name {
            return true;
        }
        let const_args = self.all_const(&call.arguments.args, ctx, args_allowed, None);
        let root_call = self.root_call.unwrap_or(call.range.start());
        self.callsites.entry(name).or_default().push(Callsite { position, const_args, root_call });
        let functions = self.known_functions.get(name).cloned().unwrap_or_default();
        functions.into_iter().all(|f| self.is_const_def(f, position, const_args))
    }

    /// All the values returned by the function are constant
    fn is_const_def(&mut self, function: usize, position: Option<usize>, const_args: bool) -> bool {
        let key = (function, position, const_args);
        if let Some(&result) = self.const_def_cache.get(&key) {
            return result;
        }
        if !self.in_progress_defs.insert(key) {
            return true;
        }
        let model = self.model;
        let record = &model.functions[function];
        let name = record.def.name.id.as_str();
        let result = name.starts_with("__") || FUNCTION_WHITELIST.contains(&name)
            || model.returns.iter()
                .filter(|ret| model.is_descendant(ret.scope, record.body_scope))
                .all(|ret| ret.value.is_some_and(|value| {
                    let ctx = Ctx { scope: ret.scope, pos: ret.pos, path: &ret.path };
                    self.is_constexpr(value, ctx, const_args, position)
                }));
        self.in_progress_defs.remove(&key);
        self.const_def_cache.insert(key, result);
        result
    }

    /// Function parameters of private methods, constants and private attributes can be used in queries
    fn is_allowable(&mut self, expr: &'a Expr, ctx: Ctx<'m>) -> bool {
        let scope = &self.model.scopes[ctx.scope];
        if scope.kind == ScopeKind::Function && (scope.name.starts_with('_') || scope.name == "init") {
            return true;
        }
        if self.is_constexpr(expr, ctx, false, None) {
            return true;
        }
        matches!(expr, Expr::Attribute(attr) if attr.value.is_name_expr() && attr.attr.id.as_str().starts_with('_'))
    }

    /// If the expression is a variable, find how it was built
    fn resolve(&self, expr: &'a Expr, ctx: Ctx<'m>) -> (&'a Expr, Ctx<'m>) {
        if let Expr::Name(name) = expr {
            for id in self.lookup(name.id.as_str(), ctx) {
                let binding = self.model.binding(id);
                if let BindingKind::Assign { value, is_assign_stmt: true } = binding.kind {
                    return (value, binding.ctx());
                }
            }
        }
        (expr, ctx)
    }

    /// Some(true) if the query is built with unsafe values, None if it can not be decided
    fn check_concatenation(&mut self, expr: &'a Expr, ctx: Ctx<'m>) -> Option<bool> {
        let (expr, ctx) = self.resolve(expr, ctx);
        if self.is_allowable(expr, ctx) {
            return Some(false);
        }
        match expr {
            Expr::BinOp(bin_op) if matches!(bin_op.op, Operator::Mod | Operator::Add) => {
                let unsafe_right = match bin_op.right.as_ref() {
                    Expr::Tuple(tuple) => !tuple.elts.iter().all(|e| self.is_allowable(e, ctx)),
                    Expr::Dict(dict) => !dict.items.iter().all(|item| self.is_allowable(&item.value, ctx)),
                    right => !self.is_allowable(right, ctx),
                };
                if unsafe_right {
                    return Some(true);
                }
                self.check_concatenation(&bin_op.left, ctx)
            }
            Expr::Call(call) if matches!(call.func.as_ref(), Expr::Attribute(attr) if attr.attr.id.as_str() == "format") => {
                let safe = call.arguments.args.iter().all(|arg| self.is_allowable(arg, ctx))
                    && call.arguments.keywords.iter().all(|k| self.is_allowable(&k.value, ctx));
                Some(!safe)
            }
            Expr::FString(fstring) => {
                let safe = fstring_interpolations(fstring, true).into_iter().all(|e| self.is_allowable(e, ctx));
                Some(!safe)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_printf_placeholders() {
        assert_eq!(count_printf_placeholders("%s and %s"), 2);
        assert_eq!(count_printf_placeholders("%(a)s and %(b)s"), 0);
        assert_eq!(count_printf_placeholders("100%% sure %s"), 1);
        assert_eq!(count_printf_placeholders("%05.2f %-3d"), 2);
        assert_eq!(count_printf_placeholders("%%s"), 0);
    }

    #[test]
    fn test_repr_placeholder() {
        assert!(has_repr_placeholder("value %r"));
        assert!(has_repr_placeholder("value %(name)r"));
        assert!(!has_repr_placeholder("value %s"));
    }

    #[test]
    fn test_dummy_names() {
        assert!(is_dummy_name("_"));
        assert!(is_dummy_name("_compute_total"));
        assert!(is_dummy_name("unused_var"));
        assert!(!is_dummy_name("__init__"));
        assert!(!is_dummy_name("compute"));
    }

    #[test]
    fn test_pragma() {
        let pragma = parse_pylint_pragma("# pylint: disable=sql-injection, W0123").unwrap();
        assert!(matches!(pragma.scope, PylintPragmaScope::LikeNoqa));
        match pragma.noqa {
            NoqaInfo::Codes(codes) => assert_eq!(codes, vec!["pylint_E8501".to_string(), "pylint_W0123".to_string()]),
            _ => panic!("expected codes"),
        }
        assert!(matches!(parse_pylint_pragma("# pylint: disable-next=E0602").unwrap().scope, PylintPragmaScope::NextLine));
        assert!(parse_pylint_pragma("# pylint: disable=missing-docstring").is_none());
        assert!(parse_pylint_pragma("# some comment").is_none());
    }
}
