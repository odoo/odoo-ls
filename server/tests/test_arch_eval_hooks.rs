use std::env;
use std::path::Path;

use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol_keys::SymbolKey;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::tree::TreeStrSlice;
use odoo_ls_server::utils::PathSanitizer;
use tracing::warn;
//use tracing::{warn};

mod setup;
mod test_utils;

fn names(session: &SessionInfo, syms: &[SymbolKey]) -> Vec<String> {
    syms.iter().map(|&s| session.st().name(s).to_string()).collect()
}

/// `BaseModel.sudo/with_env/with_company/with_context/with_prefetch/filtered/
/// filtered_domain/exists/browse` are all plain Python methods who
/// evaluate to `Self` (see `PythonArchEvalHooks` entries using
/// `Evaluation::new_self`).
#[test]
fn test_self_returning_chain_hooks() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("arch_eval_hooks_model.py").sanitize();
    assert!(Path::new(&test_file).exists(), "Test file does not exist: {}", test_file);
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol for {}", test_file);
    };

    let arch_eval_hooks_model = session.st().get_symbol(file_symbol.into(), (&[], &["ArchEvalHooksModel"]), u32::MAX);
    assert_eq!(arch_eval_hooks_model.len(), 1, "Expected to find the `ArchEvalHooksModel` class");
    let arch_eval_hooks_model = arch_eval_hooks_model[0];

    // (line, char) of the assignment target for each `self.<method>(...)` call
    // in `ArchEvalHooksModel.chain_methods`; every one of them should keep
    // resolving to `ArchEvalHooksModel` itself.
    let cases: &[(u32, u32, &str)] = &[
        (11, 8, "sudo"),
        (12, 8, "with_env"),
        (13, 8, "with_company"),
        (14, 8, "with_context"),
        (15, 8, "with_prefetch"),
        (16, 8, "filtered"),
        (17, 8, "filtered_domain"),
        (18, 8, "exists"),
        (19, 8, "browse"),
        (20, 8, "with_user"),
        (21, 8, "create"),
        (22, 8, "search"),
    ];
    for &(line, character, method) in cases {
        let resolved = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, line, character);
        assert!(
            resolved.contains(&arch_eval_hooks_model),
            "`{}()` should keep evaluating to `ArchEvalHooksModel` (self-returning hook). Got: {:?}",
            method,
            names(&session, &resolved)
        );
    }

    // `BaseModel.__iter__`: reuses the pre-existing `for var in self:` loop in
    // module_1/models/base_test_models.py (line 23, 0-indexed 22)
    let base_test_file = test_addons_path.join("module_1").join("models").join("base_test_models.py").sanitize();
    let base_file_info = file_mgr.borrow().get_file_info(&base_test_file).unwrap();
    let Some(base_file_symbol) = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&base_test_file)) else {
        panic!("Failed to get file symbol for {}", base_test_file);
    };
    let base_test_model = session.st().get_symbol(base_file_symbol.into(), (&[], &["BaseTestModel"]), u32::MAX);
    assert_eq!(base_test_model.len(), 1, "Expected to find the `BaseTestModel` class");
    let base_test_model = base_test_model[0];

    let resolved_iter_var = test_utils::get_resolved_symbols_at_position(&mut session, base_file_symbol, &base_file_info, 22, 13);
    assert!(
        resolved_iter_var.contains(&base_test_model),
        "`for var in self:` should type `var` as `BaseTestModel` (BaseModel.__iter__ hook). Got: {:?}",
        names(&session, &resolved_iter_var)
    );
}

