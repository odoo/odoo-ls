use std::cmp::Ordering;
use std::collections::HashSet;
use std::rc::Rc;
use std::cell::RefCell;
use ruff_python_ast::Expr;
use lsp_types::Diagnostic;
use tracing::error;

use crate::constants::OYarn;
use crate::core::model::{Model, ModelData};
use crate::core::symbols::class_symbol::ClassSymbol;
use crate::core::symbols::symbol_table::SymbolTable;
use crate::core::symbols::symbol_keys::{ClassKey, SymbolKey};
use crate::core::xml_data::{OdooData, OdooDataRecord};
use crate::threads::SessionInfo;
use crate::utils::compare_semver;
use crate::weak_hash_set::WeakSet;
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
    symbol: ClassKey,
}

impl PythonOdooBuilder {

    pub fn new(symbol: ClassKey) -> PythonOdooBuilder {
        PythonOdooBuilder {
            symbol: symbol,
        }
    }

    pub fn load(&mut self, session: &mut SessionInfo) -> Vec<Diagnostic> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let mut diagnostics: Vec<Diagnostic> =  vec![];
        let sym = self.symbol;
        if !self.is_symbol_model(session, &mut diagnostics) {
            return diagnostics;
        }
        st!()[sym]._model = Some(ModelData::new());
        self._load_class_inherit(session, &mut diagnostics);
        self._load_class_name(session, &mut diagnostics);
        if st!()[sym]._model.is_none() {
            return diagnostics;
        }
        self._load_class_inherits(session, &mut diagnostics);
        self._load_class_attributes(session, &mut diagnostics);
        self._add_magic_fields(session);
        let model_name = st!()[sym]._model.as_ref().unwrap().name.clone();
        if let Some(module) = st!().find_module(sym) {
            let file = st!().get_file(self.symbol.into()).unwrap();
            let xml_id_model_name = oyarn!("model_{}", model_name.replace(".", "_").as_str());
            let set = st!()[module].xml_id_locations.entry(xml_id_model_name.clone()).or_insert_with(WeakSet::new);
            set.insert(file);
            let range = st!()[self.symbol].range;
            st!().insert_xml_id(file, xml_id_model_name.clone(), OdooData::RECORD(OdooDataRecord {
                symbol: sym.into(),
                model: (Sy!("ir.model"), std::ops::Range::<usize> {
                    start: 0,
                    end: 1,
                }),
                xml_id: Some(xml_id_model_name),
                fields: vec![],
                range: std::ops::Range::<usize> {
                    start: range.start().to_usize(),
                    end: range.end().to_usize(),
                }
            }));
        }
        match session.sync_odoo.models.get(&model_name).cloned() {
            Some(model) => model.borrow_mut().add_symbol(session, sym),
            None => {
                let model = Model::new(model_name.clone(), sym);
                session.sync_odoo.models.insert(model_name.clone(), Rc::new(RefCell::new(model)));
            }
        }
        session.sync_odoo.get_main_entry().borrow_mut().search_rebuild_for_models(session, model_name);
        self.process_fields(session, sym);
        diagnostics
    }

    fn _load_class_inherit(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let symbol = self.symbol;
        let _inherit = st!().get_symbol(symbol.into(), &(vec![], vec![Sy!("_inherit")]), u32::MAX);
        let Some(&_inherit) = _inherit.last() else { return };
        let evaluations = st!().evaluations(_inherit);
        if evaluations.is_none() || evaluations.unwrap().len() == 0 {
            error!("wrong _inherit structure");
            // @arena: not present in the original code. Without this, it could crash on the unwrap below.
            return;
        }
        for eval in evaluations.unwrap().clone() {
            if let Some(eval) = eval.follow_ref_and_get_value(session, &mut None, diagnostics) {
                match eval {
                    EvaluationValue::CONSTANT(Expr::StringLiteral(s)) => {
                        st!()[symbol]._model.as_mut().unwrap().inherit = vec![oyarn!("{}", s.value)];
                    },
                    EvaluationValue::LIST(l) | EvaluationValue::TUPLE(l)=> {
                        for e in l {
                            if let Expr::StringLiteral(s) = e {
                                st!()[symbol]._model.as_mut().unwrap().inherit.push(oyarn!("{}", s.value));
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

    fn _evaluate_name(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) -> OYarn {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let symbol = self.symbol;
        let _name = st!().get_symbol(symbol.into(), &(vec![], vec![Sy!("_name")]), u32::MAX);
        if let Some(&_name) = _name.last() {
            for eval in st!().evaluations(_name).unwrap().clone() {
                let eval = eval.follow_ref_and_get_value(session, &mut None, diagnostics);
                if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = eval {
                    return oyarn!("{}", s.value);
                }
            }
            error!("unable to parse model name");
            return OYarn::from("");
        }
        if let Some(inherit_name) = st!()[symbol]._model.as_ref().unwrap().inherit.first() {
            return inherit_name.clone();
        }
        st!()[symbol].name.clone()
    }

    fn _load_class_name(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        let class_name = self._evaluate_name(session, diagnostics);
        let symbol = &mut session.sync_odoo.symbol_table[self.symbol];
        symbol._model.as_mut().unwrap().name = class_name;
        if symbol._model.as_ref().unwrap().name.is_empty() {
            symbol._model = None;
            return;
        }
        if symbol._model.as_ref().unwrap().name != Sy!("base") {
            symbol._model.as_mut().unwrap().inherit.push(Sy!("base"));
        }
    }

    fn _load_class_inherits(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let symbol = self.symbol;
        let _inherits = st!().get_symbol(symbol.into(), &(vec![], vec![Sy!("_inherits")]), u32::MAX);
        if let Some(&_inherits) = _inherits.last() {
            for eval in st!().evaluations(_inherits).unwrap().clone() {
                let eval = eval.follow_ref_and_get_value(session, &mut None, diagnostics);
                let model = st!()[symbol]._model.as_mut().unwrap();
                // @arena: clear on each iteration?? Should just take the last evaluation then.
                model.inherits.clear();
                if let Some(EvaluationValue::DICT(d)) = eval {
                    for (k, v) in d.iter() {
                        if let (Expr::StringLiteral(k), Expr::StringLiteral(v)) = (k,v) {
                            model.inherits.push((oyarn!("{}", k.value), oyarn!("{}", v.value)));
                        } else {
                            error!("wrong _inherits value");
                        }
                    }
                } else {
                    error!("wrong _inherits value");
                }
            }
        }
        //Add inherits from delegate=True from fields
        let all_fields = SymbolTable::all_members(self.symbol.into(), session, false, true, false, None, false);
        for (field_name, symbols) in all_fields.iter() {
            for (symbol, _deps) in symbols.iter() {
                let Some(evals) = st!().evaluations(*symbol) else { continue };
                for eval in evals.clone() {
                    let symbol_weak = eval.symbol.get_symbol_as_weak(session, &mut None, diagnostics, st!().get_file(self.symbol.into()));
                    let Some(eval_symbol) = symbol_weak.weak.upgrade(&st!()) else { continue };
                    if st!().name(eval_symbol) != &Sy!("Many2one") { continue; }
                    let context = &symbol_weak.context;
                    let Some(delegate) = context.get("delegate") else { continue };
                    if delegate.as_bool() == true && let Some(comodel) = context.get("comodel_name") {
                        let comodel_name = oyarn!("{}", comodel.as_string());
                        st!()[self.symbol]._model.as_mut().unwrap().inherits.push((comodel_name, field_name.clone()));
                    }
                }
            }
        }
    }

    fn _get_attribute(session: &mut SessionInfo, loc_sym: ClassKey, attr: &String, diagnostics: &mut Vec<Diagnostic>) -> Option<EvaluationValue> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let (attr_sym, _) = SymbolTable::get_member_symbol(session, loc_sym.into(), attr, None, true, false, false, false, false);
        let &attr_sym = attr_sym.first()?;
        for eval in st!().evaluations(attr_sym).unwrap().clone() {
            let eval = eval.follow_ref_and_get_value(session, &mut None, diagnostics);
            if eval.is_some() {
                return eval;
            }
        }
        None
    }

    fn _load_class_attributes(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let symbol = self.symbol;
        let descr = PythonOdooBuilder::_get_attribute(session, symbol, &"_description".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = descr {
            st!()[symbol]._model.as_mut().unwrap().description = S!(s.value.to_str());
        } else {
            st!()[symbol]._model.as_mut().unwrap().description = st!()[symbol]._model.as_ref().unwrap().name.to_string();
        }
        let auto = PythonOdooBuilder::_get_attribute(session, symbol, &"_auto".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = auto {
            st!()[symbol]._model.as_mut().unwrap().auto = b.value;
        } else {
            st!()[symbol]._model.as_mut().unwrap().auto = false;
        }
        let log_access = PythonOdooBuilder::_get_attribute(session, symbol, &"_log_access".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = log_access {
            st!()[symbol]._model.as_mut().unwrap().log_access = b.value;
        } else {
            st!()[symbol]._model.as_mut().unwrap().log_access = st!()[symbol]._model.as_ref().unwrap().auto;
        }
        let table = PythonOdooBuilder::_get_attribute(session, symbol, &"_table".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = table {
            st!()[symbol]._model.as_mut().unwrap().table = S!(s.value.to_str());
        } else {
            st!()[symbol]._model.as_mut().unwrap().table = st!()[symbol]._model.as_ref().unwrap().name.replace(".", "_");
        }
        let sequence = PythonOdooBuilder::_get_attribute(session, symbol, &"_sequence".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = sequence {
            st!()[symbol]._model.as_mut().unwrap().sequence = S!(s.value.to_str());
        } else {
            st!()[symbol]._model.as_mut().unwrap().sequence = st!()[symbol]._model.as_ref().unwrap().table.clone() + "_id_seq";
        }
        let is_abstract = PythonOdooBuilder::_get_attribute(session, symbol, &"_abstract".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = is_abstract {
            st!()[symbol]._model.as_mut().unwrap().is_abstract = b.value;
        } else {
            st!()[symbol]._model.as_mut().unwrap().is_abstract = true;
        }
        let transient = PythonOdooBuilder::_get_attribute(session, symbol, &"_transient".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = transient {
            st!()[symbol]._model.as_mut().unwrap().transient = b.value;
        } else {
            st!()[symbol]._model.as_mut().unwrap().transient = false;
        }
        let rec_name = PythonOdooBuilder::_get_attribute(session, symbol, &"_rec_name".to_string(), diagnostics);
        //TODO check that rec_name is a field
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = rec_name {
            st!()[symbol]._model.as_mut().unwrap().rec_name = Some(S!(s.value.to_str()));
        } else {
            st!()[symbol]._model.as_mut().unwrap().rec_name = Some(S!("name")); //TODO if name is not on model, take 'id'
        }
        let _check_company_auto = PythonOdooBuilder::_get_attribute(session, symbol, &"_check_company_auto".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = _check_company_auto {
            st!()[symbol]._model.as_mut().unwrap().check_company_auto = b.value;
        } else {
            st!()[symbol]._model.as_mut().unwrap().check_company_auto = false;
        }
        let parent_name = PythonOdooBuilder::_get_attribute(session, symbol, &"_parent_name".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = parent_name {
            st!()[symbol]._model.as_mut().unwrap().parent_name = S!(s.value.to_str());
        } else {
            st!()[symbol]._model.as_mut().unwrap().parent_name = S!("parent_id");
        }
        let parent_store = PythonOdooBuilder::_get_attribute(session, symbol, &"_parent_store".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::BooleanLiteral(b))) = parent_store {
            st!()[symbol]._model.as_mut().unwrap().parent_store = b.value;
        } else {
            st!()[symbol]._model.as_mut().unwrap().parent_store = false;
        }
        let active_name = PythonOdooBuilder::_get_attribute(session, symbol, &"_active_name".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = active_name {
            st!()[symbol]._model.as_mut().unwrap().active_name = Some(S!(s.value.to_str()));
        } else {
            st!()[symbol]._model.as_mut().unwrap().active_name = None;
        }
        let data_name = PythonOdooBuilder::_get_attribute(session, symbol, &"_data_name".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = data_name {
            st!()[symbol]._model.as_mut().unwrap().data_name = S!(s.value.to_str());
        } else {
            st!()[symbol]._model.as_mut().unwrap().data_name = S!("date");
        }
        let fold_name = PythonOdooBuilder::_get_attribute(session, symbol, &"_fold_name".to_string(), diagnostics);
        if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(s))) = fold_name {
            st!()[symbol]._model.as_mut().unwrap().fold_name = S!(s.value.to_str());
        } else {
            st!()[symbol]._model.as_mut().unwrap().fold_name = S!("fold");
        }
    }

    fn _add_magic_fields(&mut self, session: &mut SessionInfo) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let symbol = self.symbol;
        //These magic fields are added at odoo step, but it should be ok as most usage will be done in functions, not outside.
        //id
        let range = st!()[symbol].range.clone();
        let id = st!().add_new_variable(symbol, Sy!("id"), &range);
        let id_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Id")]), u32::MAX);
        if let Some(&id_field) = id_field.last() {
            let evaluation = Evaluation::eval_from_symbol(&st!(), id_field, Some(true));
            st!()[id].evaluations.push(evaluation);
        }
        //display_name
        let display_name = st!().add_new_variable(symbol, Sy!("display_name"), &range);
        let char_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Char")]), u32::MAX);
        if let Some(&char_field) = char_field.last() {
            let evaluation = Evaluation::eval_from_symbol(&st!(), char_field, Some(true));
            st!()[display_name].evaluations.push(evaluation);
        }
        //if log_access
        if st!()[symbol]._model.as_ref().unwrap().log_access {
            //create_uid
            let create_uid = st!().add_new_variable(symbol, Sy!("create_uid"), &range);
            let many2one_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2one")]), u32::MAX);
            if let Some(&many2one_field) = many2one_field.last() {
                let evaluation = Evaluation::eval_from_symbol(&st!(), many2one_field, Some(true));
                st!()[create_uid].evaluations.push(evaluation);
            }
            //create_date
            let create_date = st!().add_new_variable(symbol, Sy!("create_date"), &range);
            let datetime_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Datetime")]), u32::MAX);
            if let Some(&datetime_field) = datetime_field.last() {
                let evaluation = Evaluation::eval_from_symbol(&st!(), datetime_field, Some(true));
                st!()[create_date].evaluations.push(evaluation);
            }
            //write_uid
            let write_uid = st!().add_new_variable(symbol, Sy!("write_uid"), &range);
            let many2one_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Many2one")]), u32::MAX);
            if let Some(&many2one_field) = many2one_field.last() {
                let evaluation = Evaluation::eval_from_symbol(&st!(), many2one_field, Some(true));
                st!()[write_uid].evaluations.push(evaluation);
            }
            //write_date
            let write_date = st!().add_new_variable(symbol, Sy!("write_date"), &range);
            let datetime_field = session.sync_odoo.get_symbol(&session.sync_odoo.config.odoo_path.as_ref().unwrap(), &(vec![Sy!("odoo"), Sy!("fields")], vec![Sy!("Datetime")]), u32::MAX);
            if let Some(&datetime_field) = datetime_field.last() {
                let evaluation = Evaluation::eval_from_symbol(&st!(), datetime_field, Some(true));
                st!()[write_date].evaluations.push(evaluation);
            }
        }
    }

    /* true if the symbol inherits from BaseModel, Model, TransientModel, or CachedModel. symbol must be the data of rc_symbol and must be a Class */
    fn is_symbol_model(&self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) -> bool {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        let symbol = self.symbol;
        if st!()[symbol].bases.is_empty() || st!().find_module(symbol).is_none() {
            // We only consider symbols that has inheritance base or defined in modules as models
            return false;
        }
        let base_model_tree = if compare_semver(session.sync_odoo.full_version.as_str(), "18.1") >= Ordering::Equal {
            (vec![Sy!("odoo"), Sy!("orm"), Sy!("models")], vec![Sy!("BaseModel")])
        } else {
            (vec![Sy!("odoo"), Sy!("models")], vec![Sy!("BaseModel")])
        };
        let base_model_syms = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path.as_ref().unwrap(), &base_model_tree, u32::MAX);
        // @arena: different from original, here we make sure it's a class symbol
        let Some(&SymbolKey::Class(base)) = base_model_syms.first() else {
            // base_model_syms empty so sym cannot be a model, otherwise we would have found it earlier
            return false;
        };
        if !ClassSymbol::inherits(session, symbol, base, &mut None) {
            return false;
        }
        // Check if we have a _register = False
        let register = st!().get_symbol(symbol.into(), &(vec![], vec![Sy!("_register")]), u32::MAX);
        if let Some(&register) = register.last() {
            let register_evals = st!().evaluations(register).unwrap().clone();
            // Read all boolean values, ignore non-boolean-value evaluations, as they can be dynamic or type annotations
            let register_evals_values: Vec<_> = register_evals.iter().filter_map(
                |eval|
                    match eval.follow_ref_and_get_value(session, &mut None, diagnostics)? {
                        EvaluationValue::CONSTANT(Expr::BooleanLiteral(b)) => Some(b.value),
                        _ => None,
                    }
            ).collect();
            // If we have exactly *one* False value evaluation, we consider _register = False, thus it is an abstract model
            if register_evals_values == &[false] {
                return false;
            }
        }
        true
    }

    fn process_fields(&self, session: &mut SessionInfo, symbol: ClassKey) {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }
        for field in st!().all_symbols(symbol.into()) {
            let Some(evals) = st!().evaluations(field) else {
                continue;
            };
            for eval in evals.clone() {
                let eval_sym_ptr = eval.symbol.get_symbol(session, &mut None,  &mut vec![], None);
                let eval_ptrs = SymbolTable::follow_ref(&eval_sym_ptr, session, &mut None, true, false, None, None);
                for eval_ptr in eval_ptrs.iter() {
                    let eval_weak = match &eval_ptr {
                        EvaluationSymbolPtr::WEAK(w) => w,
                        _ => continue
                    };
                    let Some(member_symbol) = eval_weak.weak.upgrade(&st!()) else {
                        continue;
                    };
                    if !SymbolTable::is_field_class(session, member_symbol) {
                        continue;
                    }
                    if let Some(ContextValue::STRING(compute_ctx_val)) = eval_weak.context.get("compute") {
                        let name = st!().name(field).clone();
                        st!()[symbol]._model.as_mut().unwrap().computes.entry(oyarn!("{}", compute_ctx_val)).or_insert_with(HashSet::new).insert(name);
                    }
                }
            }
        }
    }
}
