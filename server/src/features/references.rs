use std::{cell::RefCell, path::PathBuf, rc::Rc};

use lsp_types::{Location, Range};
use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::{constants::SymType, core::{file_mgr::{FileInfo, FileMgr}, symbols::symbol::Symbol}, features::ast_utils::AstUtils, features::xml_ast_utils::{XmlAstResult, XmlAstUtils}, threads::SessionInfo, utils::PathSanitizer};


pub struct ReferenceFeature {

}

/// Visitor to find all Name expressions in an AST
struct NameFinderVisitor<'a> {
    target_name: &'a str,
    matches: Vec<TextRange>,
}

impl<'a> NameFinderVisitor<'a> {
    fn new(target_name: &'a str) -> Self {
        Self {
            target_name,
            matches: Vec::new(),
        }
    }

    fn find_all_names(stmts: &[Stmt], target_name: &str) -> Vec<TextRange> {
        let mut visitor = NameFinderVisitor::new(target_name);
        for stmt in stmts {
            visitor.visit_stmt(stmt);
        }
        visitor.matches
    }
}

impl<'a> Visitor<'a> for NameFinderVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Name(name_expr) = expr {
            if name_expr.id.as_str() == self.target_name {
                self.matches.push(name_expr.range());
            }
        }
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
    }
}

impl ReferenceFeature {
    /// Basic find all references implementation
    /// Same-file references only, local vars, params, methods
    /// TODO: Cross-file references within module
    /// TODO: All files within workspace
    /// TODO: Odoo specific (XML field refs, string-based model refs)
    pub fn get_references(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Vec<Location>> {
        let offset = file_info.borrow().position_to_offset(line, character);

        let (analyse_ast_result, _range, _call_expr) = AstUtils::get_symbols(session, file_symbol, file_info, offset as u32);

        if analyse_ast_result.evaluations.is_empty() {
            return None;
        }

        let eval = &analyse_ast_result.evaluations[0];
        let target_symbol = eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None);
        let target_symbol_rc = target_symbol.weak.upgrade()?;

        let symbol_name = target_symbol_rc.borrow().name().to_string();

        // find all Name expressions with the same name
        let file_info_ast = file_info.borrow().file_info_ast.clone();
        let file_info_ast_borrow = file_info_ast.borrow();
        let stmts = file_info_ast_borrow.get_stmts()?;

        let name_matches = NameFinderVisitor::find_all_names(stmts, &symbol_name);

        if name_matches.is_empty() {
            return None;
        }

        let file_path = file_symbol.borrow().paths()[0].clone();
        let mut locations = Vec::new();

        for match_range in name_matches {
            // infer name
            let match_offset = match_range.start().to_u32();
            let scope = Symbol::get_scope_symbol(file_symbol.clone(), match_offset, false);
            AstUtils::build_scope(session, &scope);

            let inferred = Symbol::infer_name(&mut session.sync_odoo, &scope, &symbol_name, Some(match_offset));

            // do any of the inferred symbols match our target symbol
            let refers_to_same_symbol = inferred.symbols.iter().any(|sym| {
                Rc::ptr_eq(sym, &target_symbol_rc)
            });

            if refers_to_same_symbol {
                let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &file_path, &match_range);
                locations.push(Location {
                    uri: FileMgr::pathname2uri(&file_path),
                    range,
                });
            }
        }

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    pub fn get_references_xml(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Vec<Location>> {
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
        let document = roxmltree::Document::parse(&data);
        if let Ok(document) = document {
            let root = document.root_element();
            let (symbols, _range) = XmlAstUtils::get_symbols(session, file_symbol, root, offset, false);
            if symbols.is_empty() {
                return None;
            }
            let mut links = vec![];
            for xml_result in symbols.iter() {
                match xml_result {
                    crate::features::xml_ast_utils::XmlAstResult::SYMBOL(s) => {
                        if let Some(file) = s.borrow().get_file() {
                            for path in file.upgrade().unwrap().borrow().paths().iter() {
                                let full_path = match file.upgrade().unwrap().borrow().typ() {
                                    SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", file.upgrade().unwrap().borrow().as_package().i_ext())).sanitize(),
                                    _ => path.clone()
                                };
                                let range = match s.borrow().typ() {
                                    SymType::PACKAGE(_) | SymType::FILE | SymType::NAMESPACE | SymType::DISK_DIR => Range::default(),
                                    _ => session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &full_path, &s.borrow().range()),
                                };
                                links.push(Location{uri: FileMgr::pathname2uri(&full_path), range});
                            }
                        }
                    },
                    XmlAstResult::XML_DATA(xml_file_symbol, range) => {
                        for path in xml_file_symbol.borrow().paths().iter() {
                            let full_path = match xml_file_symbol.borrow().typ() {
                                SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", xml_file_symbol.borrow().as_package().i_ext())).sanitize(),
                                _ => path.clone()
                            };
                            let range = match xml_file_symbol.borrow().typ() {
                                SymType::PACKAGE(_) | SymType::FILE | SymType::NAMESPACE | SymType::DISK_DIR => Range::default(),
                                _ => session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &full_path, &range),
                            };
                            links.push(Location{uri: FileMgr::pathname2uri(&full_path), range: range});
                        }
                    }
                }
            }
            return Some(links);
        }
        None
    }

    pub fn get_references_csv(_session: &mut SessionInfo, _file_symbol: &Rc<RefCell<Symbol>>, _file_info: &Rc<RefCell<FileInfo>>, _line: u32, _character: u32) -> Option<Vec<Location>> {
        None
    }
}