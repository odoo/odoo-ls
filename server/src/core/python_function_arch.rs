use std::cell::RefCell;
use std::rc::Rc;

use tracing::info;
use crate::constants::{BuildStatus, SymType};
use crate::core::entry_point::EntryPoint;
use crate::core::file_mgr::FileMgr;
use crate::core::python_arch_eval::PythonArchEval;
use crate::{constants::{BuildSteps, DEBUG_STEPS, DEBUG_STEPS_ONLY_INTERNAL}, core::{symbols::{symbol_keys::{PythonBuildableSymbolKey, SymbolKey}}}, threads::SessionInfo};



pub struct PythonOdooFunctionAE {
    sym_stack: Vec<SymbolKey>,
    entry_point: Rc<RefCell<EntryPoint>>,
}

impl PythonOdooFunctionAE {
    pub fn new(entry_point: Rc<RefCell<EntryPoint>>, symbol: PythonBuildableSymbolKey) -> Option<Self> {
        Some(PythonOdooFunctionAE {
            sym_stack: vec![symbol.into()],
            entry_point,
        })
    }
    pub fn build_function_ae(&mut self, session: &mut SessionInfo) {
        let symbol = self.sym_stack[0];
        let file = session.st().get_file(symbol).unwrap();
        if DEBUG_STEPS && (!DEBUG_STEPS_ONLY_INTERNAL || !session.st().is_external(symbol)) {
            info!("FUNCTION_ARCH  - PYTHON {} - {}", session.st().path(file), session.st().name(symbol));
        }
        if !session.st().ready_for_step(self.sym_stack[0].unwrap_buildable_key(), BuildSteps::ODOO_FUNCTION_AE) {
            return;
        }
        if session.st().get_in_parents(symbol, &[SymType::CLASS], true).is_none() {
            session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::ODOO_FUNCTION_AE, BuildStatus::DONE);
            return;
        }
        let (_file_info_rc, loaded) = FileMgr::get_or_recreate_file_info(session, file);
        if !loaded {
            session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::ODOO_FUNCTION_AE, BuildStatus::INVALID);
            return;
        }
        let Some(mut builder) = PythonArchEval::new(session.st(), self.entry_point.clone(), self.sym_stack[0].as_python_buildable().unwrap(), true) else {
            session.st_mut().set_build_status(symbol.unwrap_buildable_key(), BuildSteps::ODOO_FUNCTION_AE, BuildStatus::INVALID);
            return;
        };
        builder.eval_arch(session);
    }
}