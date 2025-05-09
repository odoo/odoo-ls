use std::{cell::RefCell, rc::Rc};

use lsp_types::{DocumentSymbol, DocumentSymbolResponse, Range, SymbolKind};
use ruff_python_ast::{Expr, Stmt, StmtAnnAssign, StmtAssign, StmtAugAssign, StmtClassDef, StmtFor, StmtFunctionDef, StmtGlobal, StmtIf, StmtImport, StmtImportFrom, StmtMatch, StmtNonlocal, StmtTry, StmtTypeAlias, StmtWhile, StmtWith};
use ruff_text_size::Ranged;

use crate::{constants::SymType, core::{file_mgr::FileInfo, python_utils::{unpack_assign, Assign, AssignTargetType}, symbols::symbol::Symbol}, threads::SessionInfo};


pub struct DocumentSymbolFeature;

impl DocumentSymbolFeature {

    pub fn get_symbols(session: &mut SessionInfo, file_info: &Rc<RefCell<FileInfo>>) -> Option<DocumentSymbolResponse> {
        let mut results = vec![];
        let file_info_bw = file_info.borrow();
        let file_info_ast = file_info_bw.file_info_ast.borrow();
        if let Some(ast) = &file_info_ast.ast {
            for stmt in ast.iter() {
                DocumentSymbolFeature::visit_stmt(session, stmt, &mut results, file_info);
            }
        }
        if results.is_empty() {
            return None;
        }
        Some(DocumentSymbolResponse::Nested(results))
    }

    fn visit_stmt(session: &mut SessionInfo, stmt: &Stmt, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>) {
        match stmt {
            Stmt::FunctionDef(stmt_function_def) => {DocumentSymbolFeature::visit_function(session, results, file_info, stmt_function_def)},
            Stmt::ClassDef(stmt_class_def) => {DocumentSymbolFeature::visit_class(session, results, file_info, stmt_class_def)},
            Stmt::Assign(stmt_assign) => {DocumentSymbolFeature::visit_assign(session, results, file_info, stmt_assign)},
            Stmt::AugAssign(stmt_aug_assign) => {DocumentSymbolFeature::visit_aug_assign(session, results, file_info, stmt_aug_assign)},
            Stmt::AnnAssign(stmt_ann_assign) => {DocumentSymbolFeature::visit_ann_assign(session, results, file_info, stmt_ann_assign)},
            Stmt::TypeAlias(stmt_type_alias) => {DocumentSymbolFeature::visit_type_alias(session, results, file_info, stmt_type_alias)},
            Stmt::For(stmt_for) => {DocumentSymbolFeature::visit_for(session, results, file_info, stmt_for)},
            Stmt::While(stmt_while) => {DocumentSymbolFeature::visit_while(session, results, file_info, stmt_while)},
            Stmt::If(stmt_if) => {DocumentSymbolFeature::visit_if(session, results, file_info, stmt_if)},
            Stmt::With(stmt_with) => {DocumentSymbolFeature::visit_with(session, results, file_info, stmt_with)},
            Stmt::Match(stmt_match) => {DocumentSymbolFeature::visit_match(session, results, file_info, stmt_match)},
            Stmt::Try(stmt_try) => {DocumentSymbolFeature::visit_try(session, results, file_info, stmt_try)},
            Stmt::Import(stmt_import) => {DocumentSymbolFeature::visit_import(session, results, file_info, stmt_import)},
            Stmt::ImportFrom(stmt_import_from) => {DocumentSymbolFeature::visit_import_from(session, results, file_info, stmt_import_from)},
            Stmt::Global(stmt_global) => {DocumentSymbolFeature::visit_global(session, results, file_info, stmt_global)},
            Stmt::Nonlocal(stmt_nonlocal) => {DocumentSymbolFeature::visit_nonlocal(session, results, file_info, stmt_nonlocal)},
            _ => {}
        }
    }

