use std::{cell::RefCell, rc::Rc, collections::{HashMap, HashSet}};
use byteyarn::Yarn;

use ruff_text_size::TextSize;

use crate::constants::OYarn;

use super::{class_symbol::ClassSymbol, file_symbol::FileSymbol, function_symbol::FunctionSymbol, module_symbol::ModuleSymbol, package_symbol::PythonPackageSymbol, symbol::Symbol};

#[derive(Debug, Default)]
pub struct ContentSymbols {
    pub symbols: Vec<Rc<RefCell<Symbol>>>,
    pub always_defined: bool
}

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
    fn get_last_index(&self) -> u32;
    fn add_section(&mut self, range_start: TextSize, maybe_previous_indexes: Option<SectionIndex>) -> SectionRange;
    fn change_parent(&mut self, new_parent: SectionIndex, section: &mut SectionRange);
    fn get_content_symbol(&self, name: OYarn, position: u32) -> ContentSymbols;
    fn _init_symbol_mgr(&mut self);
    fn _get_loc_symbol(&self, map: &HashMap<u32, Vec<Rc<RefCell<Symbol>>>>, position: u32, index: &SectionIndex, acc: &mut HashSet<u32>) -> ContentSymbols;
    fn get_all_visible_symbols(&self, name_prefix: &String, position: u32) -> HashMap<OYarn, Vec<Rc<RefCell<Symbol>>>>;
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
            self.sections.iter().rev().find(|section| section.start <= position).unwrap_or(self.sections.last().unwrap()).clone()
        }

        fn get_last_index(&self) -> u32 {
            (self.sections.len() - 1) as u32
        }

        /* Add a section at the END of the sections */
        fn add_section(&mut self, range_start: TextSize, maybe_previous_indexes: Option<SectionIndex>) -> SectionRange{
            let previous_indexes = maybe_previous_indexes.unwrap_or_else(|| {
                let last_index = self.get_last_index();
                SectionIndex::INDEX(last_index)
            });
            let new_section = SectionRange {
                start: range_start.to_u32(),
                index: self.sections.len() as u32,
                previous_indexes,
            };
            self.sections.push(new_section.clone());
            new_section
        }

        fn change_parent(&mut self, new_parent: SectionIndex, section: &mut SectionRange) {
            section.previous_indexes = new_parent;
        }

        ///Return all the symbols that are valid as last declaration for the given position
        fn get_content_symbol(&self, name: OYarn, position: u32) -> ContentSymbols {
            let sections: Option<&HashMap<u32, Vec<Rc<RefCell<Symbol>>>>> = self.symbols.get(&name);
            let mut content = if let Some(sections) = sections {
                let section: SectionRange = self.get_section_for(position);
                self._get_loc_symbol(sections, position, &SectionIndex::INDEX(section.index), &mut HashSet::new())
            } else {
                ContentSymbols::default()
            };
            let ext_sym = self.get_ext_symbol(&name);
            if ext_sym.len() > 1 {
                content.symbols.extend(ext_sym.iter().cloned());
                content.always_defined = true;
            }
            content
        }

        ///given all the sections of a symbol and a position, return all the Symbols that can represent the symbol
        fn _get_loc_symbol(&self, map: &HashMap<u32, Vec<Rc<RefCell<Symbol>>>>, position: u32, index: &SectionIndex, acc: &mut HashSet<u32>) -> ContentSymbols {
            let mut res = ContentSymbols::default();
            match index {
                SectionIndex::NONE => { return res; },
                SectionIndex::INDEX(index) => {
                    if acc.contains(index){
                        res.always_defined = true;
                        return res;
                    }
                    let section = self.sections.get(*index as usize).unwrap();
                    //take index and try to find an evaluation. if no evaluation is found, search in previous index, and mix evaluation if there is multiple precedences
                    if let Some(symbols) = map.get(index) {
                        for loc_sym in symbols.iter().rev() {
                            if loc_sym.borrow().range().start().to_u32() < position {
                                res.symbols.push(loc_sym.clone());
                                break;
                            }
                        }
                    }
                    acc.insert(*index);
                    if !res.symbols.is_empty() {
                        res.always_defined = true;
                        return res;
                    }
                    res = self._get_loc_symbol(map, position, &section.previous_indexes, acc);
                },
                SectionIndex::OR(indexes) => {
                    if indexes.is_empty(){
                        unreachable!("Or indexes should not be empty")
                    }
                    res.always_defined = true;
                    for index in indexes.iter() {
                        let sub_result = self._get_loc_symbol(map, position, index, acc);
                        res.symbols.extend(sub_result.symbols);
                        res.always_defined = res.always_defined && sub_result.always_defined;
                    }
                }
            }
            res
        }

        fn get_all_visible_symbols(&self, name_prefix: &String, position: u32) -> HashMap<OYarn, Vec<Rc<RefCell<Symbol>>>> {
            let mut result = HashMap::new();
            let current_section = self.get_section_for(position);
            let current_index = SectionIndex::INDEX(current_section.index);

            for (name, section_map) in self.symbols.iter() {
                if !name.starts_with(name_prefix) {
                    continue;
                }
                let mut seen = HashSet::new();
                let content = self._get_loc_symbol(section_map, position, &current_index, &mut seen);

                if !content.symbols.is_empty() {
                    result.insert(name.clone(), content.symbols);
                }
            }
            result
        }
    }
)+)
}

impl_section_mgr_for!(FileSymbol, ClassSymbol, FunctionSymbol, ModuleSymbol, PythonPackageSymbol);