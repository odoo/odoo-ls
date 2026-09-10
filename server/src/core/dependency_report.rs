use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use ruff_python_ast::{Alias, Identifier, Stmt, StmtTry};
use ruff_source_file::PositionEncoding;
use ruff_text_size::TextRange;
use serde_json::{json, Value};

use crate::core::entry_point::EntryPointType;
use crate::core::evaluation::EvaluationSymbolPtr;
use crate::core::file_mgr::{FileInfo, FileMgr};
use crate::core::import_resolver::manual_import;
use crate::core::symbols::symbol_keys::{ModuleKey, SourceFileKey, SymbolKey};
use crate::threads::SessionInfo;
use crate::S;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ImportContext {
    Direct,
    TryBody,
    ExceptHandler,
}

impl ImportContext {
    fn as_str(&self) -> &'static str {
        match self {
            ImportContext::Direct => "direct",
            ImportContext::TryBody => "try",
            ImportContext::ExceptHandler => "except",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum DependencyKind {
    OdooModule,
    PythonPackage,
}

struct Occurrence {
    file: String,
    line: u32,
    context: ImportContext,
}

#[derive(Default)]
struct DependencyEntry {
    occurrences: Vec<Occurrence>,
}

/// Walks every python file of a single odoo module, recording every import statement found
/// (including ones nested in functions/classes/try-except), classified as either another odoo
/// module or an external python package, together with the try/except-ImportError context it
/// was found under.
struct DependencyWalker<'a> {
    file_info_rc: Rc<RefCell<FileInfo>>,
    encoding: PositionEncoding,
    file_path: String,
    sym_stack: Vec<SymbolKey>,
    current_module: ModuleKey,
    deps: &'a mut BTreeMap<(DependencyKind, String), DependencyEntry>,
}

impl<'a> DependencyWalker<'a> {
    fn walk_body(&mut self, session: &mut SessionInfo, body: &[Stmt], context: ImportContext) {
        for stmt in body.iter() {
            match stmt {
                Stmt::Import(i) => self.record_import(session, None, &i.names, context),
                Stmt::ImportFrom(i) => self.record_import(session, i.module.as_ref(), &i.names, context),
                Stmt::Try(t) => self.walk_try(session, t, context),
                Stmt::FunctionDef(f) => self.walk_scoped(session, &f.name, &f.range, &f.body, context),
                Stmt::ClassDef(c) => self.walk_scoped(session, &c.name, &c.range, &c.body, context),
                Stmt::If(i) => {
                    self.walk_body(session, &i.body, context);
                    for clause in i.elif_else_clauses.iter() {
                        self.walk_body(session, &clause.body, context);
                    }
                },
                Stmt::For(f) => {
                    self.walk_body(session, &f.body, context);
                    self.walk_body(session, &f.orelse, context);
                },
                Stmt::While(w) => {
                    self.walk_body(session, &w.body, context);
                    self.walk_body(session, &w.orelse, context);
                },
                Stmt::With(w) => self.walk_body(session, &w.body, context),
                Stmt::Match(m) => {
                    for case in m.cases.iter() {
                        self.walk_body(session, &case.body, context);
                    }
                },
                _ => {},
            }
        }
    }

