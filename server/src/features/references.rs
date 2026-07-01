use crate::constants::{BuildSteps, PackageType};
use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr};
use crate::core::file_mgr::Ast;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::Dependencies;
use crate::core::symbols::ModuleSymbol;
use crate::core::symbols::symbol_keys::{ModuleKey, SourceFileKey, SymbolKey};
use crate::core::symbols::storage::SymbolTable;
use crate::features::goto_utils::{GotoRequest, GotoSourceType, GotoUtils};
use crate::features::references_csv::CsvAstReferenceVisitor;
use crate::features::references_xml::XmlAstReferenceVisitor;
use crate::{
    constants::SymType,
    core::file_mgr::{FileInfo, FileMgr},
    threads::SessionInfo,
    utils::PathSanitizer,
};
use lsp_types::Location;
use ruff_python_ast::{
    Alias, Expr, Identifier, Stmt, StmtAnnAssign, StmtAssert, StmtAssign, StmtAugAssign,
    StmtClassDef, StmtIf, StmtMatch, StmtRaise, StmtReturn, StmtTry, StmtTypeAlias, StmtWith,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use tracing::error;
use crate::utils::HashSet;
use std::{cell::RefCell, path::PathBuf, rc::Rc};

#[derive(Debug, Clone)]
pub enum ReferenceTarget {
    Symbol(SymbolKey),
    String(String),
}

impl ReferenceTarget {
    pub fn as_string(&self) -> Option<&String> {
        match self {
            ReferenceTarget::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_symbol(&self) -> Option<SymbolKey> {
        match self {
            ReferenceTarget::Symbol(s) => Some(*s),
            _ => None,
        }
    }
}

pub struct ReferenceFeature {

}

impl ReferenceFeature {
    /*
    * Get all References to a symbol at the provided line and char
     */
    /// TODO: Odoo specific (XML field refs, string-based model refs)
    pub fn get_references(session: &mut SessionInfo, file_symbol: SourceFileKey, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Vec<Location>> {
        if matches!(file_info.borrow().file_info_ast.borrow().ast, Ast::JsAst(_)) {
            return ReferenceFeature::get_reference_js(session, file_info, line, character);
        }
        //We want to search for references of the definition, and not the current symbol. Let's use definition feature for that
        SyncOdoo::process_rebuilds(session, false);
        let def_sources = match file_info.borrow().file_info_ast.borrow().ast {
            Ast::PythonAst(_) => {
                GotoUtils::get_symbols(session, GotoRequest::Definition, file_symbol, file_info, line, character)
            },
            Ast::XmlAst => {
                GotoUtils::get_symbols_xml(session, file_symbol, file_info, line, character)
            },
            Ast::CsvAst => {
                GotoUtils::get_symbols_csv(session, file_symbol, file_info, line, character)
            },
            Ast::JsAst(_) => unreachable!(),
        };


        let mut locations = Vec::new();
        for definition in def_sources.iter() {
            let GotoSourceType::SymbolKey(definition_source) = definition.source else {
                continue;
            };
            match &definition_source.typ() {
                SymType::PACKAGE(PackageType::MODULE) => {
                    let module_key = definition_source.unwrap_module_key();
                    let module_name = session.st()[module_key].name.clone();
                    let modules: Vec<_> = session.sync_odoo.modules.values().copied().collect();
                    for module in modules {
                        if let Some(module) = module.upgrade(session.st()) {
                            if session.st()[module].name == module_name {
                                locations.extend(ReferenceFeature::find_name_in_manifest(session, module));
                            }
                            if session.st()[module].depends.iter().any(|(dep, _range)| dep == &module_name) {
                                locations.extend(ReferenceFeature::find_depend_in_manifest(session, module, &module_name));
                            }
                        }
                    }
                },
                SymType::CLASS | SymType::FUNCTION | SymType::VARIABLE | SymType::FILE => {
                    let mut files_to_check = HashSet::default();

                    files_to_check.insert(file_symbol);

                    if let Some(target_file) = session.st().get_file(definition_source) {
                        let dependents = session.st().dependents(target_file);
                        //take arch and arch_eval dependents
                        for dep_level in [BuildSteps::ARCH, BuildSteps::ARCH_EVAL] {
                            for dep_set in dependents[dep_level as usize].iter() {
                                for dep_symbol_key in dep_set.iter_valid(session.st()) {
                                    files_to_check.insert(dep_symbol_key);
                                }
                            }
                        }
                    }
                    //If the symbol is a model, a field, or a method on a model, browse model dependents too
                    let target_typ = definition_source.typ();
                    let class_model_to_check = if target_typ == SymType::CLASS {
                        Some(definition_source)
                    } else if SymbolTable::is_field(session, definition_source) || target_typ == SymType::FUNCTION {
                        session.st().get_in_parents(definition_source, &[SymType::CLASS], true)
                    } else {
                        None
                    };
                    if let Some(SymbolKey::Class(class_model)) = class_model_to_check
                        && let Some(model_data) = session.st()[class_model]._model.as_ref()
                        && let Some(model) = session.sync_odoo.models.get(&model_data.name).cloned()
                    {
                        files_to_check.extend(model.borrow().dependents.iter_valid(session.st()));
                        for symbol in model.borrow().get_model_symbols(session.st(), None) {
                            if let Some(file) = session.st().get_file(symbol.into()) {
                                files_to_check.insert(file);
                            }
                        }
                        for symbol in model.borrow().get_xml_model_field_symbols(session.st(), None) {
                            if let Some(file) = session.st().get_file(symbol.into()) {
                                files_to_check.insert(file);
                            }
                        }
                    }
                    for &file in files_to_check.iter() {
                        let Some(dep_file_info) = session.sync_odoo.get_file_mgr().borrow().get_file_info(session.st().path(file)) else {
                            continue;
                        };
                        match file {
                            SourceFileKey::File(_) | SourceFileKey::PythonPackage(_) | SourceFileKey::Module(_) => {
                                locations.extend(ReferenceFeature::references_in_file(session, file, &dep_file_info, &ReferenceTarget::Symbol(definition_source)));
                            },
                            SourceFileKey::XmlFile(xml_file) => {
                                let data = dep_file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
                                let document = roxmltree::Document::parse(&data);
                                if let Ok(document) = document {
                                    let root = document.root_element();
                                    locations.extend(XmlAstReferenceVisitor::search_target(session, xml_file, root, &ReferenceTarget::Symbol(definition_source)));
                                }
                            },
                            SourceFileKey::CsvFile(csv_file) => {
                                if SymbolTable::is_field(session, definition_source) {
                                    let data = dep_file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
                                    let mut csv_reader = csv::ReaderBuilder::new().from_reader(data.as_bytes());
                                    let model_class = session.st().get_in_parents(definition_source, &[SymType::CLASS], true);
                                    if let Some(SymbolKey::Class(model_class)) = model_class
                                        && let Some(model) = &session.st()[model_class]._model {
                                            let model_name = model.name.clone();
                                            locations.extend(CsvAstReferenceVisitor::search_target(session, csv_file, &mut csv_reader, Some(&model_name), &ReferenceTarget::Symbol(definition_source), &data));
                                        }
                                }
                            },
                            SourceFileKey::JsFile(_) => { unreachable!("tsserverbridge should have handled js references")}
                        }
                    }
                    //add definition
                    let sym_typ = definition_source.typ();
                    if matches!(sym_typ, SymType::CLASS | SymType::FUNCTION | SymType::VARIABLE) {
                        let file = session.st().get_file(definition_source).unwrap();
                        let path = session.st().path(file);
                        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(path);
                        if let Some(file_info) = file_info {
                            let transformed_range = file_info.borrow().text_range_to_range(session.st().range(definition_source), session.sync_odoo.encoding);
                            let uri = FileMgr::pathname2uri(path);
                            locations.push(Location {
                                uri: uri,
                                range: transformed_range,
                            });
                        }
                    }
                },
                SymType::XML_ASSET | SymType::XML_DELETE | SymType::XML_RECORD | SymType::XML_MENUITEM | SymType::XML_TEMPLATE => {
                    let xml_data_key = definition_source.as_xml_data_key().unwrap();
                    let xml_id = session.st().get_xml_id(xml_data_key);
                    let Some(xml_id) = xml_id else {continue;};
                    //we do not have any dependency for xml-id usage. So let's search in the current module and all modules that depend on it.
                    let xml_id_file = session.st().get_file(definition_source).unwrap();
                    let current_module = session.st().find_module(xml_id_file).unwrap();
                    let data_module_name = session.st()[current_module].dir_name.clone();
                    let mut files_to_process = HashSet::default();
                    //TODO do not process all modules in the dep tree, but use dependencies on XML files when we will have them
                    let modules: Vec<_> = session.sync_odoo.modules.values().copied().collect();
                    for module in modules {
                        let Some(module) = module.upgrade(session.st()) else {continue;};
                        let name = &session.st()[current_module].name;
                        if ModuleSymbol::is_in_deps(session.st(), module, name) {
                            for &data in session.st()[module].data_symbols().values() {
                                files_to_process.insert(data);
                            }
                        }
                    }
                    //add python dependencies
                    let dependents = session.st()[current_module].dependents();
                    for dep_level in [BuildSteps::ARCH, BuildSteps::ARCH_EVAL] {
                        for dep_set in dependents[dep_level as usize].iter() {
                            for dep_symbol_key in dep_set.iter_valid(session.st()) {
                                files_to_process.insert(dep_symbol_key);
                            }
                        }
                    }
                    for source_file in files_to_process {
                        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(session.st().file_path(source_file));
                        if let Some(file_info) = file_info {
                            let full_xml_id = format!("{}.{}", data_module_name, xml_id);
                            match source_file {
                                SourceFileKey::File(_)| SourceFileKey::PythonPackage(_) | SourceFileKey::Module(_) => {
                                    locations.extend(ReferenceFeature::references_in_file(session, source_file, &file_info, &ReferenceTarget::String(full_xml_id)));
                                },
                                SourceFileKey::XmlFile(xml_file) => {
                                    let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
                                    let document = roxmltree::Document::parse(&data);
                                    if let Ok(document) = document {
                                        let root = document.root_element();
                                        locations.extend(XmlAstReferenceVisitor::search_target(session, xml_file, root, &ReferenceTarget::String(full_xml_id)));
                                    }
                                },
                                SourceFileKey::CsvFile(csv_file) => {
                                    let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
                                    let mut csv_reader = csv::ReaderBuilder::new().from_reader(data.as_bytes());
                                    locations.extend(CsvAstReferenceVisitor::search_target(session, csv_file, &mut csv_reader, None, &ReferenceTarget::String(full_xml_id), &data));
                                },
                                SourceFileKey::JsFile(_) => {unreachable!("tsserverbridge should have handled js references")}
                            }
                        }
                    }
                },
                _ => {
                    error!("Unsupported definition source type for reference: {}", definition_source.typ());
                }
            }
        }

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    fn get_reference_js(session: &mut SessionInfo, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Vec<Location>> {
        let file_path = &file_info.borrow().uri;
        let locs: Vec<Location> = if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
            bridge.get_references(&file_path, line, character)
                .into_iter()
                .map(|(target_file, sl, sc, el, ec)| Location {
                    uri: FileMgr::pathname2uri(&target_file),
                    range: lsp_types::Range {
                        start: lsp_types::Position { line: sl, character: sc },
                        end:   lsp_types::Position { line: el, character: ec },
                    },
                })
                .collect()
        } else {
            vec![]
        };
        return if locs.is_empty() { None } else { Some(locs) };
    }

    fn references_in_file(session: &mut SessionInfo, file_symbol: SourceFileKey, file_info: &Rc<RefCell<FileInfo>>, reference_target: &ReferenceTarget) -> Vec<Location> {
        let file_info_ast = file_info.borrow().file_info_ast.clone();
        if file_info_ast.borrow().get_stmts().is_none() { //filter modules with only manifest or file outside of workspace
            return vec![];
        }
        let mut visitor = ReferenceVisitor {
            sym_stack: vec![],
        };
        session.sync_odoo.evaluation_locations = vec![];
        session.sync_odoo.evaluation_search = Some(reference_target.clone());
        visitor.browse_file(session, file_symbol, file_info_ast.borrow().get_stmts().as_ref().unwrap());
        session.sync_odoo.evaluation_search = None;
        let mut locations = std::mem::take(&mut session.sync_odoo.evaluation_locations);
        // Innermost expressions are analyzed first, so stable-sort by position
        // and keep the first hit at each (uri, position) - that's the most
        // specific token. Replaces a per-push O(N) scan with one O(N log N).
        locations.sort_by(|a, b| (a.uri.as_str(), a.range.start.line, a.range.start.character)
            .cmp(&(b.uri.as_str(), b.range.start.line, b.range.start.character)));
        locations.dedup_by(|a, b| a.uri == b.uri && a.range.start == b.range.start);
        locations
    }

    //the name inside the manifest will be different, so we only search for the key in this module and return the location with the range
    fn find_name_in_manifest(session: &mut SessionInfo, module: ModuleKey) -> Vec<Location> {
        let mut locations = Vec::new();
        let manifest = PathBuf::from(&session.st()[module].path).join("__manifest__.py").sanitize();
        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest).clone();
        if let Some(file_info) = file_info {
            let file_info_ast = file_info.borrow().file_info_ast.clone();
            let file_info_ast = file_info_ast.borrow();
            if let Some(stmts) = file_info_ast.get_stmts() {
                if stmts.len() != 1 {
                    return locations;
                }
                let Stmt::Expr(e) = &stmts[0] else {return locations;};
                let Expr::Dict(d) = &*e.value else {return locations;};
                for dict_item in d.items.iter() {
                    let Some(key) = dict_item.key.as_ref() else {continue;};
                    let value = &dict_item.value;
                    let Expr::StringLiteral(key_s) = key else {continue;};
                    let Expr::StringLiteral(value_s) = value else {continue;};
                    if key_s.value.to_str() == "name" {
                        let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &manifest, &value_s.range());
                        locations.push(Location {
                            uri: FileMgr::pathname2uri(&manifest),
                            range,
                        });
                    }
                }
            }
        }
        locations
    }

    fn find_depend_in_manifest(session: &mut SessionInfo, module: ModuleKey, module_name: &str) -> Vec<Location> {
        let mut locations = Vec::new();
        let manifest = PathBuf::from(&session.st()[module].path).join("__manifest__.py").sanitize();
        let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(&manifest).clone();
        if let Some(file_info) = file_info {
            let file_info_ast = file_info.borrow().file_info_ast.clone();
            let file_info_ast = file_info_ast.borrow();
            if let Some(stmts) = file_info_ast.get_stmts() {
                if stmts.len() != 1 {
                    return locations;
                }
                let Stmt::Expr(e) = &stmts[0] else {return locations;};
                let Expr::Dict(d) = &*e.value else {return locations;};
                for dict_item in d.items.iter() {
                    let Some(key) = dict_item.key.as_ref() else {continue;};
                    let value = &dict_item.value;
                    let Expr::StringLiteral(key_s) = key else {continue;};
                    let Expr::List(depends_d) = value else {continue;};
                    if key_s.value.to_str() == "depends" {
                        for value in depends_d.elts.iter() {
                            let Expr::StringLiteral(value_s) = value else {continue;};
                            if value_s.value.to_str() == module_name {
                                let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &manifest, &value_s.range());
                                locations.push(Location {
                                    uri: FileMgr::pathname2uri(&manifest),
                                    range,
                                });
                            }
                        }
                    }
                }
            }
        }
        locations
    }
}

struct ReferenceVisitor {
    sym_stack: Vec<SymbolKey>,
}

impl ReferenceVisitor {

    pub fn browse_file(&mut self, session: &mut SessionInfo, file_symbol: SourceFileKey, stmts: &[Stmt]) {
        self.sym_stack.push(file_symbol.into());
        self.visit_vec_stmt(session, stmts);
    }

    pub fn visit_vec_stmt(&mut self, session: &mut SessionInfo, vec_ast: &[Stmt]) {
        for stmt in vec_ast.iter() {
            match stmt {
                Stmt::FunctionDef(f) => {
                    let sym = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), &f.name, &f.range);
                    if let Some(sym) = sym {
                        self.sym_stack.push(sym);
                        self.visit_vec_stmt(session, &f.body);
                        self.sym_stack.pop();
                    }
                },
                Stmt::ClassDef(c) => {
                    self.visit_class_def(session, c);
                },
                Stmt::Try(t) => {
                    self.visit_try(session, t);
                },
                Stmt::Import(i) => {
                    self._resolve_import(session, None, &i.names, None, &i.range);
                },
                Stmt::ImportFrom(i) => {
                    self._resolve_import(session, i.module.as_ref(), &i.names, Some(i.level), &i.range);
                },
                Stmt::Assign(a) => {
                    self.visit_assign(session, a);
                },
                Stmt::AnnAssign(a) => {
                    self.visit_ann_assign(session, a);
                },
                Stmt::Expr(e) => {
                    self.visit_expr(session, &e.value, &e.value.start());
                },
                Stmt::If(i) => {
                    self.visit_if(session, i);
                },
                Stmt::Break(_) => {},
                Stmt::Continue(_) => {},
                Stmt::Delete(d) => {
                    for target in d.targets.iter() {
                        self.visit_expr(session, target, &target.start());
                    }
                },
                Stmt::For(f) => {
                    self.visit_expr(session, &f.target, &f.target.start());
                    self.visit_vec_stmt(session, &f.body);
                    self.visit_vec_stmt(session, &f.orelse);
                },
                Stmt::While(w) => {
                    self.visit_expr(session, &w.test, &w.test.start());
                    self.visit_vec_stmt(session, &w.body);
                    self.visit_vec_stmt(session, &w.orelse);
                },
                Stmt::Return(stmt_return) => self.visit_return_stmt(session, stmt_return),
                Stmt::AugAssign(stmt_aug_assign) => self.visit_aug_assign(session, stmt_aug_assign),
                Stmt::TypeAlias(stmt_type_alias) => self.visit_type_alias(session, stmt_type_alias),
                Stmt::With(stmt_with) => self.visit_with(session, stmt_with),
                Stmt::Match(stmt_match) => self.visit_match(session, stmt_match),
                Stmt::Raise(stmt_raise) => self.visit_raise(session, stmt_raise),
                Stmt::Assert(stmt_assert) => self.visit_assert(session, stmt_assert),
                Stmt::Global(_) => {},
                Stmt::Nonlocal(_) => {},
                Stmt::Pass(_) => {},
                Stmt::IpyEscapeCommand(_) => {},
            }
        };
    }

    fn _resolve_import(&mut self, session: &mut SessionInfo, _from_stmt: Option<&Identifier>, name_aliases: &[Alias], _level: Option<u32>, _range: &TextRange) {
        let file_symbol = session.st().get_file(self.sym_stack[0]);
        let file_symbol = file_symbol.expect("file symbol not found");
        let Some(eval_search) = session.sync_odoo.evaluation_search.clone() else {
            return;
        };
        let eval_search_sym = match eval_search {
            ReferenceTarget::Symbol(s) => s,
            _ => return,
        };
        for alias in name_aliases.iter() {
            if alias.name.id == "*" {
                continue;
            }
            let var_name = if alias.asname.is_none() {
                alias.name.split(".").next().unwrap()
            } else {
                alias.asname.as_ref().unwrap()
            };
            let variable = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), var_name, &alias.range);
            let Some(SymbolKey::Variable(variable_key)) = variable else { continue };
            for evaluation in session.st()[variable_key].evaluations.clone() {
                let eval_sym = evaluation.symbol.get_symbol(session, None, &mut vec![], Some(file_symbol.into()));
                let EvaluationSymbolPtr::WEAK(w) = eval_sym else {
                    panic!("Internal error: evaluated has invalid evaluationType");
                };
                if let Some(symbol) = w.weak.upgrade(session.st())
                    && symbol == eval_search_sym {
                        let path = session.st().path(file_symbol).to_string();
                        let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, &alias.range);
                        session.sync_odoo.evaluation_locations.push(Location {
                            uri: FileMgr::pathname2uri(&path),
                            range,
                        });
                    }
            }
        }
    }

    fn visit_class_def(&mut self, session: &mut SessionInfo, c: &StmtClassDef) {
        let sym = session.st().get_positioned_symbol(*self.sym_stack.last().unwrap(), &c.name, &c.range);
        if let Some(sym) = sym {
            self.sym_stack.push(sym);
            self.visit_vec_stmt(session, &c.body);
            self.sym_stack.pop();
        }
    }

    fn visit_expr(&mut self, session: &mut SessionInfo, expr: &Expr, max_infer: &TextSize) {
        Evaluation::eval_from_ast(session, expr, *self.sym_stack.last().unwrap(), max_infer, false, &mut vec![]);
    }

    fn visit_assert(&mut self, session: &mut SessionInfo, stmt_assert: &StmtAssert) {
        self.visit_expr(session, &stmt_assert.test, &stmt_assert.range.start());
        if let Some(msg) = stmt_assert.msg.as_ref() {
            self.visit_expr(session, msg, &stmt_assert.range.start());
        }
    }

    fn visit_raise(&mut self, session: &mut SessionInfo, stmt_raise: &StmtRaise) {
        if let Some(exc) = stmt_raise.exc.as_ref() {
            self.visit_expr(session, exc, &stmt_raise.range.start());
        }
    }

    fn visit_match(&mut self, session: &mut SessionInfo, stmt_match: &StmtMatch) {
        self.visit_expr(session, &stmt_match.subject, &stmt_match.range.start());
        for case in stmt_match.cases.iter() {
            if let Some(guard) = case.guard.as_ref() {
                self.visit_expr(session, guard, &case.pattern.start());
            }
            self.visit_vec_stmt(session, &case.body);
        }
    }

    fn visit_type_alias(&mut self, session: &mut SessionInfo, stmt_type_alias: &StmtTypeAlias) {
        self.visit_expr(session, &stmt_type_alias.value, &stmt_type_alias.range.start());
    }

    fn visit_aug_assign(&mut self, session: &mut SessionInfo, assign: &StmtAugAssign) {
        self.visit_expr(session, &assign.value, &assign.range.start());
    }

    fn visit_return_stmt(&mut self, session: &mut SessionInfo, stmt_return: &StmtReturn) {
        if let Some(value) = stmt_return.value.as_ref() {
            self.visit_expr(session, value, &stmt_return.range.start());
        }
    }

    fn visit_if(&mut self, session: &mut SessionInfo, node: &StmtIf) {
        self.visit_expr(session, &node.test, &node.test.start());
        self.visit_vec_stmt(session, &node.body);
        for elses in node.elif_else_clauses.iter() {
            if let  Some(test) = &elses.test {
                self.visit_expr(session, test, &test.start());
            }
            self.visit_vec_stmt(session, &elses.body);
        }
    }

    fn visit_with(&mut self, session: &mut SessionInfo, stmt_with: &StmtWith) {
        for item in stmt_with.items.iter() {
            self.visit_expr(session, &item.context_expr, &stmt_with.range.start());
        }
        self.visit_vec_stmt(session, &stmt_with.body);
    }

    fn visit_assign(&mut self, session: &mut SessionInfo, assign: &StmtAssign) {
        for assign_target in assign.targets.iter() {
            self.visit_expr(session, assign_target, &assign.range.start());
        }
        self.visit_expr(session, &assign.value, &assign.range.start());
    }

    fn visit_ann_assign(&mut self, session: &mut SessionInfo, assign: &StmtAnnAssign) {
        self.visit_expr(session, &assign.target, &assign.range.start());
        if let Some(value) = assign.value.as_ref() {
            self.visit_expr(session, value, &assign.range.start());
        }
    }

    fn visit_try(&mut self, session: &mut SessionInfo, node: &StmtTry) {
        //TODO handle handlers of try
        self.visit_vec_stmt(session, &node.body);
    }
}
