use std::{cell::RefCell, rc::Rc};

use lsp_types::{DocumentSymbol, DocumentSymbolResponse, Range, SymbolKind};

use crate::{core::{file_mgr::FileInfo, symbols::symbol::Symbol}, threads::SessionInfo};


pub struct DocumentSymbolFeature;

impl DocumentSymbolFeature {

    pub fn get_symbols(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>) -> Option<DocumentSymbolResponse> {
        let symbols = DocumentSymbolFeature::get_symbols_recursive(session, file_symbol, file_info);
        if symbols.is_empty() {
            return None;
        }
        Some(DocumentSymbolResponse::Nested(symbols))
    }

    fn get_symbols_recursive(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>) -> Vec<DocumentSymbol> {
        let mut res = vec![];
        if !matches!(*symbol.borrow(), Symbol::Class(_) | Symbol::Function(_) | Symbol::File(_) | Symbol::Package(_)) {
            return res;
        }
        for sym in symbol.borrow().iter_symbols().flat_map(|(name, hashmap)| hashmap.into_iter().flat_map(|(_, vec)| vec.clone())) {
            let children = DocumentSymbolFeature::get_symbols_recursive(session, &sym, file_info);
            let sym_bw = sym.borrow();
            let doc_sym = DocumentSymbol{
                name: sym_bw.name().clone(),
                detail: None, //TODO provide signature?
                kind: DocumentSymbolFeature::get_symbol_kind(&sym_bw),
                tags: None,
                deprecated: None,
                range: Range{
                    start: file_info.borrow().offset_to_position(sym_bw.range().start().to_usize()),
                    end: file_info.borrow().offset_to_position(sym_bw.range().end().to_usize()),
                },
                selection_range: Range{
                    start: file_info.borrow().offset_to_position(sym_bw.range().start().to_usize()),
                    end: file_info.borrow().offset_to_position(sym_bw.range().end().to_usize()),
                },
                children: match children.is_empty() {
                    true => None,
                    false => Some(children)
                }
            };
            res.push(doc_sym);
        }
        res
    }

    fn get_symbol_kind(sym_bw: &Symbol) -> SymbolKind {
        match sym_bw {
            Symbol::Root(_) => panic!("Root symbol should not be in the document symbols"),
            Symbol::Namespace(_) => panic!("Namespace symbol should not be in the document symbols"),
            Symbol::Package(_) => panic!("Package symbol should not be in the document symbols"),
            Symbol::File(_) => panic!("File symbol should not be in the document symbols"),
            Symbol::Compiled(_) => panic!("Compiled symbol should not be in the document symbols"),
            Symbol::Class(_) => SymbolKind::CLASS,
            Symbol::Function(_) => SymbolKind::FUNCTION, //TODO could be more precise
            Symbol::Variable(_) => SymbolKind::VARIABLE, //TODO could be more precise
        }
    }

}