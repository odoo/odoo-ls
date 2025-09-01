use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};

use roxmltree::Node;

use crate::{constants::OYarn, core::{evaluation::ContextValue, odoo::SyncOdoo, symbols::{module_symbol::ModuleSymbol, symbol::Symbol}, xml_data::OdooData}, threads::SessionInfo, Sy, S};

pub enum XmlAstResult {
    SYMBOL(Rc<RefCell<Symbol>>),
    XML_DATA(Rc<RefCell<Symbol>>, Range<usize>), //xml file symbol and range of the xml data
}

impl XmlAstResult {
    pub fn as_symbol(&self) -> Rc<RefCell<Symbol>> {
        match self {
            XmlAstResult::SYMBOL(sym) => sym.clone(),
            XmlAstResult::XML_DATA(sym, _) =>panic!("Xml Data is not a symbol"),
        }
    }

    pub fn as_xml_data(&self) -> (Rc<RefCell<Symbol>>, Range<usize>) {
        match self {
            XmlAstResult::SYMBOL(_) => panic!("Symbol is not an XML Data"),
            XmlAstResult::XML_DATA(sym, range) => (sym.clone(), range.clone()),
        }
    }
}

pub struct XmlAstUtils {}

impl XmlAstUtils {

    pub fn get_symbols(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, root: roxmltree::Node, offset: usize, on_dep_only: bool) -> (Vec<XmlAstResult>, Option<Range<usize>>) {
        let mut results = (vec![], None);
        let from_module = file_symbol.borrow().find_module();
        let mut context_xml = HashMap::new();
        for node in root.children() {
            XmlAstUtils::visit_node(session, &node, offset, from_module.clone(), &mut context_xml, &mut results, on_dep_only);
        }
        results
    }

    fn visit_node(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
        if node.is_element() {
            match node.tag_name().name()  {
                "record" => {
                    XmlAstUtils::visit_record(session, &node, offset, from_module.clone(), ctxt, results, on_dep_only);
                }
                "field" => {
                    XmlAstUtils::visit_field(session, &node, offset, from_module.clone(), ctxt, results, on_dep_only);
                },
                "menuitem" => {
                    XmlAstUtils::visit_menu_item(session, &node, offset, from_module.clone(), ctxt, results, on_dep_only);
                },
                "template" => {
                    XmlAstUtils::visit_template(session, &node, offset, from_module.clone(), ctxt, results, on_dep_only);
                }
                _ => {
                    for child in node.children() {
                        XmlAstUtils::visit_node(session, &child, offset, from_module.clone(), ctxt, results, on_dep_only);
                    }
                }
            }
        } else if node.is_text() {
            XmlAstUtils::visit_text(session, &node, offset, from_module, ctxt, results, on_dep_only);
        }
    }

