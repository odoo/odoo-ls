use std::{cell::RefCell, rc::{Rc, Weak}};

use lsp_types::Diagnostic;
use ruff_text_size::TextRange;

use crate::{constants::{BuildStatus}, threads::SessionInfo};

use super::{evaluation::{Evaluation, SymbolRef}, model::ModelData, symbols::symbol::Symbol, symbol_location::SymbolLocation, symbols::{class_symbol::ClassSymbol, function_symbol::FunctionSymbol}};


#[derive(Debug)]
pub struct LocalizedSymbol {
    pub loc_sym_type: String,
    pub symbol: Weak<RefCell<Symbol>>, //owner
    pub doc_string: Option<String>,
    pub ast_indexes: Option<Vec<u16>>, //list of index to reach the corresponding ast node from file ast
    pub validation_status: BuildStatus,

    pub _function: Option<FunctionSymbol>,
    pub _class: Option<ClassSymbol>,
    pub _model: Option<ModelData>,
}

impl LocalizedSymbol {
    pub fn new(owner: Weak<RefCell<Symbol>>, loc_sym_type: String, range: TextRange) -> Self {
        Self {
            symbol: owner,
            symbols: None,
            loc_sym_type,
            range: range,
            is_import_variable: false,
            doc_string: None,
            ast_indexes: None,
            evaluations: vec![],
            validation_status: BuildStatus::PENDING,

            _function: None,
            _class: None,
            _model: None,
        }
    }

    pub fn get_module_sym(&self) -> Option<Rc<RefCell<Symbol>>> {
        self.symbol.upgrade().unwrap().borrow().get_module_sym().clone()
    }

    pub fn is_type_alias(&self) -> bool {
        return self.evaluations.len() >= 1 && self.evaluations.iter().all(|x| !x.symbol.instance) && !self.is_import_variable;
    }

    pub fn to_symbol_ref(&self) -> SymbolRef {
        SymbolRef::from(self)
    }

    ///Return the symbol owning this LocalizedSymbol. Panic if not available.
    pub fn symbol(&self) -> Rc<RefCell<Symbol>> {
        self.symbol.upgrade().unwrap()
    }

    /* similar to get_symbol: will return the symbol that is under this one with the specified name.
    However, if the symbol is a class or a model, it will search in the base class or in comodel classes
    if not all, it will return the first found. If all, the all found symbols are returned, but the first one
    is the one that is overriding others.
    :param: from_module: optional, can change the from_module of the given class */
    pub fn get_member_symbol(&self, session: &mut SessionInfo, name: &String, from_module: Option<Rc<RefCell<Symbol>>>, prevent_comodel: bool, all: bool, diagnostics: &mut Vec<Diagnostic>) -> Vec<SymbolRef> {
        let mut result: Vec<SymbolRef> = vec![];
        let sym = self.symbol.upgrade().unwrap();
        let sym = sym.borrow();
        if sym.module_symbols.contains_key(name) {
            if all {
                result.push(sym.module_symbols[name].borrow().to_sym_ref());
            } else {
                return vec![sym.module_symbols[name].borrow().to_sym_ref()];
            }
        }
        if sym.symbols.as_ref().unwrap().symbols().contains_key(name) {
            if all {
                result.push(sym.symbols.as_ref().unwrap().symbols()[name].borrow().to_sym_ref());
            } else {
                return vec![sym.symbols.as_ref().unwrap().symbols()[name].borrow().to_sym_ref()];
            }
        }
        if self._model.is_some() && !prevent_comodel {
            let model = session.sync_odoo.models.get(&self._model.as_ref().unwrap().name);
            if let Some(model) = model {
                let loc_symbols = model.clone().borrow().get_symbols(session, from_module.clone().unwrap_or(self.get_module_sym().expect("unable to find module")));
                for loc_sym in loc_symbols {
                    if self.is_equal(&loc_sym) {
                        continue;
                    }
                    let attribut = loc_sym.borrow().get_member_symbol(session, name, None, true, all, diagnostics);
                    if all {
                        result.extend(attribut);
                    } else {
                        return attribut;
                    }
                }
            }
        }
        if !all && result.len() != 0 {
            return result;
        }
        if self._class.is_some() {
            for base in self._class.as_ref().unwrap().bases.iter() {
                let s = base.borrow().get_member_symbol(session, name, from_module.clone(), prevent_comodel, all, diagnostics);
                if s.len() != 0 {
                    if all {
                        result.extend(s);
                    } else {
                        return s;
                    }
                }
            }
        }
        result
    }

    pub fn is_equal(&self, other: &Rc<RefCell<LocalizedSymbol>>) -> bool {
        if Weak::ptr_eq(&self.symbol, &other.borrow().symbol) {
            return self.range == other.borrow().range;
        }
        false
    }

    ///Return last declarations of LocalizedSymbols that are in the range of this LocalizedSymbol
    pub fn get_loc_symbol(&self, names: Vec<String>) -> Vec<Rc<RefCell<LocalizedSymbol>>> {
        let symbol = self.symbol();
        let symbol = symbol.borrow();
        let child = symbol.get_symbol(&(vec![], names));
        if let Some(child) = child {
            return child.borrow().get_loc_sym(self.range.end().to_u32());
        }
        vec![]
    }
}
