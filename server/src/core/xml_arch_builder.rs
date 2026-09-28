use crate::{
    Sy, constants::{BuildStatus, BuildSteps, DEBUG_STEPS, DiagnosticSource, MissingDataSource, OYarn}, core::{build_scheduler::BuildScheduler, entry_point::{EntryPoint, EntryPointType}, file_mgr::FileInfoKey, symbols::{ModuleSymbol, symbol_keys::{XmlDataKey, XmlId}}}, features::xml_ast_utils::XmlAstUtils, threads::SessionInfo
};
use crate::{
    core::{
        data_hooks,
        diagnostics::{create_diagnostic, DiagnosticCode},
        odoo::SyncOdoo,
        symbols::{Buildable, symbol_keys::XmlFileKey},
    },
};
use lsp_types::Diagnostic;
use roxmltree::{Attribute, Node};
use tracing::{info, error, warn};

/*
Struct made to load RelaxNG Odoo schemas and add hooks and specific OdooLS behavior on particular nodes.
*/
pub struct XmlArchBuilder {
    pub is_in_main_ep: bool,
    pub web_asset: bool,
    pub xml_symbol: XmlFileKey,
}

impl XmlArchBuilder {

    pub fn new(xml_symbol: XmlFileKey, web_asset: bool) -> Self {
        Self {
            is_in_main_ep: false,
            web_asset,
            xml_symbol
        }
    }

    pub fn load_arch(&mut self, session: &mut SessionInfo, file_info: FileInfoKey, node: &Node) {
        if !session.st().ready_for_step(self.xml_symbol.into(), BuildSteps::ARCH) {
            return;
        }
        let mut diagnostics = vec![];
        session.st_mut()[self.xml_symbol].set_build_status(BuildSteps::ARCH, BuildStatus::IN_PROGRESS);
        if DEBUG_STEPS {
            info!("ARCH       - XML: {}", session.st()[self.xml_symbol].path);
        }
        let ep = session.st().get_entry(self.xml_symbol);
        self.is_in_main_ep = session.ep_mgr()[ep].typ == EntryPointType::MAIN || session.ep_mgr()[ep].typ == EntryPointType::ADDON;
        if self.web_asset {
            self.load_frontend_data(session, node, &mut diagnostics);
        } else {
            self.load_odoo_openerp_data(session, node, &mut diagnostics);
        }
        session.st_mut()[self.xml_symbol].set_build_status(BuildSteps::ARCH, BuildStatus::DONE);
        session.file_mgr_mut()[file_info].replace_diagnostics(DiagnosticSource::XML_ARCH, diagnostics);
        BuildScheduler::queue(session, self.xml_symbol);
    }

    pub fn on_operation_creation(
        &self,
        session: &mut SessionInfo,
        id: Option<String>,
        t_name: Option<String>,
        node: &Node,
        xml_data: XmlDataKey,
        diagnostics: &mut Vec<Diagnostic>
    ) {
        if !self.is_in_main_ep {
            return;
        }
        if let Some(id) = id {
            let module = session.st().find_module(self.xml_symbol);
            let Some(module) = module else {
                warn!("Module not found for id: {}", id);
                return;
            };
            let id_split = id.split(".").collect::<Vec<&str>>();
            if id_split.len() > 2 {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05051, &[&id]) {
                    diagnostics.push(lsp_types::Diagnostic {
                        range: lsp_types::Range {
                            start: lsp_types::Position::new(node.range().start as u32, 0),
                            end: lsp_types::Position::new(node.range().end as u32, 0),
                        },
                        ..diagnostic.clone()
                    });
                }
                return;
            }
            let id = id_split.last().unwrap().to_string();
            let mut xml_module = module;
            if id_split.len() == 2 {
                let module_name = Sy!(id_split.first().unwrap().to_string());
                if let Some(module) = session.sync_odoo.modules.get(&module_name).and_then(|m| m.upgrade(session.st())) {
                    xml_module = module;
                }
            }
            if let XmlDataKey::XmlRecord(record) = xml_data {
                data_hooks::on_record_creation(session, self.xml_symbol.into(), record);
            }
            { let main_entry = session.sync_odoo.get_main_entry(); EntryPoint::search_rebuild_for_data_id(session, main_entry, MissingDataSource::XML_ID(Sy!(id.clone()))); }
            ModuleSymbol::insert_xml_id(session.st_mut(), xml_module, Sy!(id), XmlId::from(xml_data));
        }
        if let Some(t_name) = t_name {
            if XmlAstUtils::ensure_js_template_validity(session, &t_name) {
                if let Some(diagnostic) = create_diagnostic(session, DiagnosticCode::OLS05072, &[]) {
                    diagnostics.push(lsp_types::Diagnostic {
                        range: lsp_types::Range {
                            start: lsp_types::Position::new(node.range().start as u32, 0),
                            end: lsp_types::Position::new(node.range().end as u32, 0),
                        },
                        ..diagnostic.clone()
                    });
                }
            } else {
                let Some(template) = xml_data.as_xml_template_key() else {
                    error!("Template data is not a XmlTemplateKey for t-name: {}", t_name);
                    return;
                };
                { let main_entry = session.sync_odoo.get_main_entry(); EntryPoint::search_rebuild_for_data_id(session, main_entry, MissingDataSource::TEMPLATE(Sy!(t_name.clone()))); }
                session.sync_odoo.js_templates.entry(t_name).or_default().insert(template);
            }
        }
    }

    pub fn get_group_ids(&self, session: &mut SessionInfo, xml_id: &str, attr: &Attribute, diagnostics: &mut Vec<Diagnostic>) -> Vec<XmlId> {
        let xml_ids = SyncOdoo::get_xml_ids(session, self.xml_symbol.into(), xml_id, &attr.range(), diagnostics);
        let mut res = vec![];
        for data in xml_ids.iter_valid(session.st()) {
            if let XmlId::XmlRecord(r) = data {
                let record = &session.st()[r];
                if record.model.0 == "res.groups" {
                    res.push(data);
                }
            }
        }
        res
    }
}