    fn visit_function(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_function_def: &StmtFunctionDef) {
        if stmt_function_def.name.to_string().is_empty() {
            return;
        }
        let mut children_symbols: Vec<DocumentSymbol> = vec![];
        for arg in stmt_function_def.parameters.kwonlyargs.iter().map(|x| &x.parameter)
            .chain(stmt_function_def.parameters.args.iter().map(|x| &x.parameter))
            .chain(stmt_function_def.parameters.vararg.iter().map(|x| &**x))
            .chain(stmt_function_def.parameters.kwonlyargs.iter().map(|x| &x.parameter))
            .chain(stmt_function_def.parameters.kwarg.iter().map(|x| &**x)) {
                children_symbols.push(DocumentSymbol{
                name: arg.name.id.to_string(),
                detail: None,
                kind: SymbolKind::VARIABLE,
                tags: None,
                deprecated: None,
                range: Range{
                    start: file_info.borrow().offset_to_position(arg.range.start().to_usize()),
                    end: file_info.borrow().offset_to_position(arg.range.end().to_usize()),
                },
                selection_range: Range{
                    start: file_info.borrow().offset_to_position(arg.range.start().to_usize()),
                    end: file_info.borrow().offset_to_position(arg.range.end().to_usize()),
                },
                children: None
            });
        }
        for child in stmt_function_def.body.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, &mut children_symbols, file_info);
        }
        results.push(DocumentSymbol{
            name: stmt_function_def.name.to_string(),
            detail: None,
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: Range{
                start: file_info.borrow().offset_to_position(stmt_function_def.range.start().to_usize()),
                end: file_info.borrow().offset_to_position(stmt_function_def.range.end().to_usize()),
            },
            selection_range: Range{
                start: file_info.borrow().offset_to_position(stmt_function_def.range.start().to_usize()),
                end: file_info.borrow().offset_to_position(stmt_function_def.range.end().to_usize()),
            },
            children: Some(children_symbols)
        });
    }

    fn visit_class(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_class_def: &StmtClassDef) {
        if stmt_class_def.name.to_string().is_empty() {
            return;
        }
        let mut children_symbols: Vec<DocumentSymbol> = vec![];
        for child in stmt_class_def.body.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, &mut children_symbols, file_info);
        }
        results.push(DocumentSymbol{
            name: stmt_class_def.name.to_string(),
            detail: None,
            kind: SymbolKind::CLASS,
            tags: None,
            deprecated: None,
            range: Range{
                start: file_info.borrow().offset_to_position(stmt_class_def.range.start().to_usize()),
                end: file_info.borrow().offset_to_position(stmt_class_def.range.end().to_usize()),
            },
            selection_range: Range{
                start: file_info.borrow().offset_to_position(stmt_class_def.range.start().to_usize()),
                end: file_info.borrow().offset_to_position(stmt_class_def.range.end().to_usize()),
            },
            children: Some(children_symbols)
        });
    }

    fn visit_assign(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_assign: &StmtAssign) {
        let assigns = unpack_assign(&stmt_assign.targets, None, None);
        DocumentSymbolFeature::build_assign_results(session, results, file_info, assigns);
    }

    fn visit_aug_assign(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_aug_assign: &StmtAugAssign) {
        let assigns = unpack_assign(&vec![*stmt_aug_assign.target.clone()], None, None);
        DocumentSymbolFeature::build_assign_results(session, results, file_info, assigns);
    }

    fn visit_ann_assign(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_ann_assign: &StmtAnnAssign) {
        let assigns = unpack_assign(&vec![*stmt_ann_assign.target.clone()], None, None);
        DocumentSymbolFeature::build_assign_results(session, results, file_info, assigns);
    }

    fn build_assign_results(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, assigns: Vec<Assign>) {
        for assign in assigns.iter() {
            match assign.target {
                AssignTargetType::Name(ref target_name) => {
                    results.push(DocumentSymbol{
                        name: target_name.id.to_string(),
                        detail: None,
                        kind: SymbolKind::VARIABLE,
                        tags: None,
                        deprecated: None,
                        range: Range{
                            start: file_info.borrow().offset_to_position(target_name.range.start().to_usize()),
                            end: file_info.borrow().offset_to_position(target_name.range.end().to_usize()),
                        },
                        selection_range: Range{
                            start: file_info.borrow().offset_to_position(target_name.range.start().to_usize()),
                            end: file_info.borrow().offset_to_position(target_name.range.end().to_usize()),
                        },
                        children: None,
                    });
                },
                AssignTargetType::Attribute(ref attr_target) => {

                }
            }
        }
    }

    fn visit_type_alias(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_type_alias: &StmtTypeAlias) {
        let name = match *stmt_type_alias.name {
            Expr::Name(ref name) => name.clone(),
            _ => {return;}
        };
        results.push(DocumentSymbol{
            name: name.id.to_string(),
            detail: None,
            kind: SymbolKind::VARIABLE,
            tags: None,
            deprecated: None,
            range: Range{
                start: file_info.borrow().offset_to_position(stmt_type_alias.range.start().to_usize()),
                end: file_info.borrow().offset_to_position(stmt_type_alias.range.end().to_usize()),
            },
            selection_range: Range{
                start: file_info.borrow().offset_to_position(stmt_type_alias.range.start().to_usize()),
                end: file_info.borrow().offset_to_position(stmt_type_alias.range.end().to_usize()),
            },
            children: None
        });
    }

    fn visit_for(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_for: &StmtFor) {
        let unpacked = unpack_assign(&vec![*stmt_for.target.clone()], None, None);
        DocumentSymbolFeature::build_assign_results(session, results, file_info, unpacked);
        for child in stmt_for.body.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
        }
        //TODO should split evaluations as in if
        for child in stmt_for.orelse.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
        }
    }

    fn visit_while(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_while: &StmtWhile) {
        //TODO search for walrus operator in condition
        for child in stmt_while.body.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
        }
        //TODO should split evaluations as in if
        for child in stmt_while.orelse.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
        }
    }

    fn visit_if(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_if: &StmtIf) {
        //TODO search for walrus operator in condition
        for child in stmt_if.body.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
        }
        //TODO should split evaluations as in if
        for _else in stmt_if.elif_else_clauses.iter() {
            for child in _else.body.iter() {
                DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
            }
        }
    }

    fn visit_with(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_with: &StmtWith) {
        for item in stmt_with.items.iter() {
            if let Some(var) = &item.optional_vars {
                let name = match **var {
                    Expr::Name(ref name) => name.clone(),
                    _ => {continue;}
                };
                results.push(DocumentSymbol{
                    name: name.id.to_string(),
                    detail: None,
                    kind: SymbolKind::VARIABLE,
                    tags: None,
                    deprecated: None,
                    range: Range{
                        start: file_info.borrow().offset_to_position(var.range().start().to_usize()),
                        end: file_info.borrow().offset_to_position(var.range().end().to_usize()),
                    },
                    selection_range: Range{
                        start: file_info.borrow().offset_to_position(var.range().start().to_usize()),
                        end: file_info.borrow().offset_to_position(var.range().end().to_usize()),
                    },
                    children: None
                });
            }
        }
        for child in stmt_with.body.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
        }
    }

    fn visit_match(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_match: &StmtMatch) {
        for case in stmt_match.cases.iter() {
            //TODO handle pattern
            for child in case.body.iter() {
                DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
            }
        }
    }

    fn visit_try(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_match: &StmtTry) {
        for child in stmt_match.body.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
        }
        for handler in stmt_match.handlers.iter() {
            if let Some(handler) = handler.as_except_handler() {
                if let Some(name) = &handler.name {
                    results.push(DocumentSymbol{
                        name: name.id.to_string(),
                        detail: None,
                        kind: SymbolKind::VARIABLE,
                        tags: None,
                        deprecated: None,
                        range: Range{
                            start: file_info.borrow().offset_to_position(name.range().start().to_usize()),
                            end: file_info.borrow().offset_to_position(name.range().end().to_usize()),
                        },
                        selection_range: Range{
                            start: file_info.borrow().offset_to_position(name.range().start().to_usize()),
                            end: file_info.borrow().offset_to_position(name.range().end().to_usize()),
                        },
                        children: None
                    });
                }
                for child in handler.body.iter() {
                    DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
                }
            }
        }
        for child in stmt_match.orelse.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
        }
        for child in stmt_match.finalbody.iter() {
            DocumentSymbolFeature::visit_stmt(session, child, results, file_info);
        }
    }

    fn visit_import(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_import: &StmtImport) {
        for name in stmt_import.names.iter() {
            results.push(DocumentSymbol{
                name: name.name.to_string(),
                detail: None,
                kind: SymbolKind::VARIABLE,
                tags: None,
                deprecated: None,
                range: Range{
                    start: file_info.borrow().offset_to_position(name.range.start().to_usize()),
                    end: file_info.borrow().offset_to_position(name.range.end().to_usize()),
                },
                selection_range: Range{
                    start: file_info.borrow().offset_to_position(name.range.start().to_usize()),
                    end: file_info.borrow().offset_to_position(name.range.end().to_usize()),
                },
                children: None
            });
        }
    }

    fn visit_import_from(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_import_from: &StmtImportFrom) {
        for name in stmt_import_from.names.iter() {
            results.push(DocumentSymbol{
                name: name.name.to_string(),
                detail: None,
                kind: SymbolKind::VARIABLE,
                tags: None,
                deprecated: None,
                range: Range{
                    start: file_info.borrow().offset_to_position(name.range.start().to_usize()),
                    end: file_info.borrow().offset_to_position(name.range.end().to_usize()),
                },
                selection_range: Range{
                    start: file_info.borrow().offset_to_position(name.range.start().to_usize()),
                    end: file_info.borrow().offset_to_position(name.range.end().to_usize()),
                },
                children: None
            });
        }
    }

    fn visit_global(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_global: &StmtGlobal) {
        for name in stmt_global.names.iter() {
            results.push(DocumentSymbol{
                name: name.id.to_string(),
                detail: None,
                kind: SymbolKind::VARIABLE,
                tags: None,
                deprecated: None,
                range: Range{
                    start: file_info.borrow().offset_to_position(name.range.start().to_usize()),
                    end: file_info.borrow().offset_to_position(name.range.end().to_usize()),
                },
                selection_range: Range{
                    start: file_info.borrow().offset_to_position(name.range.start().to_usize()),
                    end: file_info.borrow().offset_to_position(name.range.end().to_usize()),
                },
                children: None
            });
        }
    }

    fn visit_nonlocal(session: &mut SessionInfo, results: &mut Vec<DocumentSymbol>, file_info: &Rc<RefCell<FileInfo>>, stmt_nonlocal: &StmtNonlocal) {
        for name in stmt_nonlocal.names.iter() {
            results.push(DocumentSymbol{
                name: name.id.to_string(),
                detail: None,
                kind: SymbolKind::VARIABLE,
                tags: None,
                deprecated: None,
                range: Range{
                    start: file_info.borrow().offset_to_position(name.range.start().to_usize()),
                    end: file_info.borrow().offset_to_position(name.range.end().to_usize()),
                },
                selection_range: Range{
                    start: file_info.borrow().offset_to_position(name.range.start().to_usize()),
                    end: file_info.borrow().offset_to_position(name.range.end().to_usize()),
                },
                children: None
            });
        }
    }

}