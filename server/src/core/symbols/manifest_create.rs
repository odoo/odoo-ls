use std::path::Path;

use lsp_types::{Diagnostic, DiagnosticTag, Position, Range};
use ruff_python_ast::{Expr, ExprStringLiteral, Stmt};
use ruff_text_size::Ranged;
use tracing::info;

use crate::{constants::{DEBUG_STEPS, DiagnosticSource}, core::{diagnostics::{DiagnosticCode, create_diagnostic}, file_mgr::FileInfo, symbols::{ModuleSymbol, symbol_keys::ModuleKey}}, oyarn, threads::SessionInfo, utils::{HashSet, PathSanitizer}};



impl ModuleSymbol {

    pub fn load_manifest_content(session: &mut SessionInfo, module_key: ModuleKey) {
        let manifest_path = Path::new(&session.st()[module_key].path).join("__manifest__.py");
        if DEBUG_STEPS {
            info!("ARCH       - MANIFEST: {}", manifest_path.sanitize_cow());
        }
        let (_, manifest_file_info) = session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, &manifest_path.sanitize_cow(), None, None, false);
        let mut manifest_file_info = (*manifest_file_info).borrow_mut();
        if manifest_file_info.file_info_ast.borrow().ast.as_py_ast().indexed_module.is_none() {
            return;
        }
        let diags = ModuleSymbol::load_manifest(session, module_key, &manifest_file_info);
        if session.sync_odoo.modules.contains_key(&session.st()[module_key].dir_name) {
            //TODO: handle multiple modules with the same name
        }
        manifest_file_info.replace_diagnostics(DiagnosticSource::PY_SYNTAX, diags);
        manifest_file_info.publish_diagnostics(session);
        info!("Detected module: {:?}", session.st()[module_key].path);
    }


    /* Load manifest to identify the module characteristics.
    Returns list of od diagnostics to publish in manifest file. */
    fn load_manifest(session: &mut SessionInfo, module_key: ModuleKey, file_info: &FileInfo) -> Vec<Diagnostic> {
        let mut res = vec![];
        let file_info_ast = file_info.file_info_ast.borrow();
        let ast = file_info_ast.get_stmts().unwrap();
        if ast.len() != 1 || !matches!(ast.first(), Some(Stmt::Expr(expr)) if expr.value.is_dict_expr()) {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04001, &[]) {
                res.push(Diagnostic {
                    range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                    ..diagnostic
                });
            }
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
                            if visited_keys.contains(key_str)
                            && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04002, &[]) {
                                res.push(Diagnostic {
                                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                                    ..diagnostic
                                });
                            }
                            visited_keys.insert(key_str);
                            if key_str == "name" {
                                ModuleSymbol::load_manifest_name(session, module_key, &mut res, key_literal, value);
                            } else if key_str == "depends" {
                                ModuleSymbol::load_manifest_depends(session, module_key, &mut res, key_literal, value);
                            } else if key_str == "data" {
                                ModuleSymbol::load_manifest_data(session, module_key, &mut res, key_literal, value);
                            } else if key_str == "assets" {
                                ModuleSymbol::load_manifest_assets(session, module_key, &mut res, key_literal, value);
                            } else if key_str == "active"
                                && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS03302, &[]) {
                                    res.push(Diagnostic {
                                        range: Range::new(Position::new(key_literal.range().start().to_u32(), 0), Position::new(key_literal.range().end().to_u32(), 0)),
                                        tags: Some(vec![DiagnosticTag::DEPRECATED]),
                                        ..diagnostic
                                    });
                                }
                        }
                        _ => {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04009, &[]) {
                                    res.push(Diagnostic {
                                        range: Range::new(Position::new(key.range().start().to_u32(), 0), Position::new(key.range().end().to_u32(), 0)),
                                        ..diagnostic
                                    });
                            }
                        }
                    }
                },
                None => {
                    if let Some(diagnostic_base) = create_diagnostic(session, DiagnosticCode::OLS04011, &[]) {
                        res.push(Diagnostic {
                            range: Range::new(Position::new(0, 0), Position::new(0, 1)),
                            ..diagnostic_base.clone()
                        });
                    }
                    return res;
                }
            }
        }
        res
    }

    fn load_manifest_name(session: &mut SessionInfo, module_key: ModuleKey, diagnostics: &mut Vec<Diagnostic>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_string_literal_expr() {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04003, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        } else {
            let module = &mut session.st_mut()[module_key];
            module.module_name = oyarn!("{}", value.as_string_literal_expr().unwrap().value);
        }
    }

    fn load_manifest_depends(session: &mut SessionInfo, module_key: ModuleKey, diagnostics: &mut Vec<Diagnostic>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_list_expr() {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04004, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        } else {
            for depend in value.as_list_expr().unwrap().elts.iter() {
                if !depend.is_string_literal_expr() {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04005, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(depend.range().start().to_u32(), 0), Position::new(depend.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                } else {
                    let depend_value = oyarn!("{}", depend.as_string_literal_expr().unwrap().value);
                    let module = &mut session.st_mut()[module_key];
                    if depend_value == module.dir_name {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04006, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range::new(Position::new(depend.range().start().to_u32(), 0), Position::new(depend.range().end().to_u32(), 0)),
                                ..diagnostic
                            });
                        }
                    } else {
                        module.depends.push((depend_value, depend.range()));
                    }
                }
            }
        }
    }

    fn load_manifest_data(session: &mut SessionInfo, module_key: ModuleKey, diagnostics: &mut Vec<Diagnostic>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_list_expr() {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04007, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        } else {
            for data in value.as_list_expr().unwrap().elts.iter() {
                if !data.is_string_literal_expr() {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04008, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(data.range().start().to_u32(), 0), Position::new(data.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                } else {
                    let module = &mut session.st_mut()[module_key];
                    module.data.push((data.as_string_literal_expr().unwrap().value.to_string(), data.range()));
                }
            }
        }
    }

    fn load_manifest_assets(session: &mut SessionInfo, module_key: ModuleKey, diagnostics: &mut Vec<Diagnostic>, key_literal: &ExprStringLiteral, value: &Expr) {
        if !value.is_dict_expr() {
            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04013, &[]) {
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(key_literal.range.start().to_u32(), 0), Position::new(key_literal.range.end().to_u32(), 0)),
                    ..diagnostic
                });
            }
        } else {
            for data in value.as_dict_expr().unwrap().items.iter() {
                if data.key.is_none() {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04014, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(data.range().start().to_u32(), 0), Position::new(data.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                    continue;
                }
                if !data.key.as_ref().unwrap().is_string_literal_expr()
                    && let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04015, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(data.range().start().to_u32(), 0), Position::new(data.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                if !data.value.is_list_expr() {
                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04016, &[]) {
                        diagnostics.push(Diagnostic {
                            range: Range::new(Position::new(data.range().start().to_u32(), 0), Position::new(data.range().end().to_u32(), 0)),
                            ..diagnostic
                        });
                    }
                    continue;
                }
                for item in data.value.as_list_expr().unwrap().iter() {
                    let module = &mut session.st_mut()[module_key];
                    if item.is_string_literal_expr() {
                        module.assets.push((item.as_string_literal_expr().unwrap().value.to_string(), item.range()));
                    } else if item.is_tuple_expr() {
                        if item.as_tuple_expr().unwrap().elts.is_empty() {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04018, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(item.range().start().to_u32(), 0), Position::new(item.range().end().to_u32(), 0)),
                                    ..diagnostic
                                });
                            }
                            continue;
                        }
                        let first_element = item.as_tuple_expr().unwrap().elts.first().unwrap();
                        if !first_element.is_string_literal_expr() {
                            if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04018, &[]) {
                                diagnostics.push(Diagnostic {
                                    range: Range::new(Position::new(item.range().start().to_u32(), 0), Position::new(item.range().end().to_u32(), 0)),
                                    ..diagnostic
                                });
                            }
                            continue;
                        }
                        let first_element_str = first_element.as_string_literal_expr().unwrap().value.to_str();
                        match first_element_str {
                            "before" | "after" | "replace" => {
                                if item.as_tuple_expr().unwrap().elts.len() != 3 {
                                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04020, &["3"]) {
                                        diagnostics.push(Diagnostic {
                                            range: Range::new(Position::new(item.range().start().to_u32(), 0), Position::new(item.range().end().to_u32(), 0)),
                                            ..diagnostic
                                        });
                                    }
                                    continue;
                                }
                                for value in item.as_tuple_expr().unwrap().elts.iter().skip(1) {
                                    if !value.is_string_literal_expr() {
                                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04018, &[]) {
                                            diagnostics.push(Diagnostic {
                                                range: Range::new(Position::new(value.range().start().to_u32(), 0), Position::new(value.range().end().to_u32(), 0)),
                                                ..diagnostic
                                            });
                                        }
                                        continue;
                                    }
                                }
                            },
                            "append" | "include" | "remove" | "prepend" => {
                                if item.as_tuple_expr().unwrap().elts.len() != 2 {
                                    if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04020, &["2"]) {
                                        diagnostics.push(Diagnostic {
                                            range: Range::new(Position::new(item.range().start().to_u32(), 0), Position::new(item.range().end().to_u32(), 0)),
                                            ..diagnostic
                                        });
                                    }
                                    continue;
                                }
                                for value in item.as_tuple_expr().unwrap().elts.iter().skip(1) {
                                    if !value.is_string_literal_expr() {
                                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04018, &[]) {
                                            diagnostics.push(Diagnostic {
                                                range: Range::new(Position::new(value.range().start().to_u32(), 0), Position::new(value.range().end().to_u32(), 0)),
                                                ..diagnostic
                                            });
                                        }
                                        continue;
                                    }
                                }
                            }
                            _ => {
                                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04019, &[]) {
                                    diagnostics.push(Diagnostic {
                                        range: Range::new(Position::new(first_element.range().start().to_u32(), 0), Position::new(first_element.range().end().to_u32(), 0)),
                                        ..diagnostic
                                    });
                                }
                                continue;
                            }
                        }
                    } else {
                        if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS04017, &[]) {
                            diagnostics.push(Diagnostic {
                                range: Range::new(Position::new(item.range().start().to_u32(), 0), Position::new(item.range().end().to_u32(), 0)),
                                ..diagnostic
                            });
                        }
                    }
                }
            }
        }
    }
}
