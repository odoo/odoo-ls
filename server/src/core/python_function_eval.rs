use std::{cell::RefCell, rc::Rc};

use lsp_types::Diagnostic;

use crate::{constants::BuildStatus, threads::SessionInfo};

use super::symbol::Symbol;

//The body Eval is used to evaluate body of classes and functions.
pub struct PythonFunctionEval {
    file_mode: bool,
    symbol: Rc<RefCell<Symbol>>,
    diagnostics: Vec<Diagnostic>,
    safe_imports: Vec<bool>,
    current_module: Option<Rc<RefCell<Symbol>>>
}

impl PythonFunctionEval {

    pub fn validate(&mut self, session: &mut SessionInfo) {
        let mut symbol = self.symbol.borrow_mut();
        self.current_module = symbol.get_module_sym();
        if symbol.validation_status != BuildStatus::PENDING {
            return;
        }
        symbol.validation_status = BuildStatus::IN_PROGRESS;
        let sym_type = symbol.sym_type.clone();
        drop(symbol);
    }
}