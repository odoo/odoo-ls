use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};
use ruff_text_size::{TextRange, TextSize};
use lsp_types::{GotoDefinitionResponse, Location, Range};

use crate::constants::SymType;
use crate::core::evaluation::{AnalyzeAstResult, Evaluation};
use crate::core::file_mgr::{FileMgr, FileInfo};
use crate::core::symbols::symbol::Symbol;
use crate::threads::SessionInfo;
use crate::features::ast_utils::AstUtils;
use crate::utils::PathSanitizer as _;



pub struct DefinitionFeature {}

impl DefinitionFeature {

    pub fn get_location(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<GotoDefinitionResponse> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let (analyse_ast_result, _range): (AnalyzeAstResult, Option<TextRange>) = AstUtils::get_symbols(session, file_symbol, file_info, offset as u32);
        if analyse_ast_result.evaluations.is_empty() {
            return None;
        }
        let mut links = vec![];
        let mut evaluations = analyse_ast_result.evaluations.clone();
        let mut index = 0;
        while index < evaluations.len() {
            let eval = evaluations[index].clone();
            if DefinitionFeature::check_for_model_string(session, &eval, file_symbol, &mut links){
                index += 1;
                continue;
            }
            let sym_ref = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let loc_sym = sym_ref.weak.upgrade();
            if loc_sym.is_none() {
                index += 1;
                continue;
            }
            let symbol = loc_sym.unwrap();
            let file = symbol.borrow().get_file();
            if let Some(file) = file {
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
                        SymType::PACKAGE(_) | SymType::FILE => Range::default(),
                        _ => session.sync_odoo.get_file_mgr().borrow_mut().text_range_to_range(session, &full_path, &symbol.borrow().range()),
                    };
                    links.push(Location{uri: FileMgr::pathname2uri(&full_path), range});
                }
            }
            index += 1;
        }
        Some(GotoDefinitionResponse::Array(links))
    }

    fn check_for_model_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: &Rc<RefCell<Symbol>>, links: &mut Vec<Location>) -> bool {
        let mut model_found = false;
        if let Some(eval_value) = eval.value.as_ref() {
            if let crate::core::evaluation::EvaluationValue::CONSTANT(ruff_python_ast::Expr::StringLiteral(expr)) = eval_value {
                let str = expr.value.to_string();
                let model = session.sync_odoo.models.get(&str).cloned();
                if let Some(model) = model {
                    let from_module = file_symbol.borrow().find_module();
                    for class_symbol_rc in model.borrow().get_symbols(session, from_module.clone()){
                        let class_symbol = class_symbol_rc.borrow();
                        if let Some(model_file_sym_weak) = class_symbol.get_file(){
                            if let Some(model_file_sym) = model_file_sym_weak.upgrade(){
                                let path = model_file_sym.borrow().paths()[0].clone();
                                let range = session.sync_odoo.get_file_mgr().borrow_mut().text_range_to_range(session, &path, &class_symbol.range());
                                model_found = true;
                                links.push(Location{uri: FileMgr::pathname2uri(&path), range});
                            }
                        }
                    }
                }
            }
        }
        model_found
    }
}