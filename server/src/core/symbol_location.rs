use std::{cell::RefCell, collections::HashMap, rc::Rc};

use ruff_text_size::{TextRange, TextSize};
use tracing::warn;

use super::{evaluation::Evaluation, symbol::Symbol};

#[derive(Debug, Clone)]
enum SectionIndex {
    INDEX(u32),
    OR(Vec<SectionIndex>),
    NONE,
}

#[derive(Debug, Clone)]
struct SectionRange {
    start: u32,
    index: u32,
    previous_indexes: SectionIndex,
}

// SymbolLocation hold all assignations and so evaluations t o a variable so we can know all possible type at a given position
#[derive(Debug)]
pub struct SymbolLocation {
    sections: Vec<SectionRange>,
    end_offset: u32,
    symbols: HashMap<String, Rc<RefCell<Symbol>>>,
}

impl SymbolLocation {

    pub fn new(range: TextRange)-> Self {
        Self {
            sections: vec![SectionRange{start: range.start().to_u32(), index: 0, previous_indexes: SectionIndex::NONE}],
            end_offset: range.end().to_u32(),
            symbols: HashMap::new(),
        }
    }

    pub fn get_section_for(&self, position: TextSize) -> SectionRange {
        let mut last_section = self.sections.first().unwrap();
        for section in self.sections.iter().skip(1) {
            if section.start > position.to_u32() {
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
        self.sections.push(new_section);
        if range.end().to_u32() < self.end_offset {
            let end_section = SectionRange {
                start: range.start().to_u32(),
                index: self.sections.len() as u32,
                previous_indexes: SectionIndex::INDEX(last_index),
            };
            self.sections.push(end_section);
        }
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
    i_body = add_section(i.body)
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
    fn _find_eval(&self, position: TextSize, parent: &Symbol, index: &SectionIndex, acc: &mut Vec<u32>) -> Vec<&Evaluation> {
        let mut res = vec![];
        match index {
            SectionIndex::NONE => { return res; },
            SectionIndex::INDEX(index) => {
                let section = parent.symbols.as_ref().unwrap().sections.get(*index as usize).unwrap();
                //take index and try to find an evaluation. if no evaluation is found, search in previous index, and mix evaluation if there is multiple precedences
                for (eval_offset, evaluation) in self.evaluation.get(section.index as usize).unwrap().iter().rev() {
                    if *eval_offset < position.to_u32() {
                        res.push(evaluation);
                        break;
                    }
                }
                if !res.is_empty() {
                    return res;
                }
                acc.push(*index);
                res = self._find_eval(position, parent, &section.previous_indexes, acc);
            },
            SectionIndex::OR(indexes) => {
                for index in indexes.iter() {
                    res.extend(self._find_eval(position, parent, index, acc));
                }
            }
        }
        res
    }

    pub fn get_evaluations(&self, position: TextSize) -> Vec<&Evaluation> {
        let mut res = vec![];
        if let Some(parent) = self.parent.as_ref() {
            if let Some(parent) = parent.upgrade() {
                let parent = parent.borrow();
                let section = &parent.symbols.unwrap().get_section_for(position);
                let evaluations = self._find_eval(position, &parent, &SectionIndex::INDEX(section.index), &mut vec![]);
            } else {
                warn!("Parent must be available to get evaluations");
            }
        } else {
            warn!("Parent must be available to get evaluations");
        }
        res
    }
}