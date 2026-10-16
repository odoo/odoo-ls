use std::path::Path;
use std::{env, fs};

use lsp_types::TextDocumentContentChangeEvent;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::core::symbols::storage::SymbolTable;
use odoo_ls_server::core::symbols::symbol_keys::SymbolKey;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

mod setup;

fn reload(session: &mut SessionInfo, path: &str, text: String, version: i32) {
    let event = [TextDocumentContentChangeEvent { range: None, range_length: None, text }];
    session.sync_odoo.get_file_mgr().borrow_mut().update_file_info(session, path, Some(event.as_slice()), Some(version), false);
    Odoo::update_file_index(session, Path::new(path), "py", false, false);
}

/// Whether `member` resolves on ols.indirect.model as seen from module_indirect_b
fn resolves_from_b(session: &mut SessionInfo, member: &str) -> bool {
    let odoo_path = Path::new(&env::var("COMMUNITY_PATH").unwrap()).sanitize();
    let model = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "addons", "module_indirect_base", "models", "models"], &["IndirectModel"]), u32::MAX)[0];
    let Some(SymbolKey::Module(module_b)) = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "addons", "module_indirect_b"], &[]), u32::MAX).first().copied() else { panic!("module_indirect_b not found") };
    !SymbolTable::get_member_symbol(session, model, member, Some(module_b), false, false, false, false, false).0.is_empty()
}

#[test]
fn test_model_classes_after_edit() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    let addons = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/addons");
    let models_path = addons.join("module_indirect_b/models/models.py").sanitize();
    let models_source = fs::read_to_string(&models_path).unwrap();
    assert!(resolves_from_b(&mut session, "own_method"));
    assert!(!resolves_from_b(&mut session, "edited_field"));
    // the module level lookup warms the class set before the new extension joins the model
    let extended = models_source.replace("from odoo import models", "from odoo import fields, models") + "\n\nwarm = IndirectModelB._description\n\n\nclass IndirectModelEdit(models.Model):\n    _inherit = \"ols.indirect.model\"\n\n    edited_field = fields.Char()\n";
    reload(&mut session, &models_path, extended, 2);
    assert!(resolves_from_b(&mut session, "edited_field"), "a field added by a new _inherit class in module_indirect_b should resolve from it");
    reload(&mut session, &models_path, models_source.clone(), 3);
    assert!(!resolves_from_b(&mut session, "edited_field"), "a field whose _inherit class was removed should no longer resolve");
    assert!(resolves_from_b(&mut session, "own_method"));
    // no class is loaded back, so only the unload can drop the removed classes from the model
    reload(&mut session, &models_path, "from odoo import models\n".to_string(), 4);
    assert!(!resolves_from_b(&mut session, "own_method"), "a method whose class was removed should no longer resolve");
    reload(&mut session, &models_path, models_source.clone(), 5);
    assert!(resolves_from_b(&mut session, "own_method"));
}
