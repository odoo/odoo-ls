use std::collections::HashMap;

use ruff_text_size::TextSize;

use crate::{constants::OYarn, core::symbols::symbol_keys::SymbolKey};
use crate::utils::NoHashBuilder;

use super::{class_symbol::ClassSymbol, file_symbol::FileSymbol, function_symbol::FunctionSymbol, module_symbol::ModuleSymbol, package_symbol::PythonPackageSymbol};

#[derive(Debug, Default)]
pub struct ContentSymbols {
    pub symbols: Vec<SymbolKey>,
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
    fn get_sections(&self) -> &[SectionRange];
    fn symbols(&self) -> &HashMap<OYarn, HashMap<u32, Vec<SymbolKey>, NoHashBuilder>>;
    fn get_section_for(&self, position: u32) -> SectionRange;
    fn get_last_index(&self) -> u32;
    fn add_section(&mut self, range_start: TextSize, maybe_previous_indexes: Option<SectionIndex>) -> SectionRange;
    fn change_parent(&mut self, new_parent: SectionIndex, section: &mut SectionRange);
    fn _init_symbol_mgr(&mut self);
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

        fn get_sections(&self) -> &[SectionRange] {
            &self.sections
        }

        fn symbols(&self) -> &HashMap<OYarn, HashMap<u32, Vec<SymbolKey>, NoHashBuilder>> {
            &self.symbols
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
    }
)+)
}

impl_section_mgr_for!(FileSymbol, ClassSymbol, FunctionSymbol, ModuleSymbol, PythonPackageSymbol);

pub fn iter_symbol_keys(symbol: &impl SymbolMgr) -> impl Iterator<Item = & SymbolKey> {
    symbol.symbols().values()
        .flat_map(|section| section.values())
        .flat_map(|symbol_list| symbol_list.iter())
}
