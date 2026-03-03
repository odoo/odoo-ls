use ruff_text_size::TextRange;

use crate::{S, constants::OYarn, core::{evaluation::{ContextValue, Evaluation}, symbols::symbol_table::{SymbolKey, VariableKey, follow_ref, get_sym}}, oyarn, threads::SessionInfo};
use std::collections::HashMap;

use super::symbol::Symbol;

#[derive(Debug)]
pub struct VariableSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub doc_string: Option<String>,
    // pub weak_self: Option<Weak<RefCell<Symbol>>>,
    // @todo: is it possible to have a variable that does not have a parent?
    pub parent: Option<SymbolKey>,
    pub is_import_variable: bool,
    pub is_parameter: bool,
    pub evaluations: Vec<Evaluation>, //Vec, because sometimes a single allocation can be ambiguous, like ''' a = "5" if X else 5 '''
    pub range: TextRange,
}

impl VariableSymbol {

    pub fn new(name: OYarn, parent: SymbolKey, range: TextRange, is_external: bool) -> Self {
        Self {
            name,
            is_external,
            doc_string: None,
            // weak_self: None,
            parent: Some(parent),
            range,
            is_import_variable: false,
            is_parameter: false,
            evaluations: vec![],
        }
    }

    pub fn is_type_alias(&self) -> bool {
        //TODO it does not use get_symbol call, and only evaluate "sym" from EvaluationSymbol
        return self.evaluations.len() >= 1 && self.evaluations.iter().all(|x| !x.symbol.is_instance().unwrap_or(true)) && !self.is_import_variable;
    }

    // pub fn full_size_of(self) -> serde_json::Value {
    //     let name_to_add = if self.name.len() > 15 {
    //         self.name.len()
    //     } else {
    //         0
    //     };
    //     let mut evals = 0;
    //     for eval in self.evaluations.iter() {
    //         evals += eval.full_size_of();
    //     }
    //     size_of::<Self>() +
    //     name_to_add +
    //     self.doc_string.map(|x| x.capacity()).unwrap_or(0) +
    //     self.ast_indexes.capacity() +
    //     evals
    // }

    /* If this variable has been evaluated to a relational field, return the main symbol of the comodel */
    pub fn get_relational_model(target: VariableKey, session: &mut SessionInfo, from_module: Option<SymbolKey>) -> Vec<SymbolKey> {
        macro_rules! st { () => { session.sync_odoo.symbol_table } }  
        let variable_symbol = st!().variables.get(target).expect("valid key"); // former method taking self
        let evaluations = variable_symbol.evaluations.clone();
        for eval in evaluations.iter() {
            let symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let mut context = None;
            if let Some(parent) = st!().variables[target].parent {
                // To be able to follow related fields, we need to have the base_attr set in order to find the __get__ hook in next_refs
                // we update the context here for the case where we are coming from a decorator for example.
                context = Some(HashMap::new());
                context.as_mut().unwrap().insert(S!("base_attr"), ContextValue::SYMBOL(parent));
            }
            let eval_weaks = follow_ref(&symbol, session, &mut context, false, false, None, None);
            for eval_weak in eval_weaks.iter() {
                if let Some(symbol) = st!().upgrade_weak(eval_weak) {
                    if ["Many2one", "One2many", "Many2many"].contains(&get_sym!(st!(), symbol).name().as_str()) {
                        let Some(comodel) = eval_weak.as_weak().context.get("comodel_name") else {
                            continue;
                        };
                        let Some(model) = session.sync_odoo.models.get(&oyarn!("{}", &comodel.as_string())).cloned() else {
                            continue;
                        };
                        return model.borrow().get_main_symbols(session, from_module);
                    } else if matches!(symbol, SymbolKey::Class(_)) { // Already evaluated from descriptor in follow_ref
                        return vec![symbol];
                    }
                }
            }
        }
        vec![]
    }

    pub fn is_value(&self) -> bool {
        return !self.evaluations.iter().any(|x| x.value.is_none());
    }

}
