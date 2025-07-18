use std::{cell::RefCell, ops::Range, rc::Rc};

use roxmltree::Node;

use crate::{core::symbols::symbol::Symbol, threads::SessionInfo};

pub struct XmlAstUtils {}

impl XmlAstUtils {

    pub fn get_symbols(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, root: roxmltree::Node, offset: usize) -> (Vec<Rc<RefCell<Symbol>>>, Option<Range<usize>>) {
        let mut results = (vec![], None);
        let from_module = file_symbol.borrow().find_module();
        for node in root.descendants() {
            match node.tag_name().name()  {
                "record" => {
                    XmlAstUtils::visit_record(session, &node, offset, from_module.clone(), &mut results);
                }
                _ => {}
            }
        }
        results
    }

    fn visit_record(session: &mut SessionInfo<'_>, node: &Node, offset: usize, from_module: Option<Rc<RefCell<Symbol>>>, results: &mut (Vec<Rc<RefCell<Symbol>>>, Option<Range<usize>>)) {
        for attr in node.attributes() {
            if attr.range_value().start <= offset && attr.range_value().end >= offset {
                if attr.name() == "model" {
                    if let Some(model) = session.sync_odoo.models.get(attr.value()).cloned() {
                        results.0.extend(model.borrow().all_symbols(session, from_module.clone(), false).iter().map(|s| s.0.clone()));
                        results.1 = Some(attr.range_value());
                    }
                }
            }
        }
    }

}