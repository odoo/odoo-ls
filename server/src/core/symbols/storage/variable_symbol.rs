use ruff_text_size::TextRange;

use crate::{
    constants::OYarn,
    core::{
        evaluation::{Evaluation},
        evaluation_context::{Context, ContextKey, ContextValue},
        symbols::{
            storage::SymbolTable,
            symbol_keys::{ModelSymbolKey, ModuleKey, SymbolKey, VariableKey},
        },
    },
    threads::SessionInfo,
};

#[derive(Debug)]
pub struct VariableSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub doc_string: Option<String>,
    pub is_import_variable: bool,
    pub is_parameter: bool,
    pub evaluations: Vec<Evaluation>, //Vec, because sometimes a single allocation can be ambiguous, like ''' a = "5" if X else 5 '''
    pub range: TextRange,

    // parent symbol (no children)
    parent: SymbolKey,
}

impl VariableSymbol {

    pub fn new(name: &str, parent: SymbolKey, range: TextRange, is_external: bool) -> Self {
        Self {
            name: name.to_string().into(),
            is_external,
            doc_string: None,
            parent,
            range,
            is_import_variable: false,
            is_parameter: false,
            evaluations: vec![],
        }
    }

    pub fn is_type_alias(&self) -> bool {
        //TODO it does not use get_symbol call, and only evaluate "sym" from EvaluationSymbol
        !self.evaluations.is_empty() && self.evaluations.iter().all(|x| !x.symbol.is_instance().unwrap_or(true)) && !self.is_import_variable
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

    /// If this variable has been evaluated to a relational field, return the main symbol of the comodel
    pub fn get_relational_model(target: VariableKey, session: &mut SessionInfo, from_module: Option<ModuleKey>) -> Vec<ModelSymbolKey> {
        let variable_symbol = &session.st()[target]; // former method taking self
        let evaluations = variable_symbol.evaluations.clone();
        for eval in evaluations.iter() {
            let symbol = eval.symbol.get_symbol(session, None, &mut vec![], None);
            let parent = session.st()[target].parent();
            // To be able to follow related fields, we need to have the base_attr set in order to find the __get__ hook in next_refs
            // we update the context here for the case where we are coming from a decorator for example.
            let context = Context::from_iter([(ContextKey::BaseAttr, ContextValue::SYMBOL(parent.into()))]);
            let eval_weaks = SymbolTable::follow_ref(&symbol, session, Some(&context), false, false, None, None);
            for eval_weak in eval_weaks.iter() {
                if let Some(symbol) = eval_weak.upgrade_weak(session.st()) {
                    if ["Many2one", "One2many", "Many2many"].contains(&session.st().name(symbol).as_str()) {
                        let Some(comodel) = eval_weak.get_weak().context.get(ContextKey::ComodelName) else {
                            continue;
                        };
                        let Some(model) = session.sync_odoo.models.get(comodel.as_str()) else {
                            continue;
                        };
                        return model.borrow().get_main_symbols(session, from_module).collect();
                    } else if let SymbolKey::Class(k) = symbol { // Already evaluated from descriptor in follow_ref
                        return vec![k.into()];
                    } else if let SymbolKey::XmlRecord(k) = symbol {
                        return vec![k.into()];
                    }
                }
            }
        }
        vec![]
    }

    pub fn is_value(&self) -> bool {
        !self.evaluations.iter().any(|x| x.value.is_none())
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

    /// no child symbols
    pub fn children(&self) -> Vec<SymbolKey> {
        vec![]
    }

}
