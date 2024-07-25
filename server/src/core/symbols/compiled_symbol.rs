use crate::{constants::SymType, threads::SessionInfo};
use std::{cell::{RefCell, RefMut}, rc::Weak};

use super::symbol::MainSymbol;

#[derive(Debug)]
pub struct CompiledSymbol {
    pub name: String,
    pub weak_self: Option<Weak<RefCell<MainSymbol>>>,
    pub parent: Option<Weak<RefCell<MainSymbol>>>,
}

impl CompiledSymbol {

    pub fn new(name: String) -> Self {
        Self {
            name,
            weak_self:None,
            parent: None
        }
    }

}