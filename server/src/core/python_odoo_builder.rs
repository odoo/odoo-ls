use std::rc::Rc;
use std::cell::RefCell;
use ruff_python_ast::Expr;
use lsp_types::Diagnostic;
use tracing::error;

use crate::constants::OYarn;
use crate::core::evaluation_context::ContextKey;
use crate::core::model::{Model, ModelData};
use crate::core::symbols::{ClassSymbol, ModuleSymbol};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::symbol_keys::{ClassKey, SymbolKey, XmlId};
use crate::threads::SessionInfo;
use crate::tree::TreeStrSlice;
use crate::{oyarn, Sy, S};

use super::evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationValue};
use super::evaluation_context::ContextValue;

pub const MAGIC_FIELDS: [&str; 6] = [
    "id",
    "display_name",
    "create_uid",
    "create_date",
    "write_uid",
    "write_date"
];
pub const ACCESS_OPERATOR_OPTIONS: [&str; 4] = ["read", "write", "create", "unlink"];

pub struct PythonOdooBuilder {
    symbol: ClassKey,
}

impl PythonOdooBuilder {

    pub fn new(symbol: ClassKey) -> PythonOdooBuilder {
        PythonOdooBuilder {
            symbol,
        }
    }

    pub fn load(&mut self, session: &mut SessionInfo) -> Vec<Diagnostic> {
        let mut diagnostics: Vec<Diagnostic> =  vec![];
        let sym = self.symbol;
        if !self.is_symbol_model(session, &mut diagnostics) {
            return diagnostics;
        }
        session.st_mut()[sym]._model = Some(ModelData::new());
        self._load_class_inherit(session, &mut diagnostics);
        self._load_class_name(session, &mut diagnostics);
        if session.st()[sym]._model.is_none() {
            return diagnostics;
        }
        self._load_class_inherits(session, &mut diagnostics);
        self._load_class_attributes(session, &mut diagnostics);
        self._add_magic_fields(session);
        let model_name = session.st()[sym]._model.as_ref().unwrap().name.clone();
        if let Some(module) = session.st().find_module(sym) {
            let xml_id_model_name = oyarn!("model_{}", model_name.replace(".", "_").as_str());
            ModuleSymbol::insert_xml_id(session.st_mut(), module, xml_id_model_name, XmlId::PythonClass(sym));
        }
        match session.sync_odoo.models.get(&model_name).cloned() {
            Some(model) => {
                if model.borrow().has_xml_symbols(session.st()) {
                    if let Some(file_sym) = session.st().get_file(self.symbol.into()) {
                        session.sync_odoo.symbol_table.add_model_dependencies(file_sym, &model);
                    }
                } else {
                    model.borrow_mut().add_symbol(session, sym)
                }
            }
            None => {
                let mut model = Model::new(model_name.clone());
                model.add_symbol(session, sym);
                session.sync_odoo.models.insert(model_name.clone(), Rc::new(RefCell::new(model)));
            }
        }
        session.sync_odoo.get_main_entry().borrow_mut().search_rebuild_for_models(session, model_name);
        self.process_fields(session, sym);
        diagnostics
    }

