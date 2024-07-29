use std::{cell::RefCell, collections::HashMap, rc::Rc};

use ruff_text_size::{TextRange, TextSize};
use tracing::warn;

use crate::S;

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