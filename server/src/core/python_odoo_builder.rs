use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use ruff_python_ast::Expr;
use tower_lsp::lsp_types::Diagnostic;

use crate::constants::{BuildSteps, BuildStatus, SymType, DEBUG_ODOO_BUILDER};
use crate::core::model::{Model, ModelData};
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::S;

use super::evaluation::EvaluationValue;

pub struct PythonOdooBuilder {
    symbol: Rc<RefCell<Symbol>>,
    diagnostics: Vec<Diagnostic>,
}

impl PythonOdooBuilder {

    pub fn new(symbol: Rc<RefCell<Symbol>>) -> PythonOdooBuilder {
        PythonOdooBuilder {
            symbol: symbol,
            diagnostics: vec![]
        }
    }

    pub fn load_odoo_content(&mut self, odoo: &mut SyncOdoo) {
        let mut symbol = self.symbol.borrow_mut();
        if symbol.odoo_status != BuildStatus::PENDING {
            return;
        }
        let mut path = symbol.paths[0].clone();
        if vec![SymType::NAMESPACE, SymType::ROOT, SymType::COMPILED].contains(&symbol.sym_type) {
            return;
        } else if symbol.sym_type == SymType::PACKAGE {
            path = PathBuf::from(path).join("__init__").with_extension(S!("py") + symbol.i_ext.as_str()).to_str().unwrap().to_string();
        }
        symbol.odoo_status = BuildStatus::IN_PROGRESS;
        symbol.validation_status = BuildStatus::PENDING;
        if DEBUG_ODOO_BUILDER {
            println!("Loading Odoo content for: {}", path);
        }
        let file_info = odoo.get_file_mgr().borrow_mut().get_file_info(&path);
        if file_info.borrow().ast.is_none() {
            symbol.odoo_status = BuildStatus::DONE;
            return;
        }
        drop(symbol);
        self._load(odoo);
        file_info.borrow_mut().replace_diagnostics(BuildSteps::ODOO, self.diagnostics.clone());
        odoo.add_to_validations(self.symbol.clone());
        let mut symbol = self.symbol.borrow_mut();
        symbol.odoo_status = BuildStatus::DONE;
    }

    fn _load(&mut self, odoo: &mut SyncOdoo) {
        let symbol = self.symbol.borrow_mut();
        let iterator = symbol.get_sorted_symbols();
        drop(symbol);
        for sym in iterator {
            let mut s_to_build = sym.borrow_mut();
            if s_to_build.sym_type != SymType::CLASS {
                continue;
            }
            if !self.test_symbol_is_model(odoo, &sym, &mut s_to_build) {
                continue;
            }
            self._load_class_inherit(odoo, &mut s_to_build);
            self._load_class_name(odoo, &mut s_to_build);
            if s_to_build._model.is_none() {
                continue;
            }
            self._load_class_inherits(odoo, &mut s_to_build);
            self._load_class_attributes(odoo, &mut s_to_build);
            let model = odoo.models.get_mut(&s_to_build._model.as_ref().unwrap().name);
            if model.is_none() {
                let model = Model::new(s_to_build._model.as_ref().unwrap().name.clone(), sym.clone());
                odoo.models.insert(s_to_build._model.as_ref().unwrap().name.clone(), Rc::new(RefCell::new(model)));
            } else {
                let model = model.unwrap();
                model.borrow_mut().add_symbol(sym.clone());
            }
        }
    }