    fn _load_class_inherit(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        let symbol = self.symbol;
        let _inherit = session.st().get_symbol(symbol.into(), (&[], &["_inherit"]), u32::MAX);
        let Some(&SymbolKey::Variable(_inherit)) = _inherit.last() else { return };
        let evaluations = &session.st()[_inherit].evaluations;
        if evaluations.is_empty() {
            error!("wrong _inherit structure");
            return;
        }
        for eval in evaluations.clone() {
            if let Some(eval) = eval.follow_ref_and_get_value(session, None, diagnostics) {
                match eval {
                    EvaluationValue::CONSTANT(c) => {
                        if let Expr::StringLiteral(s) = c.as_ref() {
                            session.st_mut()[symbol]._model.as_mut().unwrap().inherit = vec![oyarn!("{}", s.value)];
                        } else {
                            error!("wrong _inherit value");
                        }
                    },
                    EvaluationValue::LIST(l) | EvaluationValue::TUPLE(l)=> {
                        for e in l {
                            if let Expr::StringLiteral(s) = e {
                                session.st_mut()[symbol]._model.as_mut().unwrap().inherit.push(oyarn!("{}", s.value));
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
        let symbol = self.symbol;
        let _name = session.st().get_symbol(symbol.into(), (&[], &["_name"]), u32::MAX);
        if let Some(&_name) = _name.last() {
            for eval in session.st().evaluations(_name).unwrap().clone() {
                let eval = eval.follow_ref_and_get_value(session, None, diagnostics);
                if let Some(s) = eval.as_ref().and_then(EvaluationValue::as_string_literal) {
                    return oyarn!("{}", s.value);
                }
            }
            error!("unable to parse model name");
            return OYarn::from("");
        }
        if let Some(inherit_name) = session.st()[symbol]._model.as_ref().unwrap().inherit.first() {
            return inherit_name.clone();
        }
        session.st()[symbol].name.clone()
    }

    fn _load_class_name(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        let class_name = self._evaluate_name(session, diagnostics);
        let symbol = &mut session.sync_odoo.symbol_table[self.symbol];
        symbol._model.as_mut().unwrap().name = class_name;
        if symbol._model.as_ref().unwrap().name.is_empty() {
            symbol._model = None;
            return;
        }
        if symbol._model.as_ref().unwrap().name != "base" {
            symbol._model.as_mut().unwrap().inherit.push(Sy!("base"));
        }
    }

    fn _load_class_inherits(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        let symbol = self.symbol;
        let _inherits = session.st().get_symbol(symbol.into(), (&[], &["_inherits"]), u32::MAX);
        if let Some(&SymbolKey::Variable(_inherits)) = _inherits.last()
            && let Some(eval) = session.st()[_inherits].evaluations.last().cloned() {
                let eval = eval.follow_ref_and_get_value(session, None, diagnostics);
                let model = session.st_mut()[symbol]._model.as_mut().unwrap();
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
        //Add inherits from delegate=True from fields
        let all_fields = SymbolTable::all_members(
            self.symbol.into(),
            session,
            false,
            true,
            false,
            None,
            false,
        );
        for (field_name, symbols) in all_fields.iter() {
            for symbol in symbols.iter() {
                let Some(evals) = session.st().evaluations(*symbol) else { continue };
                for eval in evals.clone() {
                    let scope = session.st().get_file(self.symbol.into()).map(SymbolKey::from);
                    let symbol_weak = eval.symbol.get_symbol_as_weak(session, None, diagnostics, scope);
                    let Some(eval_symbol) = symbol_weak.weak.upgrade(session.st()) else { continue };
                    if session.st().name(eval_symbol) != &Sy!("Many2one") { continue; }
                    let context = &symbol_weak.context;
                    let Some(delegate) = context.get(ContextKey::Delegate) else { continue };
                    if delegate.as_bool() && let Some(comodel) = context.get(ContextKey::ComodelName) {
                        let comodel_name = oyarn!("{}", comodel.as_str());
                        session.st_mut()[self.symbol]._model.as_mut().unwrap().inherits.push((comodel_name, field_name.clone()));
                    }
                }
            }
        }
    }

    fn _get_attribute(session: &mut SessionInfo, loc_sym: ClassKey, attr: &str, diagnostics: &mut Vec<Diagnostic>) -> Option<EvaluationValue> {
        let (attr_sym, _) = SymbolTable::get_member_symbol(session, loc_sym.into(), attr, None, true, false, false, false, false);
        let &attr_sym = attr_sym.first()?;
        for eval in session.st().evaluations(attr_sym).unwrap().clone() {
            let eval = eval.follow_ref_and_get_value(session, None, diagnostics);
            if eval.is_some() {
                return eval;
            }
        }
        None
    }

    fn _load_class_attributes(&mut self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) {
        let symbol = self.symbol;
        let descr = PythonOdooBuilder::_get_attribute(session, symbol, "_description", diagnostics);
        if let Some(s) = descr.as_ref().and_then(EvaluationValue::as_string_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().description = S!(s.value.to_str());
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().description = session.st()[symbol]._model.as_ref().unwrap().name.to_string();
        }
        let auto = PythonOdooBuilder::_get_attribute(session, symbol, "_auto", diagnostics);
        if let Some(b) = auto.as_ref().and_then(EvaluationValue::as_bool_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().auto = b;
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().auto = false;
        }
        let log_access = PythonOdooBuilder::_get_attribute(session, symbol, "_log_access", diagnostics);
        if let Some(b) = log_access.as_ref().and_then(EvaluationValue::as_bool_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().log_access = b;
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().log_access = session.st()[symbol]._model.as_ref().unwrap().auto;
        }
        let table = PythonOdooBuilder::_get_attribute(session, symbol, "_table", diagnostics);
        if let Some(s) = table.as_ref().and_then(EvaluationValue::as_string_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().table = S!(s.value.to_str());
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().table = session.st()[symbol]._model.as_ref().unwrap().name.replace(".", "_");
        }
        let sequence = PythonOdooBuilder::_get_attribute(session, symbol, "_sequence", diagnostics);
        if let Some(s) = sequence.as_ref().and_then(EvaluationValue::as_string_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().sequence = S!(s.value.to_str());
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().sequence = session.st()[symbol]._model.as_ref().unwrap().table.clone() + "_id_seq";
        }
        let is_abstract = PythonOdooBuilder::_get_attribute(session, symbol, "_abstract", diagnostics);
        if let Some(b) = is_abstract.as_ref().and_then(EvaluationValue::as_bool_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().is_abstract = b;
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().is_abstract = true;
        }
        let transient = PythonOdooBuilder::_get_attribute(session, symbol, "_transient", diagnostics);
        if let Some(b) = transient.as_ref().and_then(EvaluationValue::as_bool_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().transient = b;
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().transient = false;
        }
        let rec_name = PythonOdooBuilder::_get_attribute(session, symbol, "_rec_name", diagnostics);
        //TODO check that rec_name is a field
        if let Some(s) = rec_name.as_ref().and_then(EvaluationValue::as_string_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().rec_name = Some(S!(s.value.to_str()));
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().rec_name = Some(S!("name")); //TODO if name is not on model, take 'id'
        }
        let _check_company_auto = PythonOdooBuilder::_get_attribute(session, symbol, "_check_company_auto", diagnostics);
        if let Some(b) = _check_company_auto.as_ref().and_then(EvaluationValue::as_bool_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().check_company_auto = b;
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().check_company_auto = false;
        }
        let parent_name = PythonOdooBuilder::_get_attribute(session, symbol, "_parent_name", diagnostics);
        if let Some(s) = parent_name.as_ref().and_then(EvaluationValue::as_string_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().parent_name = S!(s.value.to_str());
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().parent_name = S!("parent_id");
        }
        let parent_store = PythonOdooBuilder::_get_attribute(session, symbol, "_parent_store", diagnostics);
        if let Some(b) = parent_store.as_ref().and_then(EvaluationValue::as_bool_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().parent_store = b;
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().parent_store = false;
        }
        let active_name = PythonOdooBuilder::_get_attribute(session, symbol, "_active_name", diagnostics);
        if let Some(s) = active_name.as_ref().and_then(EvaluationValue::as_string_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().active_name = Some(S!(s.value.to_str()));
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().active_name = None;
        }
        let data_name = PythonOdooBuilder::_get_attribute(session, symbol, "_data_name", diagnostics);
        if let Some(s) = data_name.as_ref().and_then(EvaluationValue::as_string_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().data_name = S!(s.value.to_str());
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().data_name = S!("date");
        }
        let fold_name = PythonOdooBuilder::_get_attribute(session, symbol, "_fold_name", diagnostics);
        if let Some(s) = fold_name.as_ref().and_then(EvaluationValue::as_string_literal) {
            session.st_mut()[symbol]._model.as_mut().unwrap().fold_name = S!(s.value.to_str());
        } else {
            session.st_mut()[symbol]._model.as_mut().unwrap().fold_name = S!("fold");
        }
    }

    fn _add_magic_fields(&mut self, session: &mut SessionInfo) {
        let symbol = self.symbol;
        //These magic fields are added at odoo step, but it should be ok as most usage will be done in functions, not outside.
        //id
        let range = session.st()[symbol].range;
        let id = session.st_mut().add_new_variable(symbol, "id", range);
        let id_field = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path().as_ref().unwrap(), (&["odoo", "fields"], &["Id"]), u32::MAX);
        if let Some(&id_field) = id_field.last() {
            let evaluation = Evaluation::eval_from_symbol(session.st(), id_field, Some(true));
            session.st_mut()[id].evaluations.push(evaluation);
        }
        //display_name
        let display_name = session.st_mut().add_new_variable(symbol, "display_name", range);
        let char_field = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path().as_ref().unwrap(), (&["odoo", "fields"], &["Char"]), u32::MAX);
        if let Some(&char_field) = char_field.last() {
            let evaluation = Evaluation::eval_from_symbol(session.st(), char_field, Some(true));
            session.st_mut()[display_name].evaluations.push(evaluation);
        }
        //if log_access
        if session.st()[symbol]._model.as_ref().unwrap().log_access {
            //create_uid
            let create_uid = session.st_mut().add_new_variable(symbol, "create_uid", range);
            let many2one_field = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path().as_ref().unwrap(), (&["odoo", "fields"], &["Many2one"]), u32::MAX);
            if let Some(&many2one_field) = many2one_field.last() {
                let evaluation = Evaluation::eval_from_symbol(session.st(), many2one_field, Some(true));
                session.st_mut()[create_uid].evaluations.push(evaluation);
            }
            //create_date
            let create_date = session.st_mut().add_new_variable(symbol, "create_date", range);
            let datetime_field = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path().as_ref().unwrap(), (&["odoo", "fields"], &["Datetime"]), u32::MAX);
            if let Some(&datetime_field) = datetime_field.last() {
                let evaluation = Evaluation::eval_from_symbol(session.st(), datetime_field, Some(true));
                session.st_mut()[create_date].evaluations.push(evaluation);
            }
            //write_uid
            let write_uid = session.st_mut().add_new_variable(symbol, "write_uid", range);
            let many2one_field = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path().as_ref().unwrap(), (&["odoo", "fields"], &["Many2one"]), u32::MAX);
            if let Some(&many2one_field) = many2one_field.last() {
                let evaluation = Evaluation::eval_from_symbol(session.st(), many2one_field, Some(true));
                session.st_mut()[write_uid].evaluations.push(evaluation);
            }
            //write_date
            let write_date = session.st_mut().add_new_variable(symbol, "write_date", range);
            let datetime_field = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path().as_ref().unwrap(), (&["odoo", "fields"], &["Datetime"]), u32::MAX);
            if let Some(&datetime_field) = datetime_field.last() {
                let evaluation = Evaluation::eval_from_symbol(session.st(), datetime_field, Some(true));
                session.st_mut()[write_date].evaluations.push(evaluation);
            }
        }
    }

    /* true if the symbol inherits from BaseModel, Model, TransientModel, or CachedModel. symbol must be the data of rc_symbol and must be a Class */
    fn is_symbol_model(&self, session: &mut SessionInfo, diagnostics: &mut Vec<Diagnostic>) -> bool {
        let symbol = self.symbol;
        if session.st()[symbol].bases.is_empty() || session.st().find_module(symbol).is_none() {
            // We only consider symbols that has inheritance base or defined in modules as models
            return false;
        }
        let base_model_tree: TreeStrSlice = if session.sync_odoo.version >= (18, 1) {
            (&["odoo", "orm", "models"], &["BaseModel"])
        } else {
            (&["odoo", "models"], &["BaseModel"])
        };
        let base_model_syms = session.sync_odoo.get_symbol(session.sync_odoo.config.odoo_path().as_ref().unwrap(), base_model_tree, u32::MAX);
        let Some(&SymbolKey::Class(base)) = base_model_syms.first() else {
            // base_model_syms empty so sym cannot be a model, otherwise we would have found it earlier
            return false;
        };
        if !ClassSymbol::inherits(session, symbol, base, &mut None) {
            return false;
        }
        // Check if we have a _register = False
        let register = session.st().get_symbol(symbol.into(), (&[], &["_register"]), u32::MAX);
        if let Some(&register) = register.last() {
            let register_evals = session.st().evaluations(register).unwrap().clone();
            // Read all boolean values, ignore non-boolean-value evaluations, as they can be dynamic or type annotations
            let register_evals_values: Vec<_> = register_evals.iter().filter_map(
                |eval|
                    eval.follow_ref_and_get_value(session, None, diagnostics)?.as_bool_literal()
            ).collect();
            // If we have exactly *one* False value evaluation, we consider _register = False, thus it is an abstract model
            if register_evals_values == [false] {
                return false;
            }
        }
        true
    }

    fn process_fields(&self, session: &mut SessionInfo, symbol: ClassKey) {
        for field in session.st().all_symbols(symbol.into()) {
            let Some(evals) = session.st().evaluations(field) else {
                continue;
            };
            for eval in evals.clone() {
                let eval_sym_ptr = eval.symbol.get_symbol(session, None,  &mut vec![], None);
                let eval_ptrs = SymbolTable::follow_ref(&eval_sym_ptr, session, None, true, false, None, None);
                for eval_ptr in eval_ptrs.iter() {
                    let eval_weak = match &eval_ptr {
                        EvaluationSymbolPtr::WEAK(w) => w,
                        _ => continue
                    };
                    let Some(member_symbol) = eval_weak.weak.upgrade(session.st()) else {
                        continue;
                    };
                    if !SymbolTable::is_field_class(session, member_symbol) {
                        continue;
                    }
                    if let Some(ContextValue::STRING(compute_ctx_val)) = eval_weak.context.get(ContextKey::Compute) {
                        let name = session.st().name(field).clone();
                        session.st_mut()[symbol]._model.as_mut().unwrap().computes.entry(oyarn!("{}", compute_ctx_val)).or_default().insert(name);
                    }
                }
            }
        }
    }
}