//Test self.env.registry hook
#[test]
fn test_registry_hooks() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("arch_eval_hooks_model.py").sanitize();
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = Path::new(&odoo_path).sanitize();
    let partner_class_name = test_utils::PARTNER_CLASS_NAME(session.sync_odoo.version);

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol for {}", test_file);
    };

    let partner_class = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "addons", "base", "models", "res_partner"], &[partner_class_name]), u32::MAX);
    assert_eq!(partner_class.len(), 1, "Expected to find the `{}` model class", partner_class_name);
    let partner_class = partner_class[0];

    let registry_class = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "orm", "registry"], &["Registry"]), u32::MAX);
    assert_eq!(registry_class.len(), 1, "Expected to find the `Registry` class");
    let registry_class = registry_class[0];

    // `registry_getitem_res = self.env.registry["res.partner"]` (line 24, 0-indexed 23)
    let resolved_getitem = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 26, 8);
    assert!(
        resolved_getitem.contains(&partner_class),
        "`self.env.registry[\"res.partner\"]` should evaluate to the `{}` model class. Got: {:?}",
        partner_class_name,
        names(&session, &resolved_getitem)
    );

    // `registry_prop_res = self.env.registry` (line 25, 0-indexed 24)
    let resolved_prop = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 27, 8);
    assert!(resolved_prop.len() == 1);
    assert!(
        resolved_prop.contains(&registry_class),
        "`self.env.registry` should evaluate to a `Registry` instance. Got: {:?}",
        names(&session, &resolved_prop)
    );
}

/// Test hook on BaseModel.id and ids
#[test]
fn test_ids_and_id_field_hooks() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("arch_eval_hooks_model.py").sanitize();
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol for {}", test_file);
    };

    let list_type = session.sync_odoo.get_symbol("", (&["builtins"], &["list"]), u32::MAX);
    assert_eq!(list_type.len(), 1, "Expected to find the `list` builtin");
    let list_type = list_type[0];
    let int_type = session.sync_odoo.get_symbol("", (&["builtins"], &["int"]), u32::MAX);
    assert_eq!(int_type.len(), 1, "Expected to find the `int` builtin");
    let int_type = int_type[0];

    // `ids_res = self.ids` (line 32, 0-indexed 30)
    let resolved_ids = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 31, 8);
    assert!(resolved_ids.len() == 1);
    assert!(
        resolved_ids.contains(&list_type),
        "`self.ids` should evaluate to a `list` (BaseModel.ids hook). Got: {:?}",
        names(&session, &resolved_ids)
    );

    // `id_res = self.id` (line 31, 0-indexed 30)
    let resolved_id = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 32, 8);
    assert!(
        resolved_id.contains(&int_type),
        "`self.id` should evaluate to `int` (Id.__get__ hook). Got: {:?}",
        names(&session, &resolved_id)
    );
}

/// `odoo/init.py` dynamically assigns `odoo.SUPERUSER_ID`, `odoo.Command`,
/// `odoo._` and `odoo._lt` at runtime (`odoo.SUPERUSER_ID = SUPERUSER_ID`,
/// etc in `odoo/init.py`), which static analysis of `odoo/__init__.py`
/// alone cannot discover. The corresponding file hooks wire the top-level
/// `odoo.<name>` attribute to the real symbol imported in `odoo/init.py`.
#[test]
fn test_odoo_init_symbol_hooks() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("arch_eval_hooks_model.py").sanitize();
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = Path::new(&odoo_path).sanitize();

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol for {}", test_file);
    };

    // (name as bound in `odoo/init.py`, (line, char) of the assignment target, is it called?)
    let cases: &[(&str, u32, u32, bool)] = &[
        ("SUPERUSER_ID", 36, 8, false),
        ("Command", 37, 8, false),
        ("_lt", 38, 8, false),
        ("_", 39, 8, true),
    ];
    for &(init_name, line, character, is_call) in cases {
        let init_sym = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "init"], &[init_name]), u32::MAX);
        assert_eq!(init_sym.len(), 1, "Expected to find `odoo.init.{}`", init_name);
        let mut expected = test_utils::resolve_symbol_types(&mut session, init_sym[0]);
        if is_call {
            // `init_sym` resolves to the function itself; resolve one more hop to get
            // what calling it returns.
            assert_eq!(expected.len(), 1, "Expected `odoo.init.{}` to resolve to a single function", init_name);
            expected = test_utils::resolve_symbol_types(&mut session, expected[0]);
        }
        assert!(!expected.is_empty(), "Expected `odoo.init.{}` to resolve to a type", init_name);

        let actual = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, line, character);
        assert!(
            expected.iter().any(|sym| actual.contains(sym)),
            "`odoo.{}` usage at {}:{} should evaluate like `odoo.init.{}`. Expected one of {:?}, got {:?}",
            init_name, line, character, init_name,
            names(&session, &expected),
            names(&session, &actual)
        );
    }
}