    fn _load_class_inherit(&mut self, odoo: &mut SyncOdoo, symbol: &mut Symbol) {
        let _inherit = symbol.get_symbol(&(vec![], vec![S!("_inherit")]));
        if let Some(_inherit) = _inherit {
            if let Some(eval) = _inherit.borrow().evaluation.as_ref() {
                let eval = eval.follow_ref_and_get_value(odoo, &mut None, &mut self.diagnostics);
                if let Some(eval) = eval.as_ref() {
                    match eval {
                        EvaluationValue::CONSTANT(Expr::StringLiteral(s)) => {
                            symbol._model.as_mut().unwrap().inherit = vec![S!(s.value.to_str())];
                        },
                        EvaluationValue::LIST(l) => {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    symbol._model.as_mut().unwrap().inherit.push(S!(s.value.to_str()));
                                }
                            }
                        },
                        EvaluationValue::TUPLE(l) => {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    symbol._model.as_mut().unwrap().inherit.push(S!(s.value.to_str()));
                                }
                            }
                        },
                        _ => {
                            println!("Error: wrong _inherit value");
                        }
                    }
                } else {
                    println!("Error: wrong _inherit value");
                }
            } else {
                println!("Error: wrong _inherit structure");
            }
        }
    }

    fn _evaluate_name(&mut self, odoo: &mut SyncOdoo, symbol: &Symbol) -> String {
        if symbol.name == "ResCompany" {
            println!("here")
        }
        let _name = symbol.get_symbol(&(vec![], vec![S!("_name")]));
        if let Some(_name) = _name {
            if let Some(eval) = _name.borrow().evaluation.as_ref() {
                let eval = eval.follow_ref_and_get_value(odoo, &mut None, &mut self.diagnostics);
                if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = eval {
                    return S!(s.value.to_str());
                }
            }
            println!("unable to parse model name");
            return "".to_string();
        }
        if let Some(inherit_name) = symbol._model.as_ref().unwrap().inherit.first() {
            return inherit_name.clone();
        }
        symbol.name.clone()
    }

    fn _load_class_name(&mut self, odoo: &mut SyncOdoo, symbol: &mut Symbol) {
        symbol._model.as_mut().unwrap().name = self._evaluate_name(odoo, symbol);
        if symbol._model.as_ref().unwrap().name.is_empty() {
            symbol._model = None;
            return;
        }
        if symbol._model.as_ref().unwrap().name != S!("base") {
            symbol._model.as_mut().unwrap().inherit.push(S!("base"));
        }
    }

    fn _load_class_inherits(&mut self, odoo: &mut SyncOdoo, symbol: &mut Symbol) {
        let _inherits = symbol.get_symbol(&(vec![], vec![S!("_inherits")]));
        if let Some(_inherits) = _inherits {
            if let Some(eval) = _inherits.borrow().evaluation.as_ref() {
                let eval = eval.follow_ref_and_get_value(odoo, &mut None, &mut self.diagnostics);
                symbol._model.as_mut().unwrap().inherits.clear();
                if let Some(EvaluationValue::DICT(d)) = eval {
                    for (k, v) in d.iter() {
                        if let (Expr::StringLiteral(k), Expr::StringLiteral(v)) = (k,v) {
                            symbol._model.as_mut().unwrap().inherits.push((S!(k.value.to_str()), S!(v.value.to_str())));
                        } else {
                            println!("Error: wrong _inherits value");
                        }
                    }
                } else {
                    println!("Error: wrong _inherits value");
                }
            }
        }
    }

    fn _get_attribute(&mut self, odoo: &mut SyncOdoo, symbol: &mut Symbol, attr: &String) -> Option<EvaluationValue> {
        let attr_sym = symbol.get_member_symbol(odoo, attr, None, false, true, false, &mut self.diagnostics);
        if attr_sym.len() == 0 {
            return None;
        }
        let attr_sym = attr_sym[0].clone();
        if let Some(eval) = attr_sym.borrow().evaluation.as_ref() {
            let eval = eval.follow_ref_and_get_value(odoo, &mut None, &mut self.diagnostics);
            return eval;
        }
        None
    }

    fn _load_class_attributes(&mut self, odoo: &mut SyncOdoo, symbol: &mut Symbol) {
        let descr = self._get_attribute(odoo, symbol, &"_description".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = descr {
            symbol._model.as_mut().unwrap().description = S!(s.value.to_str());
        } else {
            symbol._model.as_mut().unwrap().description = symbol._model.as_ref().unwrap().name.clone();
        }
        let auto = self._get_attribute(odoo, symbol, &"_auto".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = auto {
            symbol._model.as_mut().unwrap().auto = b.value;
        } else {
            symbol._model.as_mut().unwrap().auto = false;
        }
        let log_access = self._get_attribute(odoo, symbol, &"_log_access".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = log_access {
            symbol._model.as_mut().unwrap().log_access = b.value;
        } else {
            symbol._model.as_mut().unwrap().log_access = symbol._model.as_ref().unwrap().auto;
        }
        let table = self._get_attribute(odoo, symbol, &"_table".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = table {
            symbol._model.as_mut().unwrap().table = S!(s.value.to_str());
        } else {
            symbol._model.as_mut().unwrap().table = symbol._model.as_ref().unwrap().name.clone().replace(".", "_");
        }
        let sequence = self._get_attribute(odoo, symbol, &"_sequence".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = sequence {
            symbol._model.as_mut().unwrap().sequence = S!(s.value.to_str());
        } else {
            symbol._model.as_mut().unwrap().sequence = symbol._model.as_ref().unwrap().table.clone() + "_id_seq";
        }
        let is_abstract = self._get_attribute(odoo, symbol, &"_abstract".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = is_abstract {
            symbol._model.as_mut().unwrap().is_abstract = b.value;
        } else {
            symbol._model.as_mut().unwrap().is_abstract = true;
        }
        let transient = self._get_attribute(odoo, symbol, &"_transient".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = transient {
            symbol._model.as_mut().unwrap().transient = b.value;
        } else {
            symbol._model.as_mut().unwrap().transient = false;
        }
        let rec_name = self._get_attribute(odoo, symbol, &"_rec_name".to_string());
        //TODO check that rec_name is a field
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = rec_name {
            symbol._model.as_mut().unwrap().rec_name = Some(S!(s.value.to_str()));
        } else {
            symbol._model.as_mut().unwrap().rec_name = Some(S!("name")); //TODO if name is not on model, take 'id'
        }
        let _check_company_auto = self._get_attribute(odoo, symbol, &"_check_company_auto".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = _check_company_auto {
            symbol._model.as_mut().unwrap().check_company_auto = b.value;
        } else {
            symbol._model.as_mut().unwrap().check_company_auto = false;
        }
        let parent_name = self._get_attribute(odoo, symbol, &"_parent_name".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = parent_name {
            symbol._model.as_mut().unwrap().parent_name = S!(s.value.to_str());
        } else {
            symbol._model.as_mut().unwrap().parent_name = S!("parent_id");
        }
        let parent_store = self._get_attribute(odoo, symbol, &"_parent_store".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = parent_store {
            symbol._model.as_mut().unwrap().parent_store = b.value;
        } else {
            symbol._model.as_mut().unwrap().parent_store = false;
        }
        let active_name = self._get_attribute(odoo, symbol, &"_active_name".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = active_name {
            symbol._model.as_mut().unwrap().active_name = Some(S!(s.value.to_str()));
        } else {
            symbol._model.as_mut().unwrap().active_name = None;
        }
        let data_name = self._get_attribute(odoo, symbol, &"_data_name".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = data_name {
            symbol._model.as_mut().unwrap().data_name = S!(s.value.to_str());
        } else {
            symbol._model.as_mut().unwrap().data_name = S!("date");
        }
        let fold_name = self._get_attribute(odoo, symbol, &"_fold_name".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = fold_name {
            symbol._model.as_mut().unwrap().fold_name = S!(s.value.to_str());
        } else {
            symbol._model.as_mut().unwrap().fold_name = S!("fold");
        }
    }

    /* true if the symbol inherit from odoo.models.BaseModel */
    fn test_symbol_is_model(&mut self, odoo: &mut SyncOdoo, rc_symbol: &Rc<RefCell<Symbol>>, symbol: &mut Symbol) -> bool {
        if symbol._class.is_none() {
            panic!("Symbol has no class Data. This should not happen");
        }
        if symbol.name == "ResCompany" {
            println!("here");
        }
        let base_model = odoo.get_symbol(&(vec![S!("odoo"), S!("models")], vec![S!("BaseModel")]));
        let model = odoo.get_symbol(&(vec![S!("odoo"), S!("models")], vec![S!("Model")]));
        let transient = odoo.get_symbol(&(vec![S!("odoo"), S!("models")], vec![S!("TransientModel")]));
        if base_model.is_none() || model.is_none() || transient.is_none() {
            panic!("Odoo models not found. This should not happen");
        }
        let base_model = base_model.unwrap();
        let model = model.unwrap();
        let transient = transient.unwrap();
        if Rc::ptr_eq(rc_symbol, &base_model) ||
            Rc::ptr_eq(rc_symbol, &model) ||
            Rc::ptr_eq(rc_symbol, &transient) {
            return false;
        }
        if !symbol._class.as_ref().unwrap().inherits(&base_model, &mut None) {
            return false;
        }
        symbol._model = Some(ModelData::new());
        let register = symbol.get_symbol(&(vec![], vec![S!("_register")]));
        if let Some(register) = register {
            let register_eval = &register.borrow().evaluation;
            if register_eval.is_some() {
                let eval = register_eval.as_ref().unwrap();
                let value = eval.follow_ref_and_get_value(odoo, &mut None, &mut self.diagnostics);
                if value.is_some() {
                    let value = value.unwrap();
                    if let EvaluationValue::CONSTANT(Expr::BooleanLiteral(b)) = value {
                        if !b.value {
                            return false;
                        }
                    }
                }
            }
            return true;
        }
        true
    }
}