    fn visit_record(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "model" {
                let model_name = attr.value().to_string();
                ctxt.insert(S!("record_model"), ContextValue::STRING(model_name.clone()));
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    if let Some(model) = session.sync_odoo.models.get(&Sy!(model_name)).cloned() {
                        let from_module = match on_dep_only {
                            true => from_module.clone(),
                            false => None,
                        };
                        results.0.extend(model.borrow().all_symbols(session, from_module, false).iter().filter(|s| s.1.is_none()).map(|s| XmlAstResult::SYMBOL(s.0.clone())));
                        results.1 = Some(attr.range_value());
                    }
                }
            } else if attr.name() == "id" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), &from_module.as_ref().unwrap(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module.clone(), ctxt, results, on_dep_only);
        }
        ctxt.remove(&S!("record_model"));
    }

    fn visit_field(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "name" {
                ctxt.insert(S!("field_name"), ContextValue::STRING(attr.value().to_string()));
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    let model_name = ctxt.get(&S!("record_model")).cloned().unwrap_or(ContextValue::STRING(S!(""))).as_string();
                    if model_name.is_empty() {
                        continue;
                    }
                    if let Some(model) = session.sync_odoo.models.get(&Sy!(model_name)).cloned() {
                        let from_module = match on_dep_only {
                            true => from_module.clone(),
                            false => None,
                        };
                        for symbol in model.borrow().all_symbols(session, from_module, true) {
                            if symbol.1.is_none() {
                                let content = symbol.0.borrow().get_content_symbol(attr.value(), u32::MAX);
                                for symbol in content.symbols.iter() {
                                    results.0.push(XmlAstResult::SYMBOL(symbol.clone()));
                                }
                            }
                        }
                        results.1 = Some(attr.range_value());
                    }
                }
            } else if attr.name() == "ref" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), &from_module.as_ref().unwrap(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module.clone(), ctxt, results, on_dep_only);
        }
        ctxt.remove(&S!("field_name"));
    }

    fn visit_text(session: &mut SessionInfo, node: &Node, offset: usize, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
        if node.range().start <= offset && node.range().end >= offset {
            let model = ctxt.get(&S!("record_model")).cloned().unwrap_or(ContextValue::STRING(S!(""))).as_string();
            let field = ctxt.get(&S!("field_name")).cloned().unwrap_or(ContextValue::STRING(S!(""))).as_string();
            if model.is_empty() || field.is_empty() {
                return;
            }
            if field == "model" || field == "res_model" { //do not check model, let's assume it will contains a model name
                XmlAstUtils::add_model_result(session, node, from_module, results, on_dep_only);
            }
        }
    }

    fn visit_menu_item(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "action" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), &from_module.as_ref().unwrap(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            } else if attr.name() == "groups" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), &from_module.as_ref().unwrap(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module.clone(), ctxt, results, on_dep_only);
        }
    }

    fn visit_template(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
        for attr in node.attributes() {
            if attr.name() == "inherit_id" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), &from_module.as_ref().unwrap(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            } else if attr.name() == "groups" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    XmlAstUtils::add_xml_id_result(session, attr.value(), &from_module.as_ref().unwrap(), attr.range_value(), results, on_dep_only);
                    results.1 = Some(attr.range_value());
                }
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module.clone(), ctxt, results, on_dep_only);
        }
    }

    fn add_model_result(session: &mut SessionInfo, node: &Node, from_module: Option<Rc<RefCell<Symbol>>>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
        if let Some(model) = session.sync_odoo.models.get(node.text().unwrap()).cloned() {
            let from_module = match on_dep_only {
                true => from_module.clone(),
                false => None,
            };
            results.0.extend(model.borrow().all_symbols(session, from_module, false).iter().filter(|s| s.1.is_none()).map(|s| XmlAstResult::SYMBOL(s.0.clone())));
            results.1 = Some(node.range());
        }
    }

    fn add_xml_id_result(session: &mut SessionInfo, xml_id: &str, file_symbol: &Rc<RefCell<Symbol>>, range: Range<usize>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>), on_dep_only: bool) {
        let mut xml_ids = SyncOdoo::get_xml_ids(session, file_symbol, xml_id, &range, &mut vec![]);
        if on_dep_only {
            xml_ids = xml_ids.into_iter().filter(|x| 
                {
                    let file = x.get_file_symbol();
                    if let Some(file) = file {
                        if let Some(file) = file.upgrade() {
                            let module = file.borrow().find_module();
                            if let Some(module) = module {
                                return ModuleSymbol::is_in_deps(session, &file_symbol.borrow().find_module().unwrap(), module.borrow().name());
                            }
                        }
                    }
                        return false;
                }
            ).collect::<Vec<_>>();
        }
        for xml_data in xml_ids.iter() {
            match xml_data {
                OdooData::RECORD(r) => {
                    results.0.push(XmlAstResult::XML_DATA(r.file_symbol.upgrade().unwrap(), r.range.clone()));
                },
                _ => {}
            }
        }
    }

}