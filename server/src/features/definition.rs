use lsp_types::{GotoDefinitionResponse, LocationLink, Range};
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::TextSize;
use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};

use crate::constants::SymType;
use crate::core::evaluation::{Evaluation, EvaluationValue, ExprOrIdent};
use crate::core::file_mgr::{FileInfo, FileMgr};
use crate::core::odoo::SyncOdoo;
use crate::core::python_odoo_builder::MAGIC_FIELDS;
use crate::core::symbols::symbol::Symbol;
use crate::features::ast_utils::AstUtils;
use crate::features::features_utils::FeaturesUtils;
use crate::features::xml_ast_utils::{XmlAstResult, XmlAstUtils};
use crate::{S, oyarn};
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;

pub struct DefinitionFeature {}

impl DefinitionFeature {

    fn check_for_domain_field(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, call_expr: &Option<ExprCall>, offset: usize, links: &mut Vec<LocationLink>) -> bool {
        let (field_name, field_range) = if let Some(eval_value) = eval.value.as_ref() {
            if let EvaluationValue::CONSTANT(Expr::StringLiteral(expr)) = eval_value {
                (expr.value.to_string(), expr.range)
            } else {
                return false;
            }
        } else {
            return  false;
        };
        let Some(call_expr) = call_expr else { return false };
        let module = file_symbol.borrow().find_module();
        let string_domain_fields = FeaturesUtils::find_argument_symbols(
            session, Symbol::get_scope_symbol(file_symbol.clone(), offset as u32, false), module, &field_name, call_expr, offset, field_range
        );
        string_domain_fields.iter().for_each(|(field, field_range)|{
            if let Some(file_sym) = field.borrow().get_file().and_then(|file_sym_weak| file_sym_weak.upgrade()){
                let path = file_sym.borrow().paths()[0].clone();
                let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, &field.borrow().range());
                links.push(LocationLink{
                    origin_selection_range: Some(session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), &field_range)),
                    target_uri: FileMgr::pathname2uri(&path),
                    target_selection_range: range,
                    target_range: range,
                });
            }
        });
        string_domain_fields.len() > 0
    }

    fn check_for_model_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, links: &mut Vec<LocationLink>) -> bool {
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
            if let Some(model_file_sym) = class_symbol.get_file().and_then(|model_file_sym_weak| model_file_sym_weak.upgrade()){
                let path = model_file_sym.borrow().get_symbol_first_path();
                let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, &class_symbol.range());
                model_found = true;
                links.push(LocationLink{
                    origin_selection_range: eval.range.map(|r| session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), &r)),
                    target_uri: FileMgr::pathname2uri(&path),
                    target_selection_range: range,
                    target_range: range,
                });
            }
        }
        model_found
    }

    fn check_for_xml_id_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, links: &mut Vec<LocationLink>) -> bool {
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
                if let Some(file) = file.upgrade() {
                    let range = session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &file.borrow().paths()[0], &xml_id.get_range());
                    xml_found = true;
                    links.push(LocationLink {
                        origin_selection_range: eval.range.map(|r| session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), &r)),
                        target_uri: FileMgr::pathname2uri(&file.borrow().paths()[0]),
                        target_range: range,
                        target_selection_range: range });
                }
            }
        }
        xml_found
    }

    fn check_for_compute_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, call_expr: &Option<ExprCall>, offset: usize, links: &mut Vec<LocationLink>) -> bool {
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
        let compute_symbols = FeaturesUtils::find_field_symbols(
            session, Symbol::get_scope_symbol(file_symbol.clone(), offset as u32, false), file_symbol.borrow().find_module(), &value, call_expr, &offset
        );
        compute_symbols.iter().for_each(|field|{
            if let Some(file_sym) = field.borrow().get_file().and_then(|file_sym_weak| file_sym_weak.upgrade()){
                let path = file_sym.borrow().paths()[0].clone();
                let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, &field.borrow().range());
                links.push(LocationLink{
                    origin_selection_range: eval.range.map(|r| session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), &r)),
                    target_uri: FileMgr::pathname2uri(&path),
                    target_selection_range: range,
                    target_range: range,
                });
            }
        });
        compute_symbols.len() > 0
    }

    pub fn add_display_name_compute_methods(session: &mut SessionInfo, links: &mut Vec<LocationLink>, expr: &ExprOrIdent, file_symbol: &Rc<RefCell<Symbol>>, offset: usize) {
        // now we want `_compute_display_name` definition(s)
        // we need the symbol of the model/ then we run get member symbol
        // to do that, we need the expr, match it to attribute, get the value, get its evals
        // with those evals, we run get_member_symbol on `_compute_display_name`
        let crate::core::evaluation::ExprOrIdent::Expr(Expr::Attribute(attr_expr)) = expr else {
            return;
        };
        let (analyse_ast_result, _range) = AstUtils::get_symbol_from_expr(session, file_symbol, &crate::core::evaluation::ExprOrIdent::Expr(&attr_expr.value), offset as u32);
        let eval_ptrs = analyse_ast_result.evaluations.iter().flat_map(|eval| Symbol::follow_ref(eval.symbol.get_symbol_ptr(), session, &mut None, false, false, None)).collect::<Vec<_>>();
        let maybe_module = file_symbol.borrow().find_module();
        let symbols = eval_ptrs.iter().flat_map(|eval_ptr| {
            let Some(symbol) = eval_ptr.upgrade_weak() else {
                return  vec![];
            };
            symbol.borrow().get_member_symbol(session, &S!("_compute_display_name"), maybe_module.clone(), false, false, true, false).0
        }).collect::<Vec<_>>();
        for symbol in symbols {
            if let Some(file) = symbol.borrow().get_file() {
                for path in file.upgrade().unwrap().borrow().paths().iter() {
                    let full_path = match file.upgrade().unwrap().borrow().typ() {
                        SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", file.upgrade().unwrap().borrow().as_package().i_ext())).sanitize(),
                        _ => path.clone()
                    };
                    let range = if symbol.borrow().has_range() {
                        if symbol.borrow().range().contains(TextSize::new(offset as u32)) {
                            continue; //skip if we are already on the definition
                        }
                        session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &full_path, &symbol.borrow().range())
                    } else {
                        Range::default()
                    };
                    links.push(LocationLink{
                        origin_selection_range: None,
                        target_uri: FileMgr::pathname2uri(&full_path),
                        target_selection_range: range,
                        target_range: range,
                    });
                }
            }
        }
    }

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let file_info_ast_clone = file_info.borrow().file_info_ast.clone();
        let file_info_ast_ref = file_info_ast_clone.borrow();
        let (analyse_ast_result, _range, expr, call_expr) = AstUtils::get_symbols(session, &file_info_ast_ref, file_symbol, offset as u32);
        if analyse_ast_result.evaluations.is_empty() {
            return None;
        }
        let Some(expr) = expr else  {
            return None; // Unreachable anyway
        };
        let mut links = vec![];
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
        if dislay_name_found {
            DefinitionFeature::add_display_name_compute_methods(session, &mut links, &expr, file_symbol, offset);
        }
        drop(expr);
        drop(file_info_ast_ref);
        let mut index = 0;
        while index < evaluations.len() {
            let eval = evaluations[index].clone();
            if DefinitionFeature::check_for_domain_field(session, &eval, file_symbol, &call_expr, offset, &mut links) ||
              DefinitionFeature::check_for_compute_string(session, &eval, file_symbol,&call_expr, offset, &mut links) ||
              DefinitionFeature::check_for_model_string(session, &eval, file_symbol, &mut links) ||
              DefinitionFeature::check_for_xml_id_string(session, &eval, file_symbol, &mut links) {
                index += 1;
                continue;
            }
            let Some(symbol) = eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None).weak.upgrade() else {
                index += 1;
                continue;
            };
            if let Some(file) = symbol.borrow().get_file() {
                for path in file.upgrade().unwrap().borrow().paths().iter() {
                    let full_path = match file.upgrade().unwrap().borrow().typ() {
                        SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", file.upgrade().unwrap().borrow().as_package().i_ext())).sanitize(),
                        _ => path.clone()
                    };
                    let range = if symbol.borrow().has_range() {
                        session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &full_path, &symbol.borrow().range())
                    } else {
                        Range::default()
                    };
                    links.push(LocationLink{
                        origin_selection_range: None,
                        target_uri: FileMgr::pathname2uri(&full_path),
                        target_selection_range: range,
                        target_range: range,
                    });
                }
            }
            index += 1;
        }
        Some(GotoDefinitionResponse::Link(links))
    }

    pub fn get_location_xml(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let data = file_info.borrow().file_info_ast.borrow().text_rope.as_ref().unwrap().to_string();
        let document = roxmltree::Document::parse(&data);
        if let Ok(document) = document {
            let root = document.root_element();
            let (symbols, link_range) = XmlAstUtils::get_symbols(session, file_symbol, root, offset, true);
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
                                let range = if s.borrow().has_range() {
                                    session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &full_path, &s.borrow().range())
                                } else {
                                    Range::default()
                                };
                                let link_range = if link_range.is_some() {
                                    Some(session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), link_range.as_ref().unwrap()))
                                } else {
                                    None
                                };
                                links.push(LocationLink{
                                    origin_selection_range: link_range,
                                    target_uri: FileMgr::pathname2uri(&full_path),
                                    target_range: range,
                                    target_selection_range: range
                                });
                            }
                        }
                    },
                    XmlAstResult::XML_DATA(xml_file_symbol, range) => {
                        let file = xml_file_symbol.borrow().get_file(); //in case of XML_DATA coming from a python class
                        if let Some(file) = file {
                            if let Some(file) = file.upgrade() {
                                for path in file.borrow().paths().iter() {
                                    let full_path = match file.borrow().typ() {
                                        SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", file.borrow().as_package().i_ext())).sanitize(),
                                        _ => path.clone()
                                    };
                                    let range = match file.borrow().typ() {
                                        SymType::PACKAGE(_) | SymType::FILE | SymType::NAMESPACE | SymType::DISK_DIR => Range::default(),
                                        _ => session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &full_path, &range),
                                    };
                                    let link_range = if link_range.is_some() {
                                        Some(session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, file_symbol.borrow().paths().first().as_ref().unwrap(), link_range.as_ref().unwrap()))
                                    } else {
                                        None
                                    };
                                    links.push(LocationLink{
                                        origin_selection_range: link_range,
                                        target_uri: FileMgr::pathname2uri(&full_path),
                                        target_range: range,
                                        target_selection_range: range
                                    });
                                }
                            }
                        }
                    }
                }
            }
            return Some(GotoDefinitionResponse::Link(links));
        }
        None
    }

    pub fn get_location_csv(_session: &mut SessionInfo,
        _file_symbol: &Rc<RefCell<Symbol>>,
        _file_info: &Rc<RefCell<FileInfo>>,
        _line: u32,
        _character: u32
    ) -> Option<GotoDefinitionResponse> {
        None
    }

}
