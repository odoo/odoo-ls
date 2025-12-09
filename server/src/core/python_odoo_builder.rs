use std::cmp::Ordering;
use std::collections::HashSet;
use std::rc::Rc;
use std::cell::RefCell;
use ruff_python_ast::Expr;
use lsp_types::Diagnostic;
use tracing::error;
use weak_table::PtrWeakHashSet;

use crate::constants::{OYarn, SymType};
use crate::core::model::{Model, ModelData};
use crate::core::symbols::symbol::Symbol;
use crate::core::xml_data::{OdooData, OdooDataRecord};
use crate::threads::SessionInfo;
use crate::utils::compare_semver;
use crate::{oyarn, Sy, S};

use super::evaluation::{ContextValue, Evaluation, EvaluationSymbolPtr, EvaluationValue};

pub const MAGIC_FIELDS: [&str; 6] = [
    "id",
    "display_name",
    "create_uid",
    "create_date",
    "write_uid",
    "write_date"
];

pub struct PythonOdooBuilder {
    symbol: Rc<RefCell<Symbol>>,
}

impl PythonOdooBuilder {

    pub fn new(symbol: Rc<RefCell<Symbol>>) -> PythonOdooBuilder {
        PythonOdooBuilder {
            symbol: symbol,
        }
    }

    pub fn load(&mut self, session: &mut SessionInfo) -> Vec<Diagnostic> {
        let mut diagnostics: Vec<Diagnostic> =  vec![];
        let sym = self.symbol.clone();
        if sym.borrow().typ() != SymType::CLASS {
            return diagnostics;
        }
        if !self.test_symbol_is_model(session, &mut diagnostics) {
            return diagnostics;
        }
        self._load_class_inherit(session, &mut diagnostics);
        self._load_class_name(session, &mut diagnostics);
        if sym.borrow().as_class_sym()._model.is_none() {
            return diagnostics;
        }
        self._load_class_inherits(session, &mut diagnostics);
        self._load_class_attributes(session, &mut diagnostics);
        self._add_magic_fields(session);
        let model_name = sym.borrow().as_class_sym()._model.as_ref().unwrap().name.clone();
        if let Some(module) = sym.borrow().find_module() {
            let file = self.symbol.borrow().get_file().unwrap().upgrade().unwrap();
            let xml_id_model_name = oyarn!("model_{}", model_name.replace(".", "_").as_str());
            let mut module = module.borrow_mut();
            let set = module.as_module_package_mut().xml_id_locations.entry(xml_id_model_name.clone()).or_insert(PtrWeakHashSet::new());
            set.insert(file.clone());
            drop(module); //in case of file being same than module
            let mut file = file.borrow_mut();
            file.insert_xml_id(xml_id_model_name.clone(), OdooData::RECORD(OdooDataRecord {
                file_symbol: Rc::downgrade(&sym),
                model: (Sy!("ir.model"), std::ops::Range::<usize> {
                    start: 0,
                    end: 1,
                }),
                xml_id: Some(xml_id_model_name),
                fields: vec![],
                range: std::ops::Range::<usize> {
                    start: self.symbol.borrow().range().start().to_usize(),
                    end: self.symbol.borrow().range().end().to_usize(),
                }
            }));
        }
        match session.sync_odoo.models.get(&model_name).cloned(){
            Some(model) => model.borrow_mut().add_symbol(session, sym.clone()),
            None => {
                let model = Model::new(model_name.clone(), sym.clone());
                session.sync_odoo.models.insert(model_name.clone(), Rc::new(RefCell::new(model)));
            }
        }
        session.sync_odoo.get_main_entry().borrow_mut().search_rebuild_for_models(session, model_name);
        self.process_fields(session, sym);
        diagnostics
    }

