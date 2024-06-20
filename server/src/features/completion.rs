use std::{cell::RefCell, rc::Rc};
use lsp_types::{CompletionItem, CompletionList, CompletionResponse};

use crate::threads::SessionInfo;
use crate::S;
use crate::core::odoo::SyncOdoo;
use crate::core::symbol::Symbol;
use crate::core::file_mgr::FileInfo;



pub struct CompletionFeature;

impl CompletionFeature {

    pub fn autocomplete(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<CompletionResponse> {


        Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items: vec![
                CompletionItem {
                    label: S!("test"),
                    ..Default::default()
                }
            ]
        }))
    }
}