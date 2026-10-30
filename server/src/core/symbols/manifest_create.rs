use std::path::Path;

use lsp_types::DiagnosticTag;
use ruff_python_ast::{Expr, ExprStringLiteral, Stmt};
use ruff_text_size::{Ranged, TextRange};
use tracing::info;

use crate::{constants::{DEBUG_STEPS, DiagnosticSource}, core::{diagnostics::{Diag, DiagnosticCode, create_diag}, file_mgr::{FileInfo, FileMgr}, symbols::{ModuleSymbol, symbol_keys::ModuleKey}}, features::node_index_ast::IndexedModule, oyarn, threads::SessionInfo, utils::{HashSet, PathSanitizer}};



impl ModuleSymbol {

    pub fn load_manifest_content(session: &mut SessionInfo, module_key: ModuleKey) {
        let manifest_path = Path::new(&session.st()[module_key].path).join("__manifest__.py");
        if DEBUG_STEPS {
            info!("ARCH       - MANIFEST: {}", manifest_path.sanitize_cow());
        }
        let (_, manifest_file_info) = FileMgr::update_file_info(session, &manifest_path.sanitize_cow(), None, None, false);
        let Some(py_ast) = session.file_mgr()[manifest_file_info].file_info_ast.ast.as_py_ast().cloned() else {
            return;
        };
        let diags = ModuleSymbol::load_manifest(session, module_key, &py_ast);
        if session.sync_odoo.modules.contains_key(&session.st()[module_key].dir_name) {
            //TODO: handle multiple modules with the same name
        }
        session.file_mgr_mut()[manifest_file_info].replace_diagnostics(DiagnosticSource::PY_SYNTAX, diags);
        FileInfo::publish_diagnostics(session, manifest_file_info);
        info!("Detected module: {:?}", session.st()[module_key].path);
    }


    /* Load manifest to identify the module characteristics.
    Returns list of od diagnostics to publish in manifest file. */
    fn load_manifest(session: &mut SessionInfo, module_key: ModuleKey, py_ast: &IndexedModule) -> Vec<Diag> {
        let mut res = vec![];
        let ast = py_ast.get_stmts();
        if ast.len() != 1 || !matches!(ast.first(), Some(Stmt::Expr(expr)) if expr.value.is_dict_expr()) {
            // @todo: decide the range in which to display diag (original one was ((0, 0), (0, 1)))
            res.extend(create_diag(session, DiagnosticCode::OLS04001, &[], TextRange::default()));
            return res;
        }
        let mut visited_keys = HashSet::default();
        let dict = &ast[0].as_expr_stmt().unwrap().value.clone().dict_expr().unwrap();
        for (index, key) in dict.iter_keys().enumerate() {
            match key {
                Some(key) => {
                    let value = &dict.items.get(index).unwrap().value;
                    match key {
                        Expr::StringLiteral(key_literal) => {
                            let key_str = key_literal.value.to_str();
                            if visited_keys.contains(key_str) {
                                res.extend(create_diag(session, DiagnosticCode::OLS04002, &[], key_literal.range));
                            }
                            visited_keys.insert(key_str);
                            if key_str == "name" {
                                ModuleSymbol::load_manifest_name(session, module_key, &mut res, key_literal, value);
                            } else if key_str == "depends" {
                                ModuleSymbol::load_manifest_depends(session, module_key, &mut res, key_literal, value);
                            } else if key_str == "data" {
                                ModuleSymbol::load_manifest_data(session, module_key, &mut res, key_literal, value);
                            } else if key_str == "assets" {
                                ModuleSymbol::load_manifest_assets(session, &mut res, key_literal, value);
                            } else if key_str == "active"
                                && let Some(mut diagnostic) = create_diag(session, DiagnosticCode::OLS03302, &[], key_literal.range()) {
                                    diagnostic.add_tag(DiagnosticTag::DEPRECATED);
                                    res.push(diagnostic);
                                }
                        }
                        _ => {
                            res.extend(create_diag(session, DiagnosticCode::OLS04009, &[], key.range()));
                        }
                    }
                },
                None => {
                    // @todo: empty range?? (originally 0,0 - 0,1)
                    res.extend(create_diag(session, DiagnosticCode::OLS04011, &[], TextRange::default()));
                    return res;
                }
            }
        }
        res
    }

