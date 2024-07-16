use std::{cell::RefCell, rc::Rc, collections::HashMap};

use ruff_text_size::TextRange;

use super::{class_symbol::ClassSymbol, file_symbol::FileSymbol, function_symbol::FunctionSymbol, module_symbol::ModuleSymbol, package_symbol::PythonPackageSymbol, symbol::Symbol};


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

pub trait SymbolMgr {
    fn get_section_for(&self, position: u32) -> SectionRange;
    fn add_section(&mut self, range: TextRange) -> SectionRange;
    fn change_parent(&mut self, new_parent: SectionIndex, section: &mut SectionRange);
    fn get_symbol(&self, name: String, position: u32) -> Vec<Rc<RefCell<Symbol>>>;
    fn _init_symbol_mgr(&mut self);
    fn _get_loc_symbol(&self, map: &HashMap<u32, Vec<Rc<RefCell<Symbol>>>>, position: u32, index: &SectionIndex, acc: &mut Vec<u32>) -> Vec<Rc<RefCell<Symbol>>>;
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

macro_rules! impl_section_mgr_for {
    ($($t:ty),+ $(,)?) => ($(
    impl SymbolMgr for $t {
        fn _init_symbol_mgr(&mut self) {
            self.sections.push(SectionRange{
                start: 0,
                index: 0,
                previous_indexes: SectionIndex::NONE
            });
        }

        fn get_section_for(&self, position: u32) -> SectionRange {
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
        fn add_section(&mut self, range: TextRange) -> SectionRange {
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

        fn change_parent(&mut self, new_parent: SectionIndex, section: &mut SectionRange) {
            section.previous_indexes = new_parent;
        }

        ///Return all the symbols that are valid as last declaration for the given position
        fn get_symbol(&self, name: String, position: u32) -> Vec<Rc<RefCell<Symbol>>> {
            let sections: Option<&HashMap<u32, Vec<Rc<RefCell<Symbol>>>>> = self.symbols.get(&name);
            if let Some(sections) = sections {
                let section: SectionRange = self.get_section_for(position);
                return self._get_loc_symbol(sections, position, &SectionIndex::INDEX(section.index), &mut vec![]);
            }
            vec![]
        }

        ///given all the sections of a symbol and a position, return all the Symbols that can represent the symbol
        fn _get_loc_symbol(&self, map: &HashMap<u32, Vec<Rc<RefCell<Symbol>>>>, position: u32, index: &SectionIndex, acc: &mut Vec<u32>) -> Vec<Rc<RefCell<Symbol>>> {
            let mut res = vec![];
            match index {
                SectionIndex::NONE => { return res; },
                SectionIndex::INDEX(index) => {
                    let section = self.sections.get(*index as usize).unwrap();
                    //take index and try to find an evaluation. if no evaluation is found, search in previous index, and mix evaluation if there is multiple precedences
                    for loc_sym in map.get(index).unwrap().iter().rev() {
                        if loc_sym.borrow().range().start().to_u32() < position {
                            res.push(loc_sym.clone());
                            break;
                        }
                    }
                    if !res.is_empty() {
                        return res;
                    }
                    acc.push(*index);
                    res = self._get_loc_symbol(map, position, &section.previous_indexes, acc);
                },
                SectionIndex::OR(indexes) => {
                    for index in indexes.iter() {
                        res.extend(self._get_loc_symbol(map, position, index, acc));
                    }
                }
            }
            res
        }

    }
)+)
}

impl_section_mgr_for!(FileSymbol, ClassSymbol, FunctionSymbol, ModuleSymbol, PythonPackageSymbol);