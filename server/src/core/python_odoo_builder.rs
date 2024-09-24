use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use lsp_types::notification::ShowMessage;
use lsp_types::MessageType;
use ruff_python_ast::Expr;
use lsp_types::{Diagnostic, ShowMessageParams, notification::Notification};
use tracing::{error, info};

use crate::constants::{BuildStatus, BuildSteps, SymType, DEBUG_ODOO_BUILDER};
use crate::core::model::{Model, ModelData};
use crate::core::symbols::symbol::Symbol;
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;
use crate::S;

use super::evaluation::EvaluationValue;
use super::odoo::SyncOdoo;

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

    pub fn load_odoo_content(&mut self, session: &mut SessionInfo) {
        let mut symbol = self.symbol.borrow_mut();
        if symbol.build_status(BuildSteps::ODOO) != BuildStatus::PENDING {
            return;
        }
        let mut path = symbol.paths()[0].clone();
        if vec![SymType::NAMESPACE, SymType::ROOT, SymType::COMPILED].contains(&symbol.typ()) {
            return;
        } else if symbol.typ() == SymType::PACKAGE {
            path = PathBuf::from(path).join("__init__").with_extension(S!("py") + symbol.as_package().i_ext().as_str()).sanitize();
        }
        symbol.set_build_status(BuildSteps::ODOO, BuildStatus::IN_PROGRESS);
        symbol.set_build_status(BuildSteps::VALIDATION, BuildStatus::PENDING);
        if DEBUG_ODOO_BUILDER {
            info!("Loading Odoo content for: {}", path);
        }
        let file_info = session.sync_odoo.get_file_mgr().borrow_mut().get_file_info(&path).expect("File not found in cache").clone();
        if file_info.borrow().ast.is_none() {
            symbol.set_build_status(BuildSteps::ODOO, BuildStatus::DONE);
            return;
        }
        drop(symbol);
        self._load(session);
        file_info.borrow_mut().replace_diagnostics(BuildSteps::ODOO, self.diagnostics.clone());
        session.sync_odoo.add_to_validations(self.symbol.clone());
        let mut symbol = self.symbol.borrow_mut();
        symbol.set_build_status(BuildSteps::ODOO, BuildStatus::DONE);
    }

    fn _load(&mut self, session: &mut SessionInfo) {
        let symbol = self.symbol.borrow_mut();
        let iterator = symbol.get_sorted_symbols();
        drop(symbol);
        for sym in iterator {
            let mut s_to_build = sym.borrow_mut();
            if s_to_build.typ() != SymType::CLASS {
                continue;
            }
            if !self.test_symbol_is_model(session, &sym, &mut s_to_build) {
                continue;
            }
            self._load_class_inherit(session, &mut s_to_build);
            self._load_class_name(session, &mut s_to_build);
            if s_to_build.as_class_sym()._model.is_none() {
                continue;
            }
            self._load_class_inherits(session, &mut s_to_build);
            self._load_class_attributes(session, &mut s_to_build);
            let model = session.sync_odoo.models.get_mut(&s_to_build.as_class_sym()._model.as_ref().unwrap().name).cloned();
            if model.is_none() {
                let model = Model::new(s_to_build.as_class_sym()._model.as_ref().unwrap().name.clone(), sym.clone());
                session.sync_odoo.models.insert(s_to_build.as_class_sym()._model.as_ref().unwrap().name.clone(), Rc::new(RefCell::new(model)));
            } else {
                let model = model.unwrap();
                drop(s_to_build);
                model.borrow_mut().add_symbol(session, sym.clone());
            }
        }
    }

    fn _load_class_inherit(&mut self, session: &mut SessionInfo, symbol: &mut Symbol) {
        let module = symbol.find_module();
        let _inherit = symbol.get_symbol(&(vec![], vec![S!("_inherit")]), u32::MAX);
        if let Some(_inherit) = _inherit.last() {
            if _inherit.borrow().evaluations().is_none() || _inherit.borrow().evaluations().unwrap().len() == 0 {
                error!("wrong _inherit structure");
            }
            for eval in _inherit.borrow().evaluations().unwrap().iter() {
                let eval = eval.follow_ref_and_get_value(session, &mut None, &mut self.diagnostics);
                if let Some(eval) = eval.as_ref() {
                    match eval {
                        EvaluationValue::CONSTANT(Expr::StringLiteral(s)) => {
                            symbol.as_class_sym_mut()._model.as_mut().unwrap().inherit = vec![S!(s.value.to_str())];
                        },
                        EvaluationValue::LIST(l) => {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    symbol.as_class_sym_mut()._model.as_mut().unwrap().inherit.push(S!(s.value.to_str()));
                                }
                            }
                        },
                        EvaluationValue::TUPLE(l) => {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    symbol.as_class_sym_mut()._model.as_mut().unwrap().inherit.push(S!(s.value.to_str()));
                                }
                            }
                        },
                        _ => {
                            error!("wrong _inherit value");
                        }
                    }
                } else {
                    error!("wrong _inherit value");
                }
            }
        }
    }

    fn _evaluate_name(&mut self, session: &mut SessionInfo, symbol: &mut Symbol) -> String {
        let _name = symbol.get_symbol(&(vec![], vec![S!("_name")]), u32::MAX);
        if let Some(_name) = _name.last() {
            for eval in _name.borrow().evaluations().unwrap().iter() {
                let eval = eval.follow_ref_and_get_value(session, &mut None, &mut self.diagnostics);
                if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = eval {
                    return S!(s.value.to_str());
                }
            }
            error!("unable to parse model name");
            return "".to_string();
        }
        if let Some(inherit_name) = symbol.as_class_sym_mut()._model.as_ref().unwrap().inherit.first() {
            return inherit_name.clone();
        }
        symbol.name().clone()
    }

    fn _load_class_name(&mut self, session: &mut SessionInfo, symbol: &mut Symbol) {
        symbol.as_class_sym_mut()._model.as_mut().unwrap().name = self._evaluate_name(session, symbol);
        if symbol.as_class_sym()._model.as_ref().unwrap().name.is_empty() {
            symbol.as_class_sym_mut()._model = None;
            return;
        }
        if symbol.as_class_sym()._model.as_ref().unwrap().name != S!("base") {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().inherit.push(S!("base"));
        }
    }

    fn _load_class_inherits(&mut self, session: &mut SessionInfo, symbol: &mut Symbol) {
        let _inherits = symbol.get_symbol(&(vec![], vec![S!("_inherits")]), u32::MAX);
        if let Some(_inherits) = _inherits.last() {
            for eval in _inherits.borrow().evaluations().unwrap().iter() {
                let eval = eval.follow_ref_and_get_value(session, &mut None, &mut self.diagnostics);
                symbol.as_class_sym_mut()._model.as_mut().unwrap().inherits.clear();
                if let Some(EvaluationValue::DICT(d)) = eval {
                    for (k, v) in d.iter() {
                        if let (Expr::StringLiteral(k), Expr::StringLiteral(v)) = (k,v) {
                            symbol.as_class_sym_mut()._model.as_mut().unwrap().inherits.push((S!(k.value.to_str()), S!(v.value.to_str())));
                        } else {
                            error!("wrong _inherits value");
                        }
                    }
                } else {
                    error!("wrong _inherits value");
                }
            }
        }
    }

    fn _get_attribute(&mut self, session: &mut SessionInfo, loc_sym: &mut Symbol, attr: &String) -> Option<EvaluationValue> {
        let attr_sym = loc_sym.get_member_symbol(session, attr, None, true, false, &mut self.diagnostics);
        if attr_sym.len() == 0 {
            return None;
        }
        let attr_sym = &attr_sym[0];
        for eval in attr_sym.borrow().evaluations().unwrap().iter() {
            let eval = eval.follow_ref_and_get_value(session, &mut None, &mut self.diagnostics);
            return eval;
        }
        None
    }

    fn _load_class_attributes(&mut self, session: &mut SessionInfo, symbol: &mut Symbol) {
        let descr = self._get_attribute(session, symbol, &"_description".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = descr {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().description = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().description = symbol.as_class_sym()._model.as_ref().unwrap().name.clone();
        }
        let auto = self._get_attribute(session, symbol, &"_auto".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = auto {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().auto = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().auto = false;
        }
        let log_access = self._get_attribute(session, symbol, &"_log_access".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = log_access {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().log_access = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().log_access = symbol.as_class_sym()._model.as_ref().unwrap().auto;
        }
        let table = self._get_attribute(session, symbol, &"_table".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = table {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().table = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().table = symbol.as_class_sym()._model.as_ref().unwrap().name.clone().replace(".", "_");
        }
        let sequence = self._get_attribute(session, symbol, &"_sequence".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = sequence {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().sequence = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().sequence = symbol.as_class_sym()._model.as_ref().unwrap().table.clone() + "_id_seq";
        }
        let is_abstract = self._get_attribute(session, symbol, &"_abstract".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = is_abstract {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().is_abstract = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().is_abstract = true;
        }
        let transient = self._get_attribute(session, symbol, &"_transient".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = transient {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().transient = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().transient = false;
        }
        let rec_name = self._get_attribute(session, symbol, &"_rec_name".to_string());
        //TODO check that rec_name is a field
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = rec_name {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().rec_name = Some(S!(s.value.to_str()));
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().rec_name = Some(S!("name")); //TODO if name is not on model, take 'id'
        }
        let _check_company_auto = self._get_attribute(session, symbol, &"_check_company_auto".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = _check_company_auto {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().check_company_auto = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().check_company_auto = false;
        }
        let parent_name = self._get_attribute(session, symbol, &"_parent_name".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = parent_name {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().parent_name = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().parent_name = S!("parent_id");
        }
        let parent_store = self._get_attribute(session, symbol, &"_parent_store".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = parent_store {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().parent_store = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().parent_store = false;
        }
        let active_name = self._get_attribute(session, symbol, &"_active_name".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = active_name {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().active_name = Some(S!(s.value.to_str()));
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().active_name = None;
        }
        let data_name = self._get_attribute(session, symbol, &"_data_name".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = data_name {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().data_name = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().data_name = S!("date");
        }
        let fold_name = self._get_attribute(session, symbol, &"_fold_name".to_string());
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = fold_name {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().fold_name = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().fold_name = S!("fold");
        }
    }

    /* true if the symbol inherit from odoo.models.BaseModel. symbol must be the data of rc_symbol and must be a Class */
    fn test_symbol_is_model(&mut self, session: &mut SessionInfo, rc_symbol: &Rc<RefCell<Symbol>>, symbol: &mut Symbol) -> bool {
        let symbol_tree = symbol.get_tree();
        if symbol_tree.0.len() == 2 && symbol_tree.1.len() == 1 && symbol_tree.0[0] == "odoo" && symbol_tree.0[1] == "models" &&
            (symbol_tree.1[0] == "BaseModel" || symbol_tree.1[0] == "Model" || symbol_tree.1[0] == "TransientModel") {
            //we don't want to compare these classes with themselves
            return false;
        } else {
            let base_model = session.sync_odoo.get_symbol(&(vec![S!("odoo"), S!("models")], vec![S!("BaseModel")]), u32::MAX);
            let model = session.sync_odoo.get_symbol(&(vec![S!("odoo"), S!("models")], vec![S!("Model")]), u32::MAX);
            let transient = session.sync_odoo.get_symbol(&(vec![S!("odoo"), S!("models")], vec![S!("TransientModel")]), u32::MAX);
            if base_model.is_empty() || model.is_empty() || transient.is_empty() {
                session.send_notification(ShowMessage::METHOD, ShowMessageParams{
                    typ: MessageType::ERROR,
                    message: "Odoo base models are not found. OdooLS will be unable to generate valid diagnostics".to_string()
                });
                return false;
            }
            let base_model = base_model[0].clone();
            let model = model[0].clone();
            let transient = transient[0].clone();
            if Rc::ptr_eq(rc_symbol, &base_model) ||
                Rc::ptr_eq(rc_symbol, &model) ||
                Rc::ptr_eq(rc_symbol, &transient) {
                return false;
            }
            if !symbol.as_class_sym().inherits(&base_model, &mut None) {
                return false;
            }
        }
        symbol.as_class_sym_mut()._model = Some(ModelData::new());
        let register = symbol.get_symbol(&(vec![], vec![S!("_register")]), u32::MAX);
        if let Some(register) = register.last() {
            let loc_register = register.borrow();
            let register_evals = &loc_register.evaluations().unwrap();
            if register_evals.len() == 1 { //we don't handle multiple values
                let eval = &register_evals[0];
                let value = eval.follow_ref_and_get_value(session, &mut None, &mut self.diagnostics);
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