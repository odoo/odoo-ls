use std::{cell::RefCell, collections::HashMap, rc::Rc};

use ruff_text_size::{TextRange, TextSize};
use tracing::warn;

use crate::S;

use super::{evaluation::Evaluation, localized_symbol::LocalizedSymbol, symbols::symbol::Symbol};

#[derive(Debug, Clone)]
pub enum SectionIndex {
    INDEX(u32),
    OR(Vec<SectionIndex>),
    NONE,
}

#[derive(Debug, Clone)]
pub struct SectionRange {
    pub start: u32,
    pub index: u32,
    pub previous_indexes: SectionIndex,
}

// SymbolLocation hold all assignatio*ns and so evaluations to a variable so we can know all possible type at a given position
#[derive(Debug)]
pub struct SymbolLocation {
    sections: Vec<SectionRange>,
    symbols: HashMap<String, Rc<RefCell<Symbol>>>,
}

impl SymbolLocation {

    pub fn new()-> Self {
        Self {
            sections: vec![SectionRange{start: 0, index: 0, previous_indexes: SectionIndex::NONE}],
            symbols: HashMap::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<Rc<RefCell<Symbol>>> {
        return self.symbols.get(name).cloned();
    }

    pub fn remove(&mut self, name: &String) -> Option<Rc<RefCell<Symbol>>> {
        self.symbols.remove(name)
    }

    pub fn add_symbol(&mut self, name: &str, symbol: Rc<RefCell<Symbol>>) {
        self.symbols.insert(S!(name), symbol);
    }

    pub fn symbols(&self) -> &HashMap<String, Rc<RefCell<Symbol>>> {
        &self.symbols
    }

    pub fn get_section_for(&self, position: u32) -> SectionRange {
        let mut last_section = self.sections.last().unwrap();
        for section in self.sections.iter().rev().skip(1) { //reverse to fasten most calls as they will be with TextSize::MAX
            if section.start < position {
                break;
            }
            last_section = section;
        }
        last_section.clone()
    }

    /* Add a section at the END of the sections */
    pub fn add_section(&mut self, range: TextRange) -> SectionRange {
        if cfg!(debug_assertions) {
            assert!(range.start().to_u32() > self.sections.last().unwrap().start);
        }

        let last_index = (self.sections.len() -1) as u32;
        let mut previous_index = SectionIndex::INDEX(last_index);
        if range.start().to_u32() == self.sections.last().unwrap().start {
            previous_index = self.sections.last().unwrap().previous_indexes.clone();
            self.sections.pop(); //remove last as it would have a size of 0
        }
        let new_section = SectionRange {
            start: range.start().to_u32(),
            index: self.sections.len() as u32,
            previous_indexes: previous_index,
        };
        self.sections.push(new_section.clone());
        new_section
    }

    pub fn change_parent(&mut self, new_parent: SectionIndex, section: &mut SectionRange) {
        section.previous_indexes = new_parent;
    }

    /* Note on how to declare sections for an if:

    given:
    i = IfStmt
    ei = ElifStmt

    old_last_section = last_section
    i_body = i.body)
        visit_body
    ei_condition = add_section(ei.condition)
        visit_condition
    ei_body = add_section(ei.body)
        visit_body
    else_body = add_section(Range_none) //needed to have the possibility  to have ei_condition evaluated but not body
    next_sections = last_section

    change_parent(old_last_section, ei_condition)
    change_parent(ei_condition, ei_body)
    change_parent(ei_condition, else_body)
    change_parent(SectionIndex::Or(old_last_section | ei_body | else_body), next_sections)
     */

}

impl Symbol {

    /* Given a position and a SectionIndex, try to find all the relevant evaluations, going in previous SectionRange if needed.
    acc can be provided to skip previously seen indexes */
    fn _find_loc_sym(&self, position: u32, parent: &Symbol, index: &SectionIndex, acc: &mut Vec<u32>) -> Vec<Rc<RefCell<LocalizedSymbol>>> {
        let mut res = vec![];
        match index {
            SectionIndex::NONE => { return res; },
            SectionIndex::INDEX(index) => {
                let section = parent.symbols.as_ref().unwrap().sections.get(*index as usize).unwrap();
                //take index and try to find an evaluation. if no evaluation is found, search in previous index, and mix evaluation if there is multiple precedences
                for loc_sym in self.localized_sym.get(*index as usize).unwrap().iter().rev() {
                    if loc_sym.borrow().range.start().to_u32() < position {
                        res.push(loc_sym.clone());
                        break;
                    }
                }
                if !res.is_empty() {
                    return res;
                }
                acc.push(*index);
                res = self._find_loc_sym(position, parent, &section.previous_indexes, acc);
            },
            SectionIndex::OR(indexes) => {
                for index in indexes.iter() {
                    res.extend(self._find_loc_sym(position, parent, index, acc));
                }
            }
        }
        res
    }

    pub fn symbols(&self) -> &SymbolLocation {
        self.symbols.as_ref().unwrap()
    }

    /*return a list of Localized Symbol that can be effective at the given position.
    For example:
    //////
    a = 4
    if X:
        a = 5
    else:
        a = 6
    Y
    ////////
    if we call get_loc_sym with the position of Y, two 'a' symbols (5 and 6) will be returned as they can be effective at Y, depending on the value of X
    */
    pub fn get_loc_sym(&self, position: u32) -> Vec<Rc<RefCell<LocalizedSymbol>>> {
        let mut res = vec![];
        if let Some(parent) = self.parent.as_ref() {
            if let Some(parent) = parent.upgrade() {
                let parent = parent.borrow();
                let section = &parent.symbols.as_ref().unwrap().get_section_for(position);
                res = self._find_loc_sym(position, &parent, &SectionIndex::INDEX(section.index), &mut vec![]);
            } else {
                warn!("Parent must be available to get localized symbols");
            }
        } else {
            warn!("Parent must be available to get localized symbols");
        }
        res
    }

    pub fn get_loc_sym_at(&self, position: u32) -> Option<Rc<RefCell<LocalizedSymbol>>> {
        for sections in self.localized_sym.iter() {
            for s in sections.iter() {
                if s.borrow().range.start().to_u32() == position {
                    return Some(s.clone());
                }
            }
        }
        None
    }
}