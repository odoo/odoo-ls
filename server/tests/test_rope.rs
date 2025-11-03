use std::{cell::RefCell, rc::Rc};

use lsp_types::Position;
use odoo_ls_server::core::file_mgr::FileInfo;

mod setup;

fn rope_test_simple_text(text: &str) -> Rc<RefCell<FileInfo>> {
    let mut sync_odoo = setup::setup::setup_server(false);
    let mut session = setup::setup::create_session(&mut sync_odoo);

    let file_mgr = session.sync_odoo.get_file_mgr();
    let mut file_mgr = file_mgr.borrow_mut();
    let (updated, file_info) = file_mgr.update_file_info(&mut session, "test.py", Some(&vec![
        lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: text.to_string(),
        }
    ]), Some(1), false);
    return file_info.clone();
}

#[test]
fn test_rope_single_line_ascii() {
    let file_info = rope_test_simple_text("a = 1");
    let file_info = file_info.borrow();
    assert_eq!(file_info.offset_to_position(0), Position { line: 0, character: 0 });
    assert_eq!(file_info.offset_to_position(1), Position { line: 0, character: 1 });
    assert_eq!(file_info.offset_to_position(2), Position { line: 0, character: 2 });
    assert_eq!(file_info.offset_to_position(3), Position { line: 0, character: 3 });
    assert_eq!(file_info.offset_to_position(4), Position { line: 0, character: 4 });
    assert_eq!(file_info.offset_to_position(5), Position { line: 0, character: 5 });
    assert_eq!(file_info.position_to_offset(0, 0), 0);
    assert_eq!(file_info.position_to_offset(0, 1), 1);
    assert_eq!(file_info.position_to_offset(0, 2), 2);
    assert_eq!(file_info.position_to_offset(0, 3), 3);
    assert_eq!(file_info.position_to_offset(0, 4), 4);
    assert_eq!(file_info.position_to_offset(0, 5), 5);
}

#[test]
fn test_rope_single_line_utf16() {
    let file_info = rope_test_simple_text("a🧩b");
    let file_info = file_info.borrow();
    assert_eq!(file_info.offset_to_position(0), Position { line: 0, character: 0 });
    assert_eq!(file_info.offset_to_position(1), Position { line: 0, character: 1 }); // puzzle takes 4 bytes \xf0\x9f\xa7\xa9
    assert_eq!(file_info.offset_to_position(2), Position { line: 0, character: 1 });
    assert_eq!(file_info.offset_to_position(3), Position { line: 0, character: 1 });
    assert_eq!(file_info.offset_to_position(4), Position { line: 0, character: 1 });
    assert_eq!(file_info.offset_to_position(5), Position { line: 0, character: 3 });
    assert_eq!(file_info.offset_to_position(6), Position { line: 0, character: 4 });
    assert_eq!(file_info.position_to_offset(0, 0), 0);
    assert_eq!(file_info.position_to_offset(0, 1), 1);
    assert_eq!(file_info.position_to_offset(0, 2), 5);
    assert_eq!(file_info.position_to_offset(0, 3), 6);
}