    fn load_manifest_name(session: &mut SessionInfo, module_key: ModuleKey, diagnostics: &mut Vec<Diag>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_string_literal_expr() {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS04003, &[], key_literal.range));
        } else {
            let module = &mut session.st_mut()[module_key];
            module.module_name = oyarn!("{}", value.as_string_literal_expr().unwrap().value);
        }
    }

    fn load_manifest_depends(session: &mut SessionInfo, module_key: ModuleKey, diagnostics: &mut Vec<Diag>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_list_expr() {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS04004, &[], key_literal.range));
        } else {
            for depend in value.as_list_expr().unwrap().elts.iter() {
                if !depend.is_string_literal_expr() {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS04005, &[], depend.range()));
                } else {
                    let depend_value = oyarn!("{}", depend.as_string_literal_expr().unwrap().value);
                    let module = &mut session.st_mut()[module_key];
                    if depend_value == module.dir_name {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS04006, &[], depend.range()));
                    } else {
                        module.depends.push((depend_value, depend.range()));
                    }
                }
            }
        }
    }

    fn load_manifest_data(session: &mut SessionInfo, module_key: ModuleKey, diagnostics: &mut Vec<Diag>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_list_expr() {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS04007, &[], key_literal.range));
        } else {
            for data in value.as_list_expr().unwrap().elts.iter() {
                if !data.is_string_literal_expr() {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS04008, &[], data.range()));
                } else {
                    let module = &mut session.st_mut()[module_key];
                    module.data.push((data.as_string_literal_expr().unwrap().value.to_string(), data.range()));
                }
            }
        }
    }

    fn load_manifest_assets(session: &mut SessionInfo, diagnostics: &mut Vec<Diag>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_dict_expr() {
            diagnostics.extend(create_diag(session, DiagnosticCode::OLS04013, &[], key_literal.range));
        } else {
            for data in value.as_dict_expr().unwrap().items.iter() {
                if data.key.is_none() {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS04014, &[], data.range()));
                    continue;
                }
                if !data.key.as_ref().unwrap().is_string_literal_expr() {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS04015, &[], data.range()));
                }
                if !data.value.is_list_expr() {
                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS04016, &[], data.range()));
                    continue;
                }
                // Nothing is kept: asset discovery is a `static/` walk. Only the shape is checked.
                for item in data.value.as_list_expr().unwrap().iter() {
                    if item.is_tuple_expr() {
                        if item.as_tuple_expr().unwrap().elts.is_empty() {
                            diagnostics.extend(create_diag(session, DiagnosticCode::OLS04018, &[], item.range()));
                            continue;
                        }
                        let first_element = item.as_tuple_expr().unwrap().elts.first().unwrap();
                        if !first_element.is_string_literal_expr() {
                            diagnostics.extend(create_diag(session, DiagnosticCode::OLS04018, &[], item.range()));
                            continue;
                        }
                        let first_element_str = first_element.as_string_literal_expr().unwrap().value.to_str();
                        match first_element_str {
                            "before" | "after" | "replace" => {
                                if item.as_tuple_expr().unwrap().elts.len() != 3 {
                                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS04020, &["3"], item.range()));
                                    continue;
                                }
                                for value in item.as_tuple_expr().unwrap().elts.iter().skip(1) {
                                    if !value.is_string_literal_expr() {
                                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS04018, &[], value.range()));
                                        continue;
                                    }
                                }
                            },
                            "append" | "include" | "remove" | "prepend" => {
                                if item.as_tuple_expr().unwrap().elts.len() != 2 {
                                    diagnostics.extend(create_diag(session, DiagnosticCode::OLS04020, &["2"], item.range()));
                                    continue;
                                }
                                for value in item.as_tuple_expr().unwrap().elts.iter().skip(1) {
                                    if !value.is_string_literal_expr() {
                                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS04018, &[], value.range()));
                                        continue;
                                    }
                                }
                            }
                            _ => {
                                diagnostics.extend(create_diag(session, DiagnosticCode::OLS04019, &[], first_element.range()));
                                continue;
                            }
                        }
                    } else if !item.is_string_literal_expr() {
                        diagnostics.extend(create_diag(session, DiagnosticCode::OLS04017, &[], item.range()));
                    }
                }
            }
        }
    }
}
