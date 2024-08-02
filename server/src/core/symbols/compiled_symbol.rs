use crate::{constants::SymType, threads::SessionInfo};
use std::{cell::{RefCell, RefMut}, rc::Weak};

use super::symbol::MainSymbol;

#[derive(Debug)]
pub struct CompiledSymbol {
    pub name: String,
    pub is_external: bool,
    pub path: String,
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
}

impl CompiledSymbol {

    pub fn new(name: String, path: String, is_external: bool) -> Self {
        Self {
            name,
            is_external,
            weak_self:None,
            path,
            parent: None,
        }
    }

}