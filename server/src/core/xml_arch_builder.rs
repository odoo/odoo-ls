use std::{cell::RefCell, collections::HashMap, fmt, fs, path::PathBuf, rc::Rc};

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use regex::Regex;
use roxmltree::Node;
use tracing::{error, warn};

use crate::{constants::{BuildStatus, BuildSteps, OYarn, EXTENSION_NAME}, oyarn, threads::SessionInfo, S};

use super::{file_mgr::FileInfo, odoo::SyncOdoo, symbols::{symbol::Symbol, xml_file_symbol::XmlFileSymbol}};

/*
Struct made to load RelaxNG Odoo schemas and add hooks and specific OdooLS behavior on particular nodes.
*/
pub struct XmlArchBuilder {
}

impl XmlArchBuilder {

    pub fn new() -> Self {
        Self {
        }
    }

    pub fn load_arch(&mut self, session: &mut SessionInfo, xml_symbol: Rc<RefCell<Symbol>>, file_info: &mut FileInfo, node: &Node) {
        let mut diagnostics = vec![];
        xml_symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        self.load_odoo_openerp_data(session, node, &mut diagnostics);
        xml_symbol.borrow_mut().set_build_status(BuildSteps::ARCH, BuildStatus::DONE);
        file_info.replace_diagnostics(BuildSteps::ARCH, diagnostics);
    }
}