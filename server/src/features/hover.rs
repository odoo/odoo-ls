use tower_lsp::lsp_types::Hover;
use crate::core::file_mgr::FileInfo;
use tower_lsp::jsonrpc::Result;
use std::rc::Rc;
use crate::core::symbol::Symbol;
use std::cell::RefCell;

pub struct HoverFeature {}

impl HoverFeature {

    pub fn get_hover(file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Result<Option<Hover>> {
        let offset = file_info.borrow().position_to_offset(line, character);
        //let (symbol, effective_sym, factory, range, context) = AstUtils::get_symbols(file_symbol, ast, offset);
            
        todo!()
    }
}