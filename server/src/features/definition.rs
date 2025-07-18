use lsp_types::{GotoDefinitionResponse, Location, Range};
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::TextSize;
use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};

use crate::constants::SymType;
use crate::core::evaluation::{Evaluation, EvaluationValue};
use crate::core::file_mgr::{FileInfo, FileMgr};
use crate::core::symbols::symbol::Symbol;
use crate::features::ast_utils::AstUtils;
use crate::features::features_utils::FeaturesUtils;
use crate::features::xml_ast_utils::XmlAstUtils;
use crate::oyarn;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;

pub struct DefinitionFeature {}

impl DefinitionFeature {

    fn check_for_domain_field(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, call_expr: &Option<ExprCall>, offset: usize, links: &mut Vec<Location>) -> bool {
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
        let string_domain_fields = FeaturesUtils::find_argument_symbols(
            session, Symbol::get_scope_symbol(file_symbol.clone(), offset as u32, false), file_symbol.borrow().find_module(), &field_name, call_expr, offset, field_range
        );
        string_domain_fields.iter().for_each(|field|{
            if let Some(file_sym) = field.borrow().get_file().and_then(|file_sym_weak| file_sym_weak.upgrade()){
                let path = file_sym.borrow().paths()[0].clone();
                let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, &field.borrow().range());
                links.push(Location{uri: FileMgr::pathname2uri(&path), range});
            }
        });
        string_domain_fields.len() > 0
    }

    fn check_for_model_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, links: &mut Vec<Location>) -> bool {
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
        for class_symbol_rc in model.borrow().get_symbols(session, from_module.clone()){
            let class_symbol = class_symbol_rc.borrow();
            if let Some(model_file_sym) = class_symbol.get_file().and_then(|model_file_sym_weak| model_file_sym_weak.upgrade()){
                let path = model_file_sym.borrow().paths()[0].clone();
                let range = session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, &class_symbol.range());
                model_found = true;
                links.push(Location{uri: FileMgr::pathname2uri(&path), range});
            }
        }
        model_found
    }

    fn check_for_compute_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, call_expr: &Option<ExprCall>, offset: usize, links: &mut Vec<Location>) -> bool {
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
                links.push(Location{uri: FileMgr::pathname2uri(&path), range});
            }
        });
        compute_symbols.len() > 0
    }

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let (analyse_ast_result, _range, call_expr) = AstUtils::get_symbols(session, file_symbol, file_info, offset as u32);
        if analyse_ast_result.evaluations.is_empty() {
            return None;
        }
        let mut links = vec![];
        let mut evaluations = analyse_ast_result.evaluations.clone();
        let mut index = 0;
        while index < evaluations.len() {
            let eval = evaluations[index].clone();
            if DefinitionFeature::check_for_domain_field(session, &eval, file_symbol, &call_expr, offset, &mut links) ||
              DefinitionFeature::check_for_compute_string(session, &eval, file_symbol,&call_expr, offset, &mut links) ||
              DefinitionFeature::check_for_model_string(session, &eval, file_symbol, &mut links){
                index += 1;
                continue;
            }
            let Some(symbol) = eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None).weak.upgrade() else {
                index += 1;
                continue;
            };
            if let Some(file) = symbol.borrow().get_file() {
                //if the symbol is at the given offset, let's take the next evaluation instead
                if Rc::ptr_eq(&file.upgrade().unwrap(), file_symbol) && symbol.borrow().has_range() && symbol.borrow().range().contains(TextSize::new(offset as u32)) {
                    evaluations.remove(index);
                    let symbol = symbol.borrow();
                    let sym_eval = symbol.evaluations();
                    if let Some(sym_eval) = sym_eval {
                        evaluations = [evaluations.clone(), sym_eval.clone()].concat();
                    }
                    continue;
                }
                for path in file.upgrade().unwrap().borrow().paths().iter() {
                    let full_path = match file.upgrade().unwrap().borrow().typ() {
                        SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", file.upgrade().unwrap().borrow().as_package().i_ext())).sanitize(),
                        _ => path.clone()
                    };
                    let range = match symbol.borrow().typ() {
                        SymType::PACKAGE(_) | SymType::FILE | SymType::NAMESPACE | SymType::DISK_DIR => Range::default(),
                        _ => session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &full_path, &symbol.borrow().range()),
                    };
                    links.push(Location{uri: FileMgr::pathname2uri(&full_path), range});
                }
            }
            index += 1;
        }
        Some(GotoDefinitionResponse::Array(links))
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
            let (symbols, _range) = XmlAstUtils::get_symbols(session, file_symbol, root, offset);
            if symbols.is_empty() {
                return None;
            }
            
        }
        let mut links = vec![];
        let mut evaluations = analyse_ast_result.evaluations.clone();
        let mut index = 0;
        while index < evaluations.len() {
            let eval = evaluations[index].clone();
            if DefinitionFeature::check_for_domain_field(session, &eval, file_symbol, &call_expr, offset, &mut links) ||
              DefinitionFeature::check_for_compute_string(session, &eval, file_symbol,&call_expr, offset, &mut links) ||
              DefinitionFeature::check_for_model_string(session, &eval, file_symbol, &mut links){
                index += 1;
                continue;
            }
            let Some(symbol) = eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None).weak.upgrade() else {
                index += 1;
                continue;
            };
            if let Some(file) = symbol.borrow().get_file() {
                //if the symbol is at the given offset, let's take the next evaluation instead
                if Rc::ptr_eq(&file.upgrade().unwrap(), file_symbol) && symbol.borrow().has_range() && symbol.borrow().range().contains(TextSize::new(offset as u32)) {
                    evaluations.remove(index);
                    let symbol = symbol.borrow();
                    let sym_eval = symbol.evaluations();
                    if let Some(sym_eval) = sym_eval {
                        evaluations = [evaluations.clone(), sym_eval.clone()].concat();
                    }
                    continue;
                }
                for path in file.upgrade().unwrap().borrow().paths().iter() {
                    let full_path = match file.upgrade().unwrap().borrow().typ() {
                        SymType::PACKAGE(_) => PathBuf::from(path).join(format!("__init__.py{}", file.upgrade().unwrap().borrow().as_package().i_ext())).sanitize(),
                        _ => path.clone()
                    };
                    let range = match symbol.borrow().typ() {
                        SymType::PACKAGE(_) | SymType::FILE | SymType::NAMESPACE | SymType::DISK_DIR => Range::default(),
                        _ => session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &full_path, &symbol.borrow().range()),
                    };
                    links.push(Location{uri: FileMgr::pathname2uri(&full_path), range});
                }
            }
            index += 1;
        }
        Some(GotoDefinitionResponse::Array(links))
    }

    pub fn get_location_csv(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        None
    }

}
