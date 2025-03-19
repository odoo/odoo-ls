use byteyarn::{yarn, Yarn};
use ruff_text_size::TextRange;

use crate::{constants::flatten_tree, core::evaluation::Evaluation, threads::SessionInfo};
use std::{cell::RefCell, rc::{Rc, Weak}};

use super::symbol::Symbol;

#[derive(Debug)]
pub struct VariableSymbol {
    pub name: Yarn,
    pub is_external: bool,
    pub doc_string: Option<String>,
    pub ast_indexes: Vec<u16>, //list of index to reach the corresponding ast node from file ast
    pub weak_self: Option<Weak<RefCell<Symbol>>>,
    pub parent: Option<Weak<RefCell<Symbol>>>,
    pub is_import_variable: bool,
    pub is_parameter: bool,
    pub evaluations: Vec<Evaluation>, //Vec, because sometimes a single allocation can be ambiguous, like ''' a = "5" if X else 5 '''
    pub range: TextRange,
}

impl VariableSymbol {

    pub fn new(name: Yarn, range: TextRange, is_external: bool) -> Self {
        Self {
            name,
            is_external,
            doc_string: None,
            ast_indexes: vec![],
            weak_self: None,
            parent: None,
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

    /* If this variable has been evaluated to a relational field, return the main symbol of the comodel */
    pub fn get_relational_model(&self, session: &mut SessionInfo, from_module: Option<Rc<RefCell<Symbol>>>) -> Vec<Rc<RefCell<Symbol>>> {
        for eval in self.evaluations.iter() {
            let symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let eval_weaks = Symbol::follow_ref(&symbol, session, &mut None, true, false, None, &mut vec![]);
            for eval_weak in eval_weaks.iter() {
                if let Some(symbol) = eval_weak.upgrade_weak() {
                    if ["Many2one", "One2many", "Many2many"].contains(&symbol.borrow().name().as_str()) {
                        let Some(comodel) = eval_weak.as_weak().context.get("comodel") else {
                            continue;
                        };
                        let Some(model) = session.sync_odoo.models.get(&yarn!("{}", &comodel.as_string())).cloned() else {
                            continue;
                        };
                        return model.borrow().get_main_symbols(session, from_module);
                    }
                }
            }
        }
        vec![]
    }

}