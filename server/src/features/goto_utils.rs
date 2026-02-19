use std::{cell::RefCell, path::PathBuf, rc::Rc};

use lsp_types::{LocationLink, Range};
use ruff_python_ast::{Expr, ExprCall};

use crate::{S, constants::{PackageType, SymType}, core::{evaluation::{Evaluation, EvaluationValue, ExprOrIdent}, file_mgr::{FileInfo, FileMgr}, odoo::SyncOdoo, python_odoo_builder::MAGIC_FIELDS, symbols::symbol::Symbol, xml_data::OdooData}, features::{ast_utils::AstUtils, features_utils::FeaturesUtils}, oyarn, threads::SessionInfo, utils::PathSanitizer};

pub enum GotoRequest {
    Definition,
    Declaration,
}

pub enum GotoSourceType {
    Symbol(Rc<RefCell<Symbol>>),
    OdooData(OdooData),
    Module(Rc<RefCell<Symbol>>),
}

pub struct GotoSource {
    pub source: GotoSourceType,
    pub origin_selection_range: Option<Range>,
}
pub struct GotoUtils {}

impl GotoUtils {
    fn check_for_domain_field(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, call_expr: &Option<ExprCall>, offset: usize, sources: &mut Vec<GotoSource>) -> bool {
        let (field_name, field_range) = if let Some(eval_value) = eval.value.as_ref() {
            if let EvaluationValue::CONSTANT(Expr::StringLiteral(expr)) = eval_value {
                (expr.value.to_string(), expr.range)
            } else {
                return false;
            }
        } else {
            return false;
        };
        let Some(call_expr) = call_expr else { return false };
        let module = file_symbol.borrow().find_module();
        let string_domain_fields = FeaturesUtils::find_argument_symbols(
            session, Symbol::get_scope_symbol(file_symbol.clone(), offset as u32, false), module, &field_name, call_expr, offset, field_range
        );
        let mut domain_found = false;
        string_domain_fields.iter().for_each(|(field, field_range)|{
            if let Some(_) = field.borrow().get_file().and_then(|file_sym_weak| file_sym_weak.upgrade()){
                domain_found = true;
                sources.push(GotoSource {
                    source:
                    GotoSourceType::Symbol(field.clone()),
                    origin_selection_range: Some(session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), &field_range))
                });
            }
        });
        domain_found
    }

    fn check_for_model_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, sources: &mut Vec<GotoSource>) -> bool {
        let value = if let Some(eval_value) = eval.value.as_ref() {
            if let EvaluationValue::CONSTANT(Expr::StringLiteral(expr)) = eval_value {
                oyarn!("{}", expr.value.to_string())
            } else {
                return false;
            }
        } else {
            return  false;
        };
        let model = session.sync_odoo.models.get(&value).cloned();
        let Some(model) = model else {
            return false;
        };
        let mut model_found = false;
        let from_module = file_symbol.borrow().find_module();
        let classes = model.borrow().get_symbols(session, from_module.clone());
        let len_classes = classes.len();
        for class_symbol_rc in classes {
            let class_symbol = class_symbol_rc.borrow();
            if let (Some(eval_range), Some(class_file)) = (eval.range, class_symbol.get_file().and_then(|file_sym_weak| file_sym_weak.upgrade())) {
                if Rc::ptr_eq(file_symbol, &class_file) && class_symbol.range().contains(eval_range.start()) && len_classes > 1{
                    continue; // if we are already on the class, skip, unless it is the only result
                }
            }
            model_found = true;
            sources.push(GotoSource {
                source: GotoSourceType::Symbol(class_symbol_rc.clone()),
                origin_selection_range: eval.range.map(|r| session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), &r))
            });
        }
        model_found
    }

    fn check_for_module_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, file_path: &String, sources: &mut Vec<GotoSource>) -> bool {
        if file_symbol.borrow().typ() != SymType::PACKAGE(PackageType::MODULE) || !file_path.ends_with("__manifest__.py") {
            // If not on manifest, we don't check for modules
            return false;
        };
        let value = if let Some(eval_value) = eval.value.as_ref() {
            if let EvaluationValue::CONSTANT(Expr::StringLiteral(expr)) = eval_value {
                oyarn!("{}", expr.value.to_string())
            } else {
                return false;
            }
        } else {
            return  false;
        };
        let Some(module) = session.sync_odoo.modules.get(&oyarn!("{}", value)).and_then(|m| m.upgrade()) else {
            return false;
        };
        sources.push(GotoSource{
            source: GotoSourceType::Module(module.clone()),
            origin_selection_range: None,
        });
        true
    }

    fn check_for_xml_id_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, sources: &mut Vec<GotoSource>) -> bool {
        let value = if let Some(eval_value) = eval.value.as_ref() {
            if let EvaluationValue::CONSTANT(Expr::StringLiteral(expr)) = eval_value {
                oyarn!("{}", expr.value.to_string())
            } else {
                return false;
            }
        } else {
            return  false;
        };
        let mut xml_found = false;
        let xml_ids = SyncOdoo::get_xml_ids(session, file_symbol, value.as_str(), &std::ops::Range{start: 0, end: 0}, &mut vec![]);
        for xml_id in xml_ids {
            let file = xml_id.get_file_symbol();
            if let Some(file) = file {
                if let Some(_file) = file.upgrade() { //test that file is valid to set xml_found to true
                    xml_found = true;
                    sources.push(GotoSource{
                        source: GotoSourceType::OdooData(xml_id.clone()),
                        origin_selection_range: eval.range.map(|r| session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), &r))
                    });
                }
            }
        }
        xml_found
    }

    fn check_for_compute_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, call_expr: &Option<ExprCall>, offset: usize, sources: &mut Vec<GotoSource>) -> bool {
        let value = if let Some(eval_value) = eval.value.as_ref() {
            if let EvaluationValue::CONSTANT(Expr::StringLiteral(expr)) = eval_value {
                expr.value.to_string()
            } else {
                return false;
            }
        } else {
            return  false;
        };
        let Some(call_expr) = call_expr else { return false };
        let method_symbols = FeaturesUtils::find_kwarg_methods_symbols(
            session, Symbol::get_scope_symbol(file_symbol.clone(), offset as u32, false), file_symbol.borrow().find_module(), &value, call_expr, &offset
        );
        let mut method_found = false;
        method_symbols.iter().for_each(|field|{
            if let Some(file_sym) = field.borrow().get_file().and_then(|file_sym_weak| file_sym_weak.upgrade()){
                method_found = true;
                sources.push(GotoSource {
                    source: GotoSourceType::Symbol(field.clone()),
                    origin_selection_range: eval.range.map(|r| session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, file_sym.borrow().paths().first().as_ref().unwrap(), &r))
                });
            }
        });
        method_found
    }

    pub fn add_display_name_compute_methods(session: &mut SessionInfo, sources: &mut Vec<GotoSource>, expr: &ExprOrIdent, file_symbol: &Rc<RefCell<Symbol>>, offset: usize) {
        // now we want `_compute_display_name` definition(s)
        // we need the symbol of the model/ then we run get member symbol
        // to do that, we need the expr, match it to attribute, get the value, get its evals
        // with those evals, we run get_member_symbol on `_compute_display_name`
        let crate::core::evaluation::ExprOrIdent::Expr(Expr::Attribute(attr_expr)) = expr else {
            return;
        };
        let (analyse_ast_result, _range) = AstUtils::get_symbol_from_expr(session, file_symbol, &crate::core::evaluation::ExprOrIdent::Expr(&attr_expr.value), offset as u32);
        let eval_ptrs = analyse_ast_result.evaluations.iter().flat_map(|eval| Symbol::follow_ref(eval.symbol.get_symbol_ptr(), session, &mut None, false, false, None, None)).collect::<Vec<_>>();
        let maybe_module = file_symbol.borrow().find_module();
        let symbols = eval_ptrs.iter().flat_map(|eval_ptr| {
            let Some(symbol) = eval_ptr.upgrade_weak() else {
                return  vec![];
            };
            symbol.borrow().get_member_symbol(session, &S!("_compute_display_name"), maybe_module.clone(), false, false, true, true, false).0
        }).collect::<Vec<_>>();
        for symbol in symbols {
            sources.push(GotoSource {
                source: GotoSourceType::Symbol(symbol.clone()),
                origin_selection_range: None,
            });
        }
    }

    pub fn get_symbols(session: &mut SessionInfo,
        goto_request: GotoRequest,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Vec<GotoSource> {
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let file_info_ast_clone = file_info.borrow().file_info_ast.clone();
        let file_info_ast_ref = file_info_ast_clone.borrow();
        let (analyse_ast_result, _range, expr, call_expr) = AstUtils::get_symbols(session, &file_info_ast_ref, file_symbol, offset as u32);
        if analyse_ast_result.evaluations.is_empty() {
            return vec![];
        }
        let mut definition_sources = vec![];
        let mut evaluations = analyse_ast_result.evaluations.clone();
        // Filter out magic fields
        let mut dislay_name_found = false;
        evaluations.retain(|eval| {
            // Filter out, variables, whose parents are a class, whose name is one of the magic fields, and have the same range as their parent
            let eval_sym = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let Some(eval_sym) = eval_sym.upgrade_weak() else { return true; };
            if !MAGIC_FIELDS.contains(&eval_sym.borrow().name().as_str()) || eval_sym.borrow().typ() != SymType::VARIABLE || !eval_sym.borrow().is_field(session) {
                return true;
            }
            if eval_sym.borrow().name() == "display_name" {
                dislay_name_found = true;
            }
            let Some(parent_sym) = eval_sym.borrow().parent().and_then(|parent| parent.upgrade()) else { return true; };
            if parent_sym.borrow().typ() != SymType::CLASS {
                return true;
            }
            eval_sym.borrow().range() != parent_sym.borrow().range()
        });
        if let Some(expr) = expr && dislay_name_found {
            GotoUtils::add_display_name_compute_methods(session, &mut definition_sources, &expr, file_symbol, offset);
        }
        drop(file_info_ast_ref);
        let mut index = 0;
        while index < evaluations.len() {
            let eval = evaluations[index].clone();
            if GotoUtils::check_for_domain_field(session, &eval, file_symbol, &call_expr, offset, &mut definition_sources) ||
              GotoUtils::check_for_compute_string(session, &eval, file_symbol,&call_expr, offset, &mut definition_sources) ||
              GotoUtils::check_for_module_string(session, &eval, file_symbol, &file_info.borrow().uri, &mut definition_sources) ||
              GotoUtils::check_for_model_string(session, &eval, file_symbol, &mut definition_sources) ||
              GotoUtils::check_for_xml_id_string(session, &eval, file_symbol, &mut definition_sources) {
                index += 1;
                continue;
            }
            if matches!(eval.value, Some(EvaluationValue::CONSTANT(_))) {
                // Skip go to definition on literals
                index += 1;
                continue;
            }
            if matches!(goto_request, GotoRequest::Definition) {
                let eval_ptr = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
                let end_symbols = Symbol::follow_imported_ref(&eval_ptr, session, &mut None);
                for end_symbol in end_symbols.iter() {
                    if let Some(symbol) = end_symbol.upgrade_weak() {
                        definition_sources.push(GotoSource{
                            source: GotoSourceType::Symbol(symbol.clone()),
                            origin_selection_range: None
                        });
                    }
                }
            } else {
                let Some(symbol) = eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None).weak.upgrade() else {
                    index += 1;
                    continue;
                };
                definition_sources.push(GotoSource{
                    source: GotoSourceType::Symbol(symbol.clone()),
                    origin_selection_range: None
                });
            }
            index += 1;
        }
        definition_sources
    }

    pub fn goto_source_to_location(session: &mut SessionInfo, def: &GotoSource) -> Vec<LocationLink> {
        let mut res = vec![];
        match &def.source {
            GotoSourceType::Symbol(symbol) => {
                if let Some(file_symbol) = symbol.borrow().get_file().and_then(|file_sym_weak| file_sym_weak.upgrade()){
                    let paths = file_symbol.borrow().paths();
                    for path in paths.iter() {
                        let full_path = match file_symbol.borrow().typ() {
                            SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", file_symbol.borrow().as_package().i_ext())).sanitize(),
                            _ => path.clone()
                        };
                        let symbol_range = if symbol.borrow().has_range() {
                            session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &full_path, &symbol.borrow().range())
                        } else {
                            Range::default()
                        };
                        res.push(LocationLink{
                            origin_selection_range: def.origin_selection_range.clone(),
                            target_uri: FileMgr::pathname2uri(&full_path),
                            target_selection_range: symbol_range,
                            target_range: symbol_range,
                        });
                    }
                }
            },
            GotoSourceType::Module(module) => {
                let path = PathBuf::from(module.borrow().paths()[0].clone()).join("__manifest__.py").sanitize();
                res.push(LocationLink{
                    origin_selection_range: None,
                    target_uri: FileMgr::pathname2uri(&path),
                    target_selection_range: Range::default(),
                    target_range: Range::default(),
                });
            },
            GotoSourceType::OdooData(xml_id) => {
                let file = xml_id.get_file_symbol();
                if let Some(file) = file {
                    if let Some(file) = file.upgrade() {
                        let range = session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &file.borrow().paths()[0], &xml_id.get_range());
                        res.push(LocationLink {
                            origin_selection_range: def.origin_selection_range.clone(),
                            target_uri: FileMgr::pathname2uri(&file.borrow().paths()[0]),
                            target_range: range,
                            target_selection_range: range });
                    }
                }
            }
        }
        res
    }
}