    fn walk_scoped(&mut self, session: &mut SessionInfo, name: &Identifier, range: &TextRange, body: &[Stmt], context: ImportContext) {
        let scope = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), name, range);
        if let Some(scope) = scope {
            self.sym_stack.push(scope);
            self.walk_body(session, body, context);
            self.sym_stack.pop();
        }
    }

    /// Only the try body and the bodies of handlers that catch a bare `ImportError` are tagged
    /// as `TryBody`/`ExceptHandler`; everything else (else/finally, other handlers) keeps the
    /// surrounding context unchanged.
    fn walk_try(&mut self, session: &mut SessionInfo, t: &StmtTry, context: ImportContext) {
        let catches_import_error: Vec<bool> = t.handlers.iter().map(|handler| {
            let handler = handler.as_except_handler().unwrap();
            handler.type_.as_ref().is_some_and(|type_| type_.is_name_expr() && type_.as_name_expr().unwrap().id == "ImportError")
        }).collect();
        let any_import_error = catches_import_error.iter().any(|c| *c);

        let body_context = if any_import_error && context == ImportContext::Direct { ImportContext::TryBody } else { context };
        self.walk_body(session, &t.body, body_context);

        for (handler, catches) in t.handlers.iter().zip(catches_import_error.iter()) {
            let handler = handler.as_except_handler().unwrap();
            let handler_context = if *catches && context == ImportContext::Direct { ImportContext::ExceptHandler } else { context };
            self.walk_body(session, &handler.body, handler_context);
        }

        self.walk_body(session, &t.orelse, context);
        self.walk_body(session, &t.finalbody, context);
    }

    fn record_import(&mut self, session: &mut SessionInfo, from_stmt: Option<&Identifier>, name_aliases: &[Alias], context: ImportContext) {
        let file_symbol = self.sym_stack[0];
        for alias in name_aliases.iter() {
            if alias.name.id == "*" {
                continue;
            }
            let var_name = match &alias.asname {
                Some(asname) => asname,
                None => alias.name.split(".").next().unwrap(),
            };
            let scope = *self.sym_stack.last().unwrap();
            let mut resolved_any = false;
            if let Some(variable) = session.st().get_positioned_symbol(scope, var_name, &alias.range) {
                let v = variable.unwrap_variable_key();
                let evaluations = session.st()[v].evaluations.clone();
                for evaluation in evaluations {
                    let mut diags = vec![];
                    let eval_sym = evaluation.symbol.get_symbol(session, None, &mut diags, Some(file_symbol));
                    if let EvaluationSymbolPtr::WEAK(w) = eval_sym
                        && let Some(symbol) = w.weak.upgrade(session.st()) {
                            resolved_any = true;
                            self.classify_resolved(session, symbol, from_stmt, alias, context);
                        }
                }
            }
            if !resolved_any {
                let name = python_root_name(from_stmt, alias);
                if name != "odoo" {
                    self.push_occurrence(name, DependencyKind::PythonPackage, context, alias.range);
                }
            }
            // deep dotted import statements, e.g. `import odoo.addons.<module>.<submodule>`
            if from_stmt.is_none() && alias.asname.is_none() && alias.name.contains('.') {
                let deep_results = manual_import(session, file_symbol, None, alias.name.as_str(), Some(S!("_")), 0, &mut None);
                for result in deep_results {
                    for symbol in result.symbols {
                        self.classify_resolved(session, symbol, from_stmt, alias, context);
                    }
                }
            }
        }
    }

    fn classify_resolved(&mut self, session: &mut SessionInfo, symbol: SymbolKey, from_stmt: Option<&Identifier>, alias: &Alias, context: ImportContext) {
        if let Some(module) = session.st().find_module(symbol) {
            if module == self.current_module {
                return; // relative import resolving back into the same module: not a dependency
            }
            let dir_name = session.st()[module].dir_name.to_string();
            self.push_occurrence(dir_name, DependencyKind::OdooModule, context, alias.range);
            return;
        }
        if session.st().get_entry(symbol).borrow().typ == EntryPointType::BUILTIN {
            return; // python stdlib: never declarable/needed in a manifest
        }
        let name = python_root_name(from_stmt, alias);
        if name == "odoo" {
            return; // the odoo framework itself, not a "dependency" of the module
        }
        self.push_occurrence(name, DependencyKind::PythonPackage, context, alias.range);
    }

    fn push_occurrence(&mut self, name: String, kind: DependencyKind, context: ImportContext, range: TextRange) {
        let position_range = self.file_info_rc.borrow().text_range_to_range(range, self.encoding);
        let line = position_range.start.line + 1;
        let entry = self.deps.entry((kind, name)).or_default();
        if !entry.occurrences.iter().any(|o| o.file == self.file_path && o.line == line && o.context == context) {
            entry.occurrences.push(Occurrence { file: self.file_path.clone(), line, context });
        }
    }
}

fn python_root_name(from_stmt: Option<&Identifier>, alias: &Alias) -> String {
    match from_stmt {
        Some(id) => id.split('.').next().unwrap().to_string(),
        None => alias.name.split('.').next().unwrap().to_string(),
    }
}

fn walk_module_tree(session: &mut SessionInfo, key: SymbolKey, module_key: ModuleKey, deps: &mut BTreeMap<(DependencyKind, String), DependencyEntry>) {
    match key {
        SymbolKey::Module(m) => {
            process_file(session, SourceFileKey::Module(m), key, module_key, deps);
            let children: Vec<SymbolKey> = session.st()[m].module_symbols().values().copied().collect();
            for child in children {
                walk_module_tree(session, child, module_key, deps);
            }
        },
        SymbolKey::PythonPackage(p) => {
            process_file(session, SourceFileKey::PythonPackage(p), key, module_key, deps);
            let children: Vec<SymbolKey> = session.st()[p].module_symbols().values().copied().collect();
            for child in children {
                walk_module_tree(session, child, module_key, deps);
            }
        },
        SymbolKey::File(f) => {
            process_file(session, SourceFileKey::File(f), key, module_key, deps);
        },
        _ => {},
    }
}