/// `ir.rule`'s `global` field cannot be declared as a normal class attribute
/// because `global` is a Python keyword: Odoo declares it with
/// `setattr(IrRule, 'global', fields.Boolean(...))` at module level instead
/// (`odoo/addons/base/models/ir_rule.py`).
/// Can only be tested on Odoo versions < 19.4, where the `ir.rule` model was still present.
#[test]
fn test_ir_rule_global_field_hook() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("arch_eval_hooks_model.py").sanitize();
    let mut session = setup::setup::create_init_session(&mut odoo, config);
    if session.sync_odoo.version >= (19, 4) {
        warn!("Skipping test ir_rule_global_field_hook because Odoo version is >= 19.4, which is not in the range of the hook.");
        return;
    }

    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = Path::new(&odoo_path).sanitize();

    // The synthesized `IrRule.global` member should exist...
    let ir_rule_file = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "addons", "base", "models", "ir_rule"], &[]), u32::MAX);
    assert_eq!(ir_rule_file.len(), 1, "Expected to find the ir_rule.py file symbol");
    let global_sym = session.st().get_symbol(ir_rule_file[0], (&[], &["IrRule", "global"]), u32::MAX);
    assert_eq!(global_sym.len(), 1, "Expected the arch builder to synthesize a single `IrRule.global` member");

    // ...and the eval hook should have wired its evaluation to the real `Boolean` field class.
    let boolean_class = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "orm", "fields_misc"], &["Boolean"]), u32::MAX);
    assert_eq!(boolean_class.len(), 1, "Expected to find the `Boolean` field class");
    let resolved = test_utils::resolve_symbol_types(&mut session, global_sym[0]);
    assert!(
        resolved.contains(&boolean_class[0]),
        "Expected `IrRule.global`'s evaluation to resolve to the `Boolean` field class. Got: {:?}",
        names(&session, &resolved)
    );

    // `self.env["ir.rule"].search([("global", "=", True)])` (line 41, 0-indexed 40)
    // should raise no diagnostic now that `global` is a recognized field.
    let diagnostics = setup::setup::get_diagnostics_for_path(&mut session, &test_file);
    let line_diagnostics = test_utils::diag_on_line(&diagnostics, 43);
    assert!(
        line_diagnostics.is_empty(),
        "Expected no diagnostics on the `global` domain search, got: {:?}",
        line_diagnostics
    );
}

/// Test hook on `BaseModel.env`
#[test]
fn test_base_model_env_hook() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("arch_eval_hooks_model.py").sanitize();
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = Path::new(&odoo_path).sanitize();

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol for {}", test_file);
    };

    let env_tree: TreeStrSlice = if session.sync_odoo.version >= (18, 1) {
        (&["odoo", "orm", "environments"], &["Environment"])
    } else {
        (&["odoo", "api"], &["Environment"])
    };
    let env_class = session.sync_odoo.get_symbol(&odoo_path, env_tree, u32::MAX);
    assert_eq!(env_class.len(), 1, "Expected to find the `Environment` class");
    let env_class = env_class[0];

    // `env_res = self.env` (line 69, 0-indexed 68)
    let resolved = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 68, 8);
    assert!(
        resolved.contains(&env_class),
        "`self.env` should evaluate to the `Environment` class (BaseModel.env hook). Got: {:?}",
        names(&session, &resolved)
    );
}

