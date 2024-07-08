use std::{cell::RefCell, rc::Rc};
use lsp_types::{CompletionItem, CompletionList, CompletionResponse};

use crate::core::evaluation::ExprOrIdent;
use crate::threads::SessionInfo;
use crate::S;
use crate::core::symbol::Symbol;
use crate::core::file_mgr::FileInfo;

use super::ast_utils::ExprFinderVisitor;



pub struct CompletionFeature;

impl CompletionFeature {

    pub fn autocomplete(session: &mut SessionInfo,
        file_symbol: &Rc<RefCell<Symbol>>,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Option<CompletionResponse> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let file_info =  file_info.borrow();
        let ast = file_info.ast.as_ref().unwrap();
        let mut expr: Option<ExprOrIdent> = None;
        for stmt in ast.iter() {
            println!("{:?}", stmt);
            expr = ExprFinderVisitor::find_expr_at(stmt, offset as u32);
            if expr.is_some() {
                break;
            }
        }
        

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