fn process_file(session: &mut SessionInfo, source_file_key: SourceFileKey, owner: SymbolKey, module_key: ModuleKey, deps: &mut BTreeMap<(DependencyKind, String), DependencyEntry>) {
    let (file_info_rc, loaded) = FileMgr::get_or_recreate_file_info(session, source_file_key);
    if !loaded {
        return;
    }
    if !file_info_rc.borrow().file_info_ast.borrow().ast.is_built() {
        file_info_rc.borrow_mut().prepare_ast(session);
    }
    let file_path = session.st().file_path(source_file_key).to_string();
    let ast_rc = file_info_rc.borrow().file_info_ast.clone();
    let ast = ast_rc.borrow();
    let Some(stmts) = ast.get_stmts() else {
        return;
    };
    let encoding = session.sync_odoo.encoding;
    let mut walker = DependencyWalker {
        file_info_rc: file_info_rc.clone(),
        encoding,
        file_path,
        sym_stack: vec![owner],
        current_module: module_key,
        deps,
    };
    walker.walk_body(session, stmts, ImportContext::Direct);
}

/// Where a used odoo module stands relative to the current module's manifest: declared directly
/// in `depends`, only reachable transitively through another declared dependency ("indirect" -
/// works today, but only because that other dependency happens to require it), or not available
/// through the manifest at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ManifestRelation {
    Direct,
    Indirect,
    Missing,
}

fn build_dependency_json(kind: DependencyKind, name: &str, direct_depends: &HashSet<String>, all_depends: &HashSet<String>, entry: DependencyEntry) -> Value {
    let occurrences: Vec<Value> = entry.occurrences.iter().map(|o| json!({
        "file": o.file,
        "line": o.line,
        "import_context": o.context.as_str(),
    })).collect();
    match kind {
        DependencyKind::OdooModule => {
            let relation = if direct_depends.contains(name) {
                ManifestRelation::Direct
            } else if all_depends.contains(name) {
                ManifestRelation::Indirect
            } else {
                ManifestRelation::Missing
            };
            json!({
                "name": name,
                "type": "odoo_module",
                "in_manifest": relation == ManifestRelation::Direct,
                "action_needed": relation != ManifestRelation::Direct,
                "occurrences": occurrences,
            })
        },
        DependencyKind::PythonPackage => {
            json!({
                "name": name,
                "type": "python_module",
                "in_manifest": false,
                "action_needed": true,
                "occurrences": occurrences,
            })
        },
    }
}

/// Build the `--list-python-dependencies` report: for every (non-external) odoo module found in
/// the session, every odoo-module and python-package dependency actually imported by its code,
/// whether it is already declared in the manifest, and where it was found.
pub fn generate_report(session: &mut SessionInfo) -> Value {
    let module_keys: Vec<(String, ModuleKey)> = session.sync_odoo.modules.iter()
        .filter_map(|(name, wk)| wk.upgrade(session.st()).map(|m| (name.to_string(), m)))
        .filter(|(_, m)| !session.st()[*m].is_external)
        .collect();

    let mut modules_json = serde_json::Map::new();
    for (module_name, module_key) in module_keys {
        let mut deps: BTreeMap<(DependencyKind, String), DependencyEntry> = BTreeMap::new();
        walk_module_tree(session, SymbolKey::Module(module_key), module_key, &mut deps);

        let direct_depends: HashSet<String> = session.st()[module_key].depends.iter()
            .map(|(n, _)| n.to_string())
            .collect();
        let all_depends: HashSet<String> = session.st()[module_key].get_all_depends().iter()
            .map(|n| n.to_string())
            .collect();

        let dependencies_json: Vec<Value> = deps.into_iter()
            .map(|((kind, name), entry)| build_dependency_json(kind, &name, &direct_depends, &all_depends, entry))
            .collect();

        let mut manifest_depends_sorted: Vec<&String> = all_depends.iter().collect();
        manifest_depends_sorted.sort();
        let manifest_depends_json: Vec<Value> = manifest_depends_sorted.into_iter().map(|name| json!({
            "name": name,
            "direct": direct_depends.contains(name),
        })).collect();

        modules_json.insert(module_name, json!({
            "manifest_depends": manifest_depends_json,
            "dependencies": dependencies_json,
        }));
    }

    json!({ "modules": Value::Object(modules_json) })
}