/// Scalar `Field.__get__` hooks (`Boolean`, `Float`, `Monetary`, `Char`, `Text`,
/// `Html`, `Date`, `Datetime`, `Binary`, `Image`, `Selection`, `Reference`,
/// `Json`, `Properties`, `PropertiesDefinition`) resolve field access to the
/// matching builtin/stdlib type (see `update_get_eval_func_level` calls in
/// `PythonArchEvalHooks`).
#[test]
fn test_scalar_field_get_hooks() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("arch_eval_hooks_model.py").sanitize();
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol for {}", test_file);
    };

    // (line, char) of the assignment target, field kind, tree of the expected type
    let cases: &[(u32, u32, &str, TreeStrSlice)] = &[
        (72, 8, "Boolean", (&["builtins"], &["bool"])),
        (73, 8, "Float", (&["builtins"], &["float"])),
        (74, 8, "Monetary", (&["builtins"], &["float"])),
        (75, 8, "Char", (&["builtins"], &["str"])),
        (76, 8, "Text", (&["builtins"], &["str"])),
        (77, 8, "Html", (&["markupsafe"], &["Markup"])),
        (78, 8, "Date", (&["datetime"], &["date"])),
        (79, 8, "Datetime", (&["datetime"], &["datetime"])),
        (80, 8, "Binary", (&["builtins"], &["bytes"])),
        (81, 8, "Image", (&["builtins"], &["bytes"])),
        (82, 8, "Selection", (&["builtins"], &["str"])),
        (83, 8, "Reference", (&["builtins"], &["str"])),
        (84, 8, "Json", (&["builtins"], &["object"])),
        (85, 8, "Properties", (&["builtins"], &["object"])),
        (86, 8, "PropertiesDefinition", (&["builtins"], &["object"])),
    ];
    for &(line, character, field_kind, expected_tree) in cases {
        let expected = session.sync_odoo.get_symbol("", expected_tree, u32::MAX);
        assert_eq!(expected.len(), 1, "Expected to find the `{}` type", expected_tree.1.last().unwrap());
        let expected = expected[0];

        let resolved = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, line, character);
        assert!(
            resolved.contains(&expected),
            "`{}` field access should evaluate to `{}` ({}.__get__ hook). Got: {:?}",
            field_kind, expected_tree.1.last().unwrap(), field_kind,
            names(&session, &resolved)
        );
    }
}

/// Relational `Field.__get__` hooks for `One2many`/`Many2many` resolve to
/// the comodel's class (`eval_relational`, driven by the `ComodelName`
/// context set up by `update_field_init_func_level`).
#[test]
fn test_relational_field_get_hooks() {
    let (mut odoo, config) = setup::setup::setup_server(true);
    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    let test_file = test_addons_path.join("module_1").join("models").join("arch_eval_hooks_model.py").sanitize();
    let mut session = setup::setup::create_init_session(&mut odoo, config);

    let odoo_path = env::var("COMMUNITY_PATH").unwrap();
    let odoo_path = Path::new(&odoo_path).sanitize();
    let partner_class_name = test_utils::PARTNER_CLASS_NAME(session.sync_odoo.version);

    let file_mgr = session.sync_odoo.get_file_mgr();
    let file_info = file_mgr.borrow().get_file_info(&test_file).unwrap();
    let Some(file_symbol) = SyncOdoo::get_symbol_of_opened_file(&mut session, Path::new(&test_file)) else {
        panic!("Failed to get file symbol for {}", test_file);
    };

    let arch_eval_hooks_model = session.st().get_symbol(file_symbol.into(), (&[], &["ArchEvalHooksModel"]), u32::MAX);
    assert_eq!(arch_eval_hooks_model.len(), 1, "Expected to find the `ArchEvalHooksModel` class");
    let arch_eval_hooks_model = arch_eval_hooks_model[0];

    let partner_class = session.sync_odoo.get_symbol(&odoo_path, (&["odoo", "addons", "base", "models", "res_partner"], &[partner_class_name]), u32::MAX);
    assert_eq!(partner_class.len(), 1, "Expected to find the `{}` model class", partner_class_name);
    let partner_class = partner_class[0];

    // `child_ids_res = self.child_ids` (line 93, 0-indexed 92): One2many("...ArchEvalHooksModel...", "parent_id")
    let resolved_child_ids = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 92, 8);
    assert!(
        resolved_child_ids.contains(&arch_eval_hooks_model),
        "`self.child_ids` (One2many) should evaluate to the `ArchEvalHooksModel` comodel class. Got: {:?}",
        names(&session, &resolved_child_ids)
    );

    // `partner_ids_res = self.partner_ids` (line 94, 0-indexed 93): Many2many("res.partner")
    let resolved_partner_ids = test_utils::get_resolved_symbols_at_position(&mut session, file_symbol, &file_info, 93, 8);
    assert!(
        resolved_partner_ids.contains(&partner_class),
        "`self.partner_ids` (Many2many) should evaluate to the `{}` comodel class. Got: {:?}",
        partner_class_name,
        names(&session, &resolved_partner_ids)
    );
}