    fn _load_class_inherit(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        let mut symbol = self.symbol.borrow_mut();
        let _inherit = symbol.get_symbol(&(vec![], vec![Sy!("_inherit")]), u32::MAX);
        if let Some(_inherit) = _inherit.last() {
            if _inherit.borrow().evaluations().is_none() || _inherit.borrow().evaluations().unwrap().len() == 0 {
                error!("wrong _inherit structure");
            }
            for eval in _inherit.borrow().evaluations().unwrap().iter() {
                let eval = eval.follow_ref_and_get_value(session, &mut None, diagnostics);
                if let Some(eval) = eval.as_ref() {
                    match eval {
                        EvaluationValue::CONSTANT(Expr::StringLiteral(s)) => {
                            symbol.as_class_sym_mut()._model.as_mut().unwrap().inherit = vec![oyarn!("{}", s.value)];
                        },
                        EvaluationValue::LIST(l) | EvaluationValue::TUPLE(l)=> {
                            for e in l {
                                if let Expr::StringLiteral(s) = e {
                                    symbol.as_class_sym_mut()._model.as_mut().unwrap().inherit.push(oyarn!("{}", s.value));
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

    fn _evaluate_name(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) -> OYarn {
        let mut symbol = self.symbol.borrow_mut();
        let _name = symbol.get_symbol(&(vec![], vec![Sy!("_name")]), u32::MAX);
        if let Some(_name) = _name.last() {
            for eval in _name.borrow().evaluations().unwrap().iter() {
                let eval = eval.follow_ref_and_get_value(session, &mut None, diagnostics);
                if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = eval {
                    return oyarn!("{}", s.value);
                }
            }
            error!("unable to parse model name");
            return OYarn::from("");
        }
        if let Some(inherit_name) = symbol.as_class_sym_mut()._model.as_ref().unwrap().inherit.first() {
            return inherit_name.clone();
        }
        symbol.name().clone()
    }

    fn _load_class_name(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        let class_name = self._evaluate_name(session, diagnostics);
        let mut symbol = self.symbol.borrow_mut();
        symbol.as_class_sym_mut()._model.as_mut().unwrap().name = class_name;
        if symbol.as_class_sym()._model.as_ref().unwrap().name.is_empty() {
            symbol.as_class_sym_mut()._model = None;
            return;
        }
        if symbol.as_class_sym()._model.as_ref().unwrap().name != Sy!("base") {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().inherit.push(Sy!("base"));
        }
    }

    fn _load_class_inherits(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        let mut symbol = self.symbol.borrow_mut();
        let _inherits = symbol.get_symbol(&(vec![], vec![Sy!("_inherits")]), u32::MAX);
        if let Some(_inherits) = _inherits.last() {
            for eval in _inherits.borrow().evaluations().unwrap().iter() {
                let eval = eval.follow_ref_and_get_value(session, &mut None, diagnostics);
                symbol.as_class_sym_mut()._model.as_mut().unwrap().inherits.clear();
                if let Some(EvaluationValue::DICT(d)) = eval {
                    for (k, v) in d.iter() {
                        if let (Expr::StringLiteral(k), Expr::StringLiteral(v)) = (k,v) {
                            symbol.as_class_sym_mut()._model.as_mut().unwrap().inherits.push((oyarn!("{}", k.value), oyarn!("{}", v.value)));
                        } else {
                            error!("wrong _inherits value");
                        }
                    }
                } else {
                    error!("wrong _inherits value");
                }
            }
        }
        drop(_inherits);
        drop(symbol);
        //Add inherits from delegate=True from fields
        let all_fields = Symbol::all_members(&self.symbol, session, false, true, false, None, false);
        for (field_name, symbols) in all_fields.iter() {
            for (symbol, _deps) in symbols.iter() {
                if let Some(evals) = symbol.borrow().evaluations() {
                    for eval in evals.iter() {
                        let symbol_weak = eval.symbol.get_symbol_as_weak(session, &mut None, diagnostics, self.symbol.borrow().get_file().unwrap().upgrade());
                        if let Some(eval_symbol) = symbol_weak.weak.upgrade() {
                            if eval_symbol.borrow().name() == &Sy!("Many2one") {
                                let context = &symbol_weak.context;
                                if let Some(delegate) = context.get("delegate") {
                                    if delegate.as_bool() == true {
                                        if let Some(comodel) = context.get("comodel_name") {
                                            let comodel_name = oyarn!("{}", comodel.as_string());
                                            self.symbol.borrow_mut().as_class_sym_mut()._model.as_mut().unwrap().inherits.push((comodel_name, field_name.clone()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn _get_attribute(session: &mut SessionInfo, loc_sym: &mut Symbol, attr: &String, diagnostics: &mut Vec<Diagnostic>) -> Option<EvaluationValue> {
        let (attr_sym, _) = loc_sym.get_member_symbol(session, attr, None, true, false, false, false, false);
        if attr_sym.len() == 0 {
            return None;
        }
        let attr_sym = &attr_sym[0];
        for eval in attr_sym.borrow().evaluations().unwrap().iter() {
            let eval = eval.follow_ref_and_get_value(session, &mut None, diagnostics);
            if eval.is_some() {
                return eval;
            }
        }
        None
    }

    fn _load_class_attributes(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        let mut symbol = self.symbol.borrow_mut();
        let descr = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_description".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = descr {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().description = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().description = symbol.as_class_sym()._model.as_ref().unwrap().name.to_string();
        }
        let auto = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_auto".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = auto {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().auto = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().auto = false;
        }
        let log_access = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_log_access".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = log_access {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().log_access = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().log_access = symbol.as_class_sym()._model.as_ref().unwrap().auto;
        }
        let table = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_table".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = table {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().table = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().table = symbol.as_class_sym()._model.as_ref().unwrap().name.clone().replace(".", "_");
        }
        let sequence = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_sequence".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = sequence {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().sequence = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().sequence = symbol.as_class_sym()._model.as_ref().unwrap().table.clone() + "_id_seq";
        }
        let is_abstract = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_abstract".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = is_abstract {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().is_abstract = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().is_abstract = true;
        }
        let transient = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_transient".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = transient {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().transient = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().transient = false;
        }
        let rec_name = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_rec_name".to_string(), diagnostics);
        //TODO check that rec_name is a field
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = rec_name {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().rec_name = Some(S!(s.value.to_str()));
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().rec_name = Some(S!("name")); //TODO if name is not on model, take 'id'
        }
        let _check_company_auto = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_check_company_auto".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = _check_company_auto {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().check_company_auto = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().check_company_auto = false;
        }
        let parent_name = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_parent_name".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = parent_name {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().parent_name = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().parent_name = S!("parent_id");
        }
        let parent_store = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_parent_store".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = parent_store {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().parent_store = b.value;
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().parent_store = false;
        }
        let active_name = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_active_name".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = active_name {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().active_name = Some(S!(s.value.to_str()));
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().active_name = None;
        }
        let data_name = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_data_name".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = data_name {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().data_name = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().data_name = S!("date");
        }
        let fold_name = PythonOdooBuilder::_get_attribute(session, &mut symbol, &"_fold_name".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = fold_name {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().fold_name = S!(s.value.to_str());
        } else {
            symbol.as_class_sym_mut()._model.as_mut().unwrap().fold_name = S!("fold");
        }
    }

    fn _add_magic_fields(&mut self, session: &mut SessionInfo) {
        let mut symbol = self.symbol.borrow_mut();
        //These magic fields are added at odoo step, but it should be ok as most usage will be done in functions, not outside.
        //id
        let range = symbol.range().clone();
        let id = symbol.add_new_variable(session, Sy!("id"), &range);
        let id_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Id")]), u32::MAX);
        if !id_field.is_empty() {
            id.borrow_mut().evaluations_mut().unwrap().push(Evaluation::eval_from_symbol(&Rc::downgrade(id_field.last().unwrap()), Some(true)));
        }
        //display_name
        let display_name = symbol.add_new_variable(session, Sy!("display_name"), &range);
        let char_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Char")]), u32::MAX);
        if !char_field.is_empty() {
            display_name.borrow_mut().evaluations_mut().unwrap().push(Evaluation::eval_from_symbol(&Rc::downgrade(char_field.last().unwrap()), Some(true)));
        }
        //if log_access
        if symbol.as_class_sym()._model.as_ref().unwrap().log_access {
            //create_uid
            let create_uid = symbol.add_new_variable(session, Sy!("create_uid"), &range);
            let many2one_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2one")]), u32::MAX);
            if !many2one_field.is_empty() {
                create_uid.borrow_mut().evaluations_mut().unwrap().push(Evaluation::eval_from_symbol(&Rc::downgrade(many2one_field.last().unwrap()), Some(true)));
            }
            //create_date
            let create_date = symbol.add_new_variable(session, Sy!("create_date"), &range);
            let datetime_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Datetime")]), u32::MAX);
            if !datetime_field.is_empty() {
                create_date.borrow_mut().evaluations_mut().unwrap().push(Evaluation::eval_from_symbol(&Rc::downgrade(datetime_field.last().unwrap()), Some(true)));
            }
            //write_uid
            let write_uid = symbol.add_new_variable(session, Sy!("write_uid"), &range);
            let many2one_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2one")]), u32::MAX);
            if !many2one_field.is_empty() {
                write_uid.borrow_mut().evaluations_mut().unwrap().push(Evaluation::eval_from_symbol(&Rc::downgrade(many2one_field.last().unwrap()), Some(true)));
            }
            //write_date
            let write_date = symbol.add_new_variable(session, Sy!("write_date"), &range);
            let datetime_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Datetime")]), u32::MAX);
            if !datetime_field.is_empty() {
                write_date.borrow_mut().evaluations_mut().unwrap().push(Evaluation::eval_from_symbol(&Rc::downgrade(datetime_field.last().unwrap()), Some(true)));
            }
        }
    }

    /* true if the symbol inherits from BaseModel, Model, TransientModel, or CachedModel. symbol must be the data of rc_symbol and must be a Class */
    fn test_symbol_is_model(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) -> bool {
        let symbol = &self.symbol.clone();
        let odoo_symbol_tree = symbol.borrow().get_main_entry_tree(session);
        let mut sym = symbol.borrow_mut();
        if [&[Sy!("BaseModel")], &[Sy!("Model")], &[Sy!("TransientModel")]].iter().any(|x| x == &odoo_symbol_tree.1.as_slice()) &&
            // [BaseModel|Model|TransientModel]
            (( // < 18.1, and we are on odoo.models.
                compare_semver(session.sync_odoo.full_version.as_str(), "18.1") == Ordering::Less
                && odoo_symbol_tree.0 == &["odoo", "models"]
            ) || ( // >= 18.1, and we are on odoo.orm.models.
                compare_semver(session.sync_odoo.full_version.as_str(), "18.1") >= Ordering::Equal
                && odoo_symbol_tree.0 == &["odoo", "orm", "models"]
            ))
            // >= 18.3, and we are on odoo.orm.models_transient.TransientModel
            || (
                compare_semver(session.sync_odoo.full_version.as_str(), "18.3") >= Ordering::Equal
                && odoo_symbol_tree.1 == &["TransientModel"]
                && odoo_symbol_tree.0 == &["odoo", "orm", "models_transient"]
            )
            // we are on odoo.orm.models_cached.CachedModel
            || (
                compare_semver(session.sync_odoo.full_version.as_str(), "19.1") >= Ordering::Equal
                && odoo_symbol_tree.1 == &["CachedModel"]
                && odoo_symbol_tree.0 == &["odoo", "orm", "models_cached"]
            )
        {
            //we don't want to compare these classes with themselves, so we exit early
            return false;
        }
        if sym.as_class_sym().bases.is_empty() {
            return false;
        }
        let mut base_model_tree = (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel")]);
        let mut model_tree = (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("Model")]);
        let mut transient_tree = (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("TransientModel")]);
        if compare_semver(session.sync_odoo.full_version.as_str(), "18.1") >= Ordering::Equal {
            base_model_tree = (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel")]);
            model_tree = (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("Model")]);
            transient_tree = (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("TransientModel")]);
        }
        if compare_semver(session.sync_odoo.full_version.as_str(), "18.3") >= Ordering::Equal {
            transient_tree = (vec![Sy!("odoo"), Sy!("orm"), Sy!("models_transient")], vec![Sy!("TransientModel")]);
        }
        let base_model_syms = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path.as_ref().unwrap(), &base_model_tree, u32::MAX);
        let model_syms = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path.as_ref().unwrap(), &model_tree, u32::MAX);
        let transient_syms = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path.as_ref().unwrap(), &transient_tree, u32::MAX);
        if base_model_syms.is_empty() || model_syms.is_empty() || transient_syms.is_empty() {
            //one of them is not already loaded, but that's not really an issue, as now odoo step has been merged
            //with arch eval step, some files will be odooed before loading the orm fully. In this case we should
            //ignore this error. Moreover if a base is set on the class, it means that the base has been loaded, so
            //it is NOT a model.
            // session.send_notification(ShowMessage::METHOD, ShowMessageParams{
            //     typ: MessageType::ERROR,
            //     message: "Odoo base models are not found. OdooLS will be unable to generate valid diagnostics".to_string()
            // });
            return false;
        }
        if Rc::ptr_eq(symbol, &base_model_syms[0]) ||
            Rc::ptr_eq(symbol, &model_syms[0]) ||
            Rc::ptr_eq(symbol, &transient_syms[0])
        {
            return false;
        }
        if compare_semver(session.sync_odoo.full_version.as_str(), "19.1") >= Ordering::Equal{
            let cached_model_tree = (vec![Sy!("odoo"), Sy!("orm"), Sy!("models_cached")], vec![Sy!("CachedModel")]);
            let cached_model = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path.as_ref().unwrap(), &cached_model_tree, u32::MAX);
            if cached_model.is_empty() || Rc::ptr_eq(symbol, &cached_model[0]){
                return false;
            }

        }
        if !sym.as_class_sym().inherits(&base_model_syms[0], &mut None) {
            return false;
        }
        sym.as_class_sym_mut()._model = Some(ModelData::new());
        let register = sym.get_symbol(&(vec![], vec![Sy!("_register")]), u32::MAX);
        if let Some(register) = register.last() {
            let loc_register = register.borrow();
            let register_evals = &loc_register.evaluations().unwrap();
            if register_evals.len() == 1 { //we don't handle multiple values
                let eval = &register_evals[0];
                let value = eval.follow_ref_and_get_value(session, &mut None, diagnostics);
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

    fn process_fields(&self, session: &mut SessionInfo, symbol: Rc<RefCell<Symbol>>) {
        let members: Vec<_> = symbol.borrow().all_symbols().collect();
        for field in members{
            let field_borrow = field.borrow();
            let Some(evals) = field_borrow.evaluations() else {
                continue;
            };
            for eval in evals.iter() {
                let eval_sym_ptr = eval.symbol.get_symbol(session, &mut None,  &mut vec![], None);
                let eval_ptrs = Symbol::follow_ref(&eval_sym_ptr, session, &mut None, true, false, None);
                for eval_ptr in eval_ptrs.iter() {
                    let eval_weak = match &eval_ptr {
                        EvaluationSymbolPtr::WEAK(w) => w,
                        _ => continue
                    };
                    let Some(member_symbol) = eval_weak.weak.upgrade() else {
                        continue;
                    };
                    if !member_symbol.borrow().is_field_class(session){
                        continue;
                    }
                    if let Some(ContextValue::STRING(compute_ctx_val)) = eval_weak.context.get("compute"){
                        symbol.borrow_mut().as_class_sym_mut()._model.as_mut().unwrap().computes.entry(oyarn!("{}", compute_ctx_val)).or_insert_with(HashSet::new).insert(oyarn!("{}", field_borrow.name().clone()));
                    }
                }
            }
        }
    }
}
