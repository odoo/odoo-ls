use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};

use roxmltree::Node;

use crate::{core::{evaluation::ContextValue, symbols::symbol::Symbol, xml_data::XmlData}, threads::SessionInfo, S};

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

    pub fn get_symbols(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, root: roxmltree::Node, offset: usize) -> (Vec<XmlAstResult>, Option<Range<usize>>) {
        let mut results = (vec![], None);
        let from_module = file_symbol.borrow().find_module();
        let mut context_xml = HashMap::new();
        for node in root.children() {
            XmlAstUtils::visit_node(session, &node, offset, from_module.clone(), &mut context_xml, &mut results);
        }
        results
    }

    fn visit_node(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>)) {
        if node.is_element() {
            match node.tag_name().name()  {
                "record" => {
                    XmlAstUtils::visit_record(session, &node, offset, from_module.clone(), ctxt, results);
                }
                "field" => {
                    XmlAstUtils::visit_field(session, &node, offset, from_module.clone(), ctxt, results);
                }
                _ => {
                    for child in node.children() {
                        XmlAstUtils::visit_node(session, &child, offset, from_module.clone(), ctxt, results);
                    }
                }
            }
        }
    }

    fn visit_record(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>)) {
        for attr in node.attributes() {
            if attr.name() == "model" {
                let model_name = attr.value().to_string();
                ctxt.insert(S!("model"), ContextValue::STRING(model_name.clone()));
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    if let Some(model) = session.sync_odoo.models.get(&model_name).cloned() {
                        results.0.extend(model.borrow().all_symbols(session, from_module.clone(), false).iter().filter(|s| s.1.is_none()).map(|s| XmlAstResult::SYMBOL(s.0.clone())));
                        results.1 = Some(attr.range_value());
                    }
                }
            } else if attr.name() == "id" {
                if attr.range_value().start <= offset && attr.range_value().end >= offset {
                    let xml_ids = session.sync_odoo.get_xml_ids(&from_module.clone().unwrap(), attr.value(), &attr.range(), &mut vec![]);
                    for xml_data in xml_ids.iter() {
                        match xml_data {
                            XmlData::RECORD(r) => {
                                results.0.push(XmlAstResult::XML_DATA(r.file_symbol.upgrade().unwrap(), r.range.clone()));
                                results.1 = Some(attr.range_value());
                            },
                            _ => {}
                        }
                    }
                }
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module.clone(), ctxt, results);
        }
    }

    fn visit_field(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<Rc<RefCell<Symbol>>>, ctxt: &mut HashMap<String, ContextValue>, results: &mut (Vec<XmlAstResult>, Option<Range<usize>>)) {
        for attr in node.attributes() {
            if attr.range_value().start <= offset && attr.range_value().end >= offset {
                if attr.name() == "name" {
                    let model_name = ctxt.get(&S!("model")).cloned().unwrap_or(ContextValue::STRING(S!(""))).as_string();
                    if model_name.is_empty() {
                        continue;
                    }
                    if let Some(model) = session.sync_odoo.models.get(&model_name).cloned() {
                        for symbol in model.borrow().all_symbols(session, from_module.clone(), true) {
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
            }
        }
        for child in node.children() {
            XmlAstUtils::visit_node(session, &child, offset, from_module.clone(), ctxt, results);
        }
    }

}