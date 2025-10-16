use assert_fs::prelude::*;
use assert_fs::TempDir;
use odoo_ls_server::utils::PathSanitizer;
use std::collections::HashMap;
use std::collections::HashSet;
use odoo_ls_server::core::config::get_configuration;
use odoo_ls_server::S;

#[test]
fn test_config_entry_single_workspace_with_addons_path() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();
    // Create a subdirectory with __manifest__.py to simulate an Odoo addon
    let addon_dir = ws_folder.child("my_module");
    addon_dir.create_dir_all().unwrap();
    addon_dir.child("__manifest__.py").touch().unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    assert!(config.addons_paths.iter().any(|p| p == &ws_folder.path().sanitize()));
}

#[test]
fn test_config_entry_multiple_workspaces_with_various_addons() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    let ws2 = temp.child("ws2");
    ws1.create_dir_all().unwrap();
    ws2.create_dir_all().unwrap();
    // ws1 has an addon subdir with __manifest__.py
    let addon1 = ws1.child("addon1");
    addon1.create_dir_all().unwrap();
    addon1.child("__manifest__.py").touch().unwrap();
    // ws2 has no addon subdir, so it is not an addon path

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());
    ws_folders.insert(S!("ws2"), ws2.path().sanitize().to_string());

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    assert!(config.addons_paths.iter().any(|p| p == &ws1.path().sanitize()));
    assert!(!config.addons_paths.iter().any(|p| p == &ws2.path().sanitize()));
}

#[test]
fn test_config_entry_with_odoo_path_detection() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace_odoo");
    ws_folder.create_dir_all().unwrap();
    ws_folder.child("odoo").create_dir_all().unwrap();
    ws_folder.child("odoo").child("release.py").touch().unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("odoo_ws"), ws_folder.path().sanitize().to_string());

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    assert!(config.odoo_path.as_ref().map(|p| p == &ws_folder.path().sanitize()).unwrap_or(false));
}

#[test]
fn test_single_odools_toml_config() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Write odools.toml
    let toml_content = r#"
        [[config]]
        name = "default"
        python_path = 'python'
        file_cache = false
        auto_refresh_delay = 1234
    "#;
    ws_folder.child("odools.toml").write_str(toml_content).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let (config_map, config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    assert_eq!(config.python_path, "python");
    assert_eq!(config.file_cache, false);
    assert_eq!(config.auto_refresh_delay, 1234);

    // Check config_file serialization matches
    let config_file_str = config_file.to_html_string();
    assert!(config_file_str.contains("python"));
    assert!(config_file_str.contains("false"));
    assert!(config_file_str.contains("1234"));
}

#[test]
fn test_multiple_odools_toml_shadowing() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Parent odools.toml
    let parent_toml = r#"
        [[config]]
        name = "default"
        python_path = "python3"
        file_cache = true
        auto_refresh_delay = 1111
    "#;
    temp.child("odools.toml").write_str(parent_toml).unwrap();

    // Workspace odools.toml (should shadow parent)
    let ws_toml = r#"
        [[config]]
        name = "default"
        python_path = "python"
        file_cache = false
    "#;
    ws_folder.child("odools.toml").write_str(ws_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let (config_map, config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    // ws_folder/odools.toml should take priority
    assert_eq!(config.python_path, "python");
    assert_eq!(config.file_cache, false);
    // auto_refresh_delay should fall back to parent
    assert_eq!(config.auto_refresh_delay, 1111);

    let config_file_str = config_file.to_html_string();
    assert!(config_file_str.contains("python"));
    assert!(config_file_str.contains("false"));
    assert!(config_file_str.contains("1111"));
}

#[test]
fn test_extends_and_shadowing() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Parent odools.toml with a base config
    let parent_toml = r#"
        [[config]]
        name = "base"
        python_path = "python3"
        file_cache = true
        auto_refresh_delay = 2222

        [[config]]
        name = "default"
        extends = "base"
        python_path = "python"
    "#;
    temp.child("odools.toml").write_str(parent_toml).unwrap();

    // Workspace odools.toml overrides auto_refresh_delay
    let ws_toml = r#"
        [[config]]
        name = "default"
        auto_refresh_delay = 3333
    "#;
    ws_folder.child("odools.toml").write_str(ws_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let (config_map, config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    // Should extend from base, but shadow python_path and auto_refresh_delay
    assert_eq!(config.python_path, "python");
    assert_eq!(config.file_cache, true);
    assert_eq!(config.auto_refresh_delay, 3333);

    let config_file_str = config_file.to_html_string();
    assert!(config_file_str.contains("python"));
    assert!(config_file_str.contains("true"));
    assert!(config_file_str.contains("3333"));
}

#[test]
fn test_workspacefolder_template_variable_variations() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();
    let addon_dir = ws_folder.child("my_module");
    addon_dir.create_dir_all().unwrap();
    addon_dir.child("__manifest__.py").touch().unwrap();

    // Also create a second workspace for :ws2
    let ws2_folder = temp.child("workspace2");
    ws2_folder.create_dir_all().unwrap();
    let addon2_dir = ws2_folder.child("my_module2");
    addon2_dir.create_dir_all().unwrap();
    addon2_dir.child("__manifest__.py").touch().unwrap();

    // Test all template variable forms
    let toml_content = r#"
        [[config]]
        name = "default"
        addons_paths = [
            "${workspaceFolder}",
            "${workspaceFolder:ws1}",
            "${workspaceFolder:ws2}",
            "${workspaceFolder:doesnotexist}",
        ]
    "#;
    ws_folder.child("odools.toml").write_str(toml_content).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());
    ws_folders.insert(S!("ws2"), ws2_folder.path().sanitize().to_string());

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    // "${workspaceFolder}" should resolve to ws1 (the current workspace)
    assert!(config.addons_paths.iter().any(|p| p == &ws_folder.path().sanitize()));
    // "${workspaceFolder:ws1}" should resolve to ws1
    assert!(config.addons_paths.iter().any(|p| p == &ws_folder.path().sanitize()));
    // "${workspaceFolder:ws2}" should resolve to ws2
    assert!(config.addons_paths.iter().any(|p| p == &ws2_folder.path().sanitize()));
    // "${workspaceFolder:doesnotexist}" should NOT resolve to anything
    assert!(!config.addons_paths.iter().any(|p| p.ends_with("doesnotexist")));
}

#[test]
fn test_workspacefolder_template_ws2_in_ws1_add_workspace_addon_path_behavior() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    let ws2 = temp.child("ws2");
    ws1.create_dir_all().unwrap();
    ws2.create_dir_all().unwrap();

    // ws1 has an addon subdir with __manifest__.py
    let addon1 = ws1.child("addon1");
    addon1.create_dir_all().unwrap();
    addon1.child("__manifest__.py").touch().unwrap();

    // ws2 has an addon subdir with __manifest__.py
    let addon2 = ws2.child("addon2");
    addon2.create_dir_all().unwrap();
    addon2.child("__manifest__.py").touch().unwrap();

    // odools.toml in ws1 only references ws2 via template
    let toml_content = r#"
        [[config]]
        name = "default"
        addons_paths = [
            "${workspaceFolder:ws2}"
        ]
    "#;
    ws1.child("odools.toml").write_str(toml_content).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());
    ws_folders.insert(S!("ws2"), ws2.path().sanitize().to_string());

    // By default, ws1 should NOT be added as an addon path, only ws2
    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert!(!config.addons_paths.iter().any(|p| p == &ws1.path().sanitize()));
    assert!(config.addons_paths.iter().any(|p| p == &ws2.path().sanitize()));

    // Now set $autoDetectAddons = true, ws1 should be added as well
    let toml_content_with_flag = r#"
        [[config]]
        name = "default"
        addons_paths = ["$autoDetectAddons"]
    "#;
    ws1.child("odools.toml").write_str(toml_content_with_flag).unwrap();

    let (config_map2, _config_file2) = get_configuration(&ws_folders, &None).unwrap();
    let config2 = config_map2.get("default").unwrap();
    assert!(config2.addons_paths.iter().any(|p| p == &ws1.path().sanitize()));
    assert!(config2.addons_paths.iter().any(|p| p == &ws2.path().sanitize()));
}

#[test]
fn test_config_file_sources_single_file() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    let toml_content = r#"
        [[config]]
        name = "default"
        python_path = "python"
        file_cache = false
        auto_refresh_delay = 1234
    "#;
    let odools_path = ws_folder.child("odools.toml");
    odools_path.write_str(toml_content).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let (_config_map, config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config_entry = &config_file.config[0];

    // All sourced fields should have the odools.toml as their only source
    let odools_src = odools_path.path().sanitize();
    assert_eq!(config_entry.python_path_sourced().unwrap().sources().len(), 1);
    assert!(config_entry.python_path_sourced().unwrap().sources().contains(&odools_src));
    assert_eq!(config_entry.file_cache_sourced().unwrap().sources().len(), 1);
    assert!(config_entry.file_cache_sourced().unwrap().sources().contains(&odools_src));
    assert_eq!(config_entry.auto_refresh_delay_sourced().unwrap().sources().len(), 1);
    assert!(config_entry.auto_refresh_delay_sourced().unwrap().sources().contains(&odools_src));
}

#[test]
fn test_config_file_sources_multiple_files_and_extends() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Parent odools.toml with a base config
    let parent_toml = r#"
        [[config]]
        name = "base"
        python_path = "python3"
        file_cache = true
        auto_refresh_delay = 2222

        [[config]]
        name = "default"
        extends = "base"
        python_path = "python"
    "#;
    let parent_odools = temp.child("odools.toml");
    parent_odools.write_str(parent_toml).unwrap();

    // Workspace odools.toml overrides auto_refresh_delay
    let ws_toml = r#"
        [[config]]
        name = "default"
        auto_refresh_delay = 3333
    "#;
    let ws_odools = ws_folder.child("odools.toml");
    ws_odools.write_str(ws_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let (_config_map, config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config_entry = config_file.config.iter().find(|c| c.name == "default").unwrap();

    // python_path should be sourced from parent odools.toml (root config)
    assert!(config_entry.python_path_sourced().unwrap().sources().contains(&parent_odools.path().sanitize()));
    // file_cache should be sourced from parent odools.toml (base config, via extends)
    assert!(config_entry.file_cache_sourced().unwrap().sources().contains(&parent_odools.path().sanitize()));
    // auto_refresh_delay should be sourced from ws odools.toml (overrides parent)
    assert!(config_entry.auto_refresh_delay_sourced().unwrap().sources().contains(&ws_odools.path().sanitize()));
}

#[test]
fn test_config_file_sources_template_variable_workspacefolder() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    let ws2 = temp.child("ws2");
    ws1.create_dir_all().unwrap();
    ws2.create_dir_all().unwrap();

    // ws1 has an addon subdir with __manifest__.py
    let addon1 = ws1.child("addon1");
    addon1.create_dir_all().unwrap();
    addon1.child("__manifest__.py").touch().unwrap();

    // ws2 has an addon subdir with __manifest__.py
    let addon2 = ws2.child("addon2");
    addon2.create_dir_all().unwrap();
    addon2.child("__manifest__.py").touch().unwrap();

    // odools.toml in ws1 references both ws1 and ws2 via template
    let toml_content = r#"
        [[config]]
        name = "default"
        addons_paths = [
            "${workspaceFolder}",
            "${workspaceFolder:ws2}"
        ]
    "#;
    let ws1_odools = ws1.child("odools.toml");
    ws1_odools.write_str(toml_content).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());
    ws_folders.insert(S!("ws2"), ws2.path().sanitize().to_string());

    let (_config_map, config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config_entry = config_file.config.iter().find(|c| c.name == "default").unwrap();

    // Both ws1 and ws2 should be present in addons_paths, each sourced from ws1_odools
    let addons_paths = config_entry.addons_paths_sourced();
    assert!(addons_paths.iter().flatten().any(|s| s.value() == &ws1.path().sanitize() && s.sources().contains(&ws1_odools.path().sanitize())));
    assert!(addons_paths.iter().flatten().any(|s| s.value() == &ws2.path().sanitize() && s.sources().contains(&ws1_odools.path().sanitize())));
}

#[test]
fn test_config_file_sources_default_values() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // No odools.toml, so all values should be defaults and sourced from "$default"
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let (_config_map, config_file) = get_configuration(&ws_folders, &None).unwrap();

    // The $default source is only visible in the HTML serialization
    let config_file_str = config_file.to_html_string();
    assert!(config_file_str.contains("$default"));
}

#[test]
fn test_config_file_sources_multiple_workspace_folders_and_shadowing() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    let ws2 = temp.child("ws2");
    ws1.create_dir_all().unwrap();
    ws2.create_dir_all().unwrap();

    // ws1 odools.toml
    let ws1_toml = r#"
        [[config]]
        name = "default"
        python_path = "python"
    "#;
    let ws1_odools = ws1.child("odools.toml");
    ws1_odools.write_str(ws1_toml).unwrap();

    // ws2 odools.toml
    let ws2_toml = r#"
        [[config]]
        name = "default"
        python_path = "python"
    "#;
    let ws2_odools = ws2.child("odools.toml");
    ws2_odools.write_str(ws2_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());
    ws_folders.insert(S!("ws2"), ws2.path().sanitize().to_string());

    let (_config_map, config_file) = get_configuration(&ws_folders, &None).unwrap();

    // There should be only one config entry for "default" (merged)
    let root_entry = config_file.config.iter().find(|c| c.name == "default").unwrap();

    // The merged python_path should be "python" (from ws1, as it is merged in order)
    assert_eq!(root_entry.python_path_sourced().as_ref().unwrap().value(), "python");

    // The sources should include both ws1 and ws2 odools.toml files
    let sources = root_entry.python_path_sourced().as_ref().unwrap().sources();
    assert!(sources.contains(&ws1_odools.path().sanitize()));
    assert!(sources.contains(&ws2_odools.path().sanitize()));
}

#[test]
fn test_config_file_sources_json_serialization() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    ws1.create_dir_all().unwrap();
    let addon1 = ws1.child("addon1");
    addon1.create_dir_all().unwrap();
    addon1.child("__manifest__.py").touch().unwrap();

    let toml_content = r#"
        [[config]]
        name = "default"
    "#;
    ws1.child("odools.toml").write_str(toml_content).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());

    let (_config_map, config_file) = get_configuration(&ws_folders, &None).unwrap();

    // Serialize to JSON and check sources for python_path and addons_paths
    let json = serde_json::to_value(&config_file).unwrap();
    let config_arr = json.get("config").unwrap().as_array().unwrap();
    let root = config_arr.iter().find(|c| c.get("name").unwrap() == "default").unwrap();

    // python_path should have $default in sources
    let python_path = root.get("python_path").unwrap();
    assert!(python_path.get("sources").unwrap().as_array().unwrap().iter().any(|v| v == "$default"));

    // addons_paths should have workspaceFolder:ws1 in sources
    let addons_paths = root.get("addons_paths").unwrap().as_array().unwrap();
    assert!(addons_paths.iter().any(|ap| {
        ap.get("sources").unwrap().as_array().unwrap().iter().any(|v| v.as_str().unwrap().contains("workspaceFolder:ws1"))
    }));
}

#[test]
fn test_conflict_two_workspace_folders_both_odoo_path() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    let ws2 = temp.child("ws2");
    ws1.create_dir_all().unwrap();
    ws2.create_dir_all().unwrap();

    // Make both ws1 and ws2 valid odoo paths
    ws1.child("odoo").create_dir_all().unwrap();
    ws1.child("odoo").child("release.py").touch().unwrap();
    ws2.child("odoo").create_dir_all().unwrap();
    ws2.child("odoo").child("release.py").touch().unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());
    ws_folders.insert(S!("ws2"), ws2.path().sanitize().to_string());

    // Should error due to ambiguous odoo_path
    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_err());
    assert!(result.err().unwrap().contains("More than one workspace folder is a valid odoo_path"));
}

#[test]
fn test_no_conflict_when_config_files_point_to_same_odoo_path() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    let ws2 = temp.child("ws2");
    ws1.create_dir_all().unwrap();
    ws2.create_dir_all().unwrap();

    // Only ws1 is a valid odoo path
    ws1.child("odoo").create_dir_all().unwrap();
    ws1.child("odoo").child("release.py").touch().unwrap();

    // Both configs explicitly set odoo_path to ws1
    let ws1_toml = format!(
        r#"
        [[config]]
        name = "default"
        odoo_path = "{}"
    "#, ws1.path().sanitize());
    ws1.child("odools.toml").write_str(&ws1_toml).unwrap();
    ws2.child("odools.toml").write_str(&ws1_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());
    ws_folders.insert(S!("ws2"), ws2.path().sanitize().to_string());

    // Should NOT error, odoo_path is unambiguous
    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_ok());
    let (config_map, _config_file) = result.unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.odoo_path.as_ref().unwrap(), &ws1.path().sanitize());
}

#[test]
fn test_merge_different_odoo_paths_and_addons_paths() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    let ws2 = temp.child("ws2");
    let shared_addons = temp.child("shared_addons");
    ws1.create_dir_all().unwrap();
    ws2.create_dir_all().unwrap();
    shared_addons.create_dir_all().unwrap();

    // shared_addons is a valid addons path (contains a module)
    let shared_mod = shared_addons.child("shared_mod");
    shared_mod.create_dir_all().unwrap();
    shared_mod.child("__manifest__.py").touch().unwrap();

    // ws1_addons is a valid addons path (contains a module)
    let ws1_addons = ws1.child("addons1");
    ws1_addons.create_dir_all().unwrap();
    ws1_addons.child("mod1").create_dir_all().unwrap();
    ws1_addons.child("mod1").child("__manifest__.py").touch().unwrap();

    // ws2_addons is a valid addons path (contains a module)
    let ws2_addons = ws2.child("addons2");
    ws2_addons.create_dir_all().unwrap();
    ws2_addons.child("mod2").create_dir_all().unwrap();
    ws2_addons.child("mod2").child("__manifest__.py").touch().unwrap();

    // Both configs use the same odoo_path (ws1), but different addons_paths
    let ws1_toml = format!(
        r#"
        [[config]]
        name = "default"
        odoo_path = "{}"
        addons_paths = [
            "{}",
            "{}"
        ]
    "#,
        ws1.path().sanitize(),
        shared_addons.path().sanitize(),
        ws1_addons.path().sanitize()
    );
    let ws2_toml = format!(
        r#"
        [[config]]
        name = "default"
        odoo_path = "{}"
        addons_paths = [
            "{}",
            "{}"
        ]
    "#,
        ws1.path().sanitize(),
        shared_addons.path().sanitize(),
        ws2_addons.path().sanitize()
    );
    ws1.child("odools.toml").write_str(&ws1_toml).unwrap();
    ws2.child("odools.toml").write_str(&ws2_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());
    ws_folders.insert(S!("ws2"), ws2.path().sanitize().to_string());

    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_ok());
    let (config_map, config_file) = result.unwrap();
    let config = config_map.get("default").unwrap();


    // addons_paths should include shared_addons, ws1_addons, ws2_addons (order not guaranteed, but all present)
    let expected = vec![
        shared_addons.path().sanitize(),
        ws1_addons.path().sanitize(),
        ws2_addons.path().sanitize(),
    ].into_iter().collect::<HashSet<_>>();
    let actual = config.addons_paths.clone();
    assert_eq!(actual, expected);

    // Also check that sources for shared_addons include both ws1 and ws2 odools.toml
    let shared_addons_sources: Vec<_> = config_file
        .config
        .iter()
        .find(|c| c.name == "default")
        .unwrap()
        .addons_paths_sourced()
        .iter()
        .flatten()
        .filter(|s| s.value() == &shared_addons.path().sanitize())
        .flat_map(|s| s.sources())
        .cloned()
        .collect();
    assert!(shared_addons_sources.iter().any(|src| src.ends_with("ws1/odools.toml")));
    assert!(shared_addons_sources.iter().any(|src| src.ends_with("ws2/odools.toml")));
}

#[test]
fn test_addons_paths_merge_method_override_vs_merge() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Parent odools.toml with two addons_paths and override
    let parent_addons1 = temp.child("parent_addons1");
    let parent_addons2 = temp.child("parent_addons2");
    parent_addons1.create_dir_all().unwrap();
    parent_addons1.child("mod1").create_dir_all().unwrap();
    parent_addons1.child("mod1").child("__manifest__.py").touch().unwrap();
    parent_addons2.create_dir_all().unwrap();
    parent_addons2.child("mod2").create_dir_all().unwrap();
    parent_addons2.child("mod2").child("__manifest__.py").touch().unwrap();

    // Workspace odools.toml with its own addons_paths
    let ws_addons = ws_folder.child("ws_addons");
    ws_addons.create_dir_all().unwrap();
    ws_addons.child("mod3").create_dir_all().unwrap();
    ws_addons.child("mod3").child("__manifest__.py").touch().unwrap();

    // First, test with override: only workspace's addons_paths should be present
    let parent_toml = format!(
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}",
            "{}"
        ]
    "#,
        parent_addons1.path().sanitize(),
        parent_addons2.path().sanitize()
    );
    temp.child("odools.toml").write_str(&parent_toml).unwrap();

    let ws_toml = format!(
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}"
        ]
        addons_merge = "override"
    "#,
        ws_addons.path().sanitize()
    );
    ws_folder.child("odools.toml").write_str(&ws_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    // With override, only workspace's addons_paths should be present
    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.addons_paths, vec![ws_addons.path().sanitize()].into_iter().collect::<HashSet<_>>());

    // Now test with merge: both parent and workspace addons_paths should be present

    let ws_toml = format!(
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}"
        ]
    "#,
        ws_addons.path().sanitize()
    );
    ws_folder.child("odools.toml").write_str(&ws_toml).unwrap();

    // Re-run config
    let (config_map2, _config_file2) = get_configuration(&ws_folders, &None).unwrap();
    let config2 = config_map2.get("default").unwrap();
    let expected = vec![
        parent_addons1.path().sanitize(),
        parent_addons2.path().sanitize(),
        ws_addons.path().sanitize(),
    ].into_iter().collect::<HashSet<_>>();
    let actual = config2.addons_paths.clone();
    assert_eq!(actual, expected);
}

#[test]
fn test_conflict_and_merge_of_boolean_fields() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    let ws2 = temp.child("ws2");
    ws1.create_dir_all().unwrap();
    ws2.create_dir_all().unwrap();

    // --- Conflict: file_cache differs between workspaces ---
    let ws1_toml = r#"
        [[config]]
        name = "default"
        file_cache = true
    "#;
    let ws2_toml = r#"
        [[config]]
        name = "default"
        file_cache = false
    "#;
    ws1.child("odools.toml").write_str(ws1_toml).unwrap();
    ws2.child("odools.toml").write_str(ws2_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());
    ws_folders.insert(S!("ws2"), ws2.path().sanitize().to_string());

    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_err());
    assert!(result.err().unwrap().contains("Conflict detected"));

    // --- Merge: file_cache set in parent, not in workspace ---
    let parent_toml = r#"
        [[config]]
        name = "default"
        file_cache = false
        ac_filter_model_names = false
    "#;
    temp.child("odools.toml").write_str(parent_toml).unwrap();

    let ws1_toml = r#"
        [[config]]
        name = "default"
        # file_cache not set, should inherit from parent
        ac_filter_model_names = true
    "#;
    ws1.child("odools.toml").write_str(ws1_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.file_cache, false); // inherited from parent
    assert_eq!(config.ac_filter_model_names, true); // overridden by workspace
}

#[test]
fn test_path_case_and_trailing_slash_normalization() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("Workspace1");
    ws_folder.create_dir_all().unwrap();

    // Addon path with trailing slash and different case
    let addon_dir = ws_folder.child("My_Module").child("");
    addon_dir.create_dir_all().unwrap();
    addon_dir.child("__manifest__.py").touch().unwrap();

    // Use different case and trailing slash in config
    let with_slash = ws_folder.child("").path().sanitize();
    let without_slash = ws_folder.path().sanitize();

    let toml_content = format!(
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}",
            "{}"
        ]
    "#,
        with_slash,
        without_slash,
    );
    ws_folder.child("odools.toml").write_str(&toml_content).unwrap();

    let mut ws_folders = HashMap::new();
    // Use different case for workspace folder
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize());

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    // Should only have one normalized path for the addon
    let mut normalized_addon = ws_folder.path().sanitize();
    // Remove trailing slash if present
    if normalized_addon.ends_with('/') {
        normalized_addon.pop();
    }
    assert!(config.addons_paths.iter().any(|p| p == &normalized_addon));

    // Now test with only the slash version
    let toml_content_slash = format!(
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}"
        ]
    "#,
        with_slash
    );
    ws_folder.child("odools.toml").write_str(&toml_content_slash).unwrap();

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert!(config.addons_paths.iter().any(|p| p == &normalized_addon));

    // Now test with only the non-slash version
    let toml_content_noslash = format!(
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}"
        ]
    "#,
        without_slash
    );
    ws_folder.child("odools.toml").write_str(&toml_content_noslash).unwrap();

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert!(config.addons_paths.iter().any(|p| p == &normalized_addon));
}

#[test]
fn test_extends_chain_multiple_profiles_and_order() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Parent odools.toml: defines base, mid, root in different orders
    let parent_toml = r#"
        [[config]]
        name = "mid"
        extends = "base"
        file_cache = false

        [[config]]
        name = "base"
        auto_refresh_delay = 1111
        ac_filter_model_names = false

        [[config]]
        name = "default"
        extends = "mid"
        diag_missing_imports = "only_odoo"
    "#;
    temp.child("odools.toml").write_str(parent_toml).unwrap();

    // Workspace odools.toml: overrides only root
    let ws_toml = r#"
        [[config]]
        name = "default"
        ac_filter_model_names = true
    "#;
    ws_folder.child("odools.toml").write_str(ws_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    // Should inherit file_cache from mid, auto_refresh_delay from base, diag_missing_imports from root, ac_filter_model_names from workspace
    assert_eq!(config.file_cache, false);
    assert_eq!(config.auto_refresh_delay, 1111);
    assert_eq!(format!("{:?}", config.diag_missing_imports).to_lowercase(), "onlyodoo");
    assert_eq!(config.ac_filter_model_names, true);

    // Now swap the order of base and mid in parent file, should not affect result
    let parent_toml_swapped = r#"
        [[config]]
        name = "base"
        auto_refresh_delay = 1111
        ac_filter_model_names = false

        [[config]]
        name = "mid"
        extends = "base"
        file_cache = false

        [[config]]
        name = "default"
        extends = "mid"
        diag_missing_imports = "only_odoo"
    "#;
    temp.child("odools.toml").write_str(parent_toml_swapped).unwrap();

    let (config_map2, _config_file2) = get_configuration(&ws_folders, &None).unwrap();
    let config2 = config_map2.get("default").unwrap();
    assert_eq!(config2.file_cache, false);
    assert_eq!(config2.auto_refresh_delay, 1111);
    assert_eq!(format!("{:?}", config2.diag_missing_imports).to_lowercase(), "onlyodoo");
    assert_eq!(config2.ac_filter_model_names, true);

    // Now move base to workspace file, mid and root in parent
    let ws_toml_base = r#"
        [[config]]
        name = "base"
        auto_refresh_delay = 1111
        ac_filter_model_names = false
    "#;
    ws_folder.child("odools.toml").write_str(ws_toml_base).unwrap();
    let parent_toml_mid_root = r#"
        [[config]]
        name = "mid"
        extends = "base"
        file_cache = false

        [[config]]
        name = "default"
        extends = "mid"
        diag_missing_imports = "only_odoo"
    "#;
    temp.child("odools.toml").write_str(parent_toml_mid_root).unwrap();

    let (config_map3, _config_file3) = get_configuration(&ws_folders, &None).unwrap();
    let config3 = config_map3.get("default").unwrap();
    // Should still resolve the chain correctly
    assert_eq!(config3.file_cache, false);
    assert_eq!(config3.auto_refresh_delay, 1111);
    assert_eq!(format!("{:?}", config3.diag_missing_imports).to_lowercase(), "onlyodoo");
    assert_eq!(config3.ac_filter_model_names, false);
}

#[test]
fn test_extends_cycle_detection() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Parent odools.toml: cycle between base and mid
    let parent_toml = r#"
        [[config]]
        name = "base"
        extends = "mid"
        auto_refresh_delay = 111

        [[config]]
        name = "mid"
        extends = "base"
        file_cache = false

        [[config]]
        name = "default"
        extends = "mid"
        diag_missing_imports = "only_odoo"
    "#;
    temp.child("odools.toml").write_str(parent_toml).unwrap();

    let ws_toml = r#"
        [[config]]
        name = "default"
        ac_filter_model_names = true
    "#;
    ws_folder.child("odools.toml").write_str(ws_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_err());
    assert!(result.err().unwrap().contains("Circular dependency detected"));
}

#[test]
fn test_extends_nonexistent_profile_error() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Parent odools.toml: root extends a non-existent profile "doesnotexist"
    let parent_toml = r#"
        [[config]]
        name = "default"
        extends = "doesnotexist"
        file_cache = false
    "#;
    temp.child("odools.toml").write_str(parent_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_err());
    assert!(result.err().unwrap().to_lowercase().contains("extends non-existing profile"));
}

#[test]
fn test_invalid_toml_config() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Invalid TOML: missing closing quote
    let invalid_toml = r#"
        [[config]]
        name = "default"
        file_cache = true
        python_path = "python
    "#;
    ws_folder.child("odools.toml").write_str(invalid_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_err());
    assert!(result.err().unwrap().to_lowercase().contains("toml"));
}

#[test]
fn test_malformed_config_missing_required_fields() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Malformed config: missing 'name' field, but should default to "default"
    let toml_missing_name = r#"
        [[config]]
        file_cache = true
    "#;
    ws_folder.child("odools.toml").write_str(toml_missing_name).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    // Should not error, should default name to "default"
    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_ok());
    let (config_map, _) = result.unwrap();
    assert!(config_map.contains_key("default"));

    // Malformed config: completely empty config
    let empty_toml = "";
    ws_folder.child("odools.toml").write_str(empty_toml).unwrap();

    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_ok());
    let (config_map, _) = result.unwrap();
    // Should still have a default "default" config entry
    assert!(config_map.contains_key("default"));
}

#[test]
fn test_template_variable_expansion_userhome_and_workspacefolder() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    ws1.create_dir_all().unwrap();

    // Create a dummy addon in user home for testing
    let user_home = dirs::home_dir().unwrap();
    let user_home_addon_path = user_home.join("my_home_addons").join("my_home_addon");
    std::fs::create_dir_all(&user_home_addon_path).unwrap();
    std::fs::File::create(user_home_addon_path.join("__manifest__.py")).unwrap();

    // Create a dummy addon in ws1 for testing
    let ws1_addon = ws1.child("my_ws1_addon");
    ws1_addon.create_dir_all().unwrap();
    ws1_addon.child("__manifest__.py").touch().unwrap();

    // Compose config using both ${userHome} and ${workspaceFolder:ws1}
    let toml_content = format!(
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "${{userHome}}/my_home_addons",
            "${{workspaceFolder:ws1}}"
        ]
    "#
    );
    ws1.child("odools.toml").write_str(&toml_content).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    // Both expanded paths should be present in addons_paths
    let expected_home_addon = user_home.join("my_home_addons").sanitize();
    let expected_ws1_addon = ws1.path().sanitize();

    assert!(config.addons_paths.contains(&expected_home_addon));
    assert!(config.addons_paths.contains(&expected_ws1_addon));
}

#[test]
fn test_config_with_relative_addons_paths() {
    let temp = TempDir::new().unwrap();
    let ws = temp.child("ws");
    ws.create_dir_all().unwrap();

    // Create addons1/mod1/__manifest__.py
    let addons1 = ws.child("addons1");
    let mod1 = addons1.child("mod1");
    mod1.create_dir_all().unwrap();
    mod1.child("__manifest__.py").touch().unwrap();

    // Create addons2/mod2/__manifest__.py
    let addons2 = ws.child("addons2");
    let mod2 = addons2.child("mod2");
    mod2.create_dir_all().unwrap();
    mod2.child("__manifest__.py").touch().unwrap();

    // Write odools.toml with relative paths
    let toml_content = r#"
        [[config]]
        name = "default"
        addons_paths = [
            "./addons1",
            "./addons2"
        ]
    "#;
    ws.child("odools.toml").write_str(toml_content).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws"), ws.path().sanitize().to_string());

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    // The expected absolute, sanitized paths
    let expected1 = ws.child("addons1").path().sanitize();
    let expected2 = ws.child("addons2").path().sanitize();

    assert!(config.addons_paths.contains(&expected1), "Expected addons_paths to contain {}", expected1);
    assert!(config.addons_paths.contains(&expected2), "Expected addons_paths to contain {}", expected2);
}

#[test]
fn test_relative_addons_paths_in_parent_config() {
    let temp = TempDir::new().unwrap();
    let ws = temp.child("ws");
    ws.create_dir_all().unwrap();

    // Create addons1/mod1/__manifest__.py
    let addons1 = temp.child("addons1");
    let mod1 = addons1.child("mod1");
    mod1.create_dir_all().unwrap();
    mod1.child("__manifest__.py").touch().unwrap();

    // Create addons2/mod2/__manifest__.py
    let addons2 = temp.child("addons2");
    let mod2 = addons2.child("mod2");
    mod2.create_dir_all().unwrap();
    mod2.child("__manifest__.py").touch().unwrap();

    // Write odools.toml in parent directory (temp), with relative paths
    let toml_content = r#"
        [[config]]
        name = "default"
        addons_paths = [
            "./addons1",
            "./addons2"
        ]
    "#;
    temp.child("odools.toml").write_str(toml_content).unwrap();

    // Workspace folder does not have its own odools.toml
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws"), ws.path().sanitize().to_string());

    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();

    // The expected absolute, sanitized paths
    let expected1 = addons1.path().sanitize();
    let expected2 = addons2.path().sanitize();

    assert!(config.addons_paths.contains(&expected1), "Expected addons_paths to contain {}", expected1);
    assert!(config.addons_paths.contains(&expected2), "Expected addons_paths to contain {}", expected2);
}

#[test]
fn test_auto_refresh_delay_boundaries() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Below minimum (should clamp to 1000)
    let toml_content_min = r#"
        [[config]]
        name = "default"
        auto_refresh_delay = 500
    "#;
    ws_folder.child("odools.toml").write_str(toml_content_min).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let (config_map, _) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.auto_refresh_delay, 1000);

    // Above maximum (should clamp to 15000)
    let toml_content_max = r#"
        [[config]]
        name = "default"
        auto_refresh_delay = 20000
    "#;
    ws_folder.child("odools.toml").write_str(toml_content_max).unwrap();

    let (config_map, _) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.auto_refresh_delay, 15000);

    // Within bounds (should keep value)
    let toml_content_ok = r#"
        [[config]]
        name = "default"
        auto_refresh_delay = 1234
    "#;
    ws_folder.child("odools.toml").write_str(toml_content_ok).unwrap();

    let (config_map, _) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.auto_refresh_delay, 1234);
}

#[test]
fn test_odoo_path_with_version_variable_and_workspace_folder() {
    let temp = TempDir::new().unwrap();

    // Create temp/18.0/odoo/release.py
    let odoo_18 = temp.child("18.0").child("odoo").child("odoo");
    odoo_18.create_dir_all().unwrap();
    odoo_18.child("release.py").touch().unwrap();

    // Create temp/17.0/odoo/release.py
    let odoo_17 = temp.child("17.0").child("odoo").child("odoo");
    odoo_17.create_dir_all().unwrap();
    odoo_17.child("release.py").touch().unwrap();

    // Write odools.toml in temp with odoo_path using $version
    let toml_content = r#"
        [[config]]
        name = "default"
        "$version" = "18.0"
        odoo_path = "./${version}/odoo"
    "#;
    temp.child("odools.toml").write_str(toml_content).unwrap();

    // Workspace: temp (simulate as workspace root)
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws"), temp.path().sanitize().to_string());

    let (config_map, _) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    let expected_odoo_path = temp.child("18.0").child("odoo").path().sanitize();
    assert_eq!(
        config.odoo_path.as_ref().unwrap(),
        &expected_odoo_path,
        "odoo_path should resolve to 18.0/odoo"
    );

    // Now test with workspace at temp/18.0/addons/
    let ws_18_addons = temp.child("18.0").child("addons");
    let addon1 = ws_18_addons.child("addon1");
    addon1.create_dir_all().unwrap();
    addon1.child("__manifest__.py").touch().unwrap();

    // Write odools.toml in temp/18.0/addons/ with $version = "${workspaceFolder}/.."
    let ws18_toml = r#"
        [[config]]
        "$version" = "${workspaceFolder}/.."
    "#;
    ws_18_addons.child("odools.toml").write_str(ws18_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws18"), ws_18_addons.path().sanitize().to_string());

    let (config_map, _) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    let expected_odoo_path = temp.child("18.0").child("odoo").path().sanitize();
    assert_eq!(
        config.odoo_path.as_ref().unwrap(),
        &expected_odoo_path,
        "odoo_path should resolve to 18.0/odoo when workspace is 18.0/addons"
    );

    // Now test with workspace at temp/17.0/addons/
    let ws_17_addons = temp.child("17.0").child("addons");
    let addon2 = ws_17_addons.child("addon2");
    addon2.create_dir_all().unwrap();
    addon2.child("__manifest__.py").touch().unwrap();

    // Write odools.toml in temp/17.0/addons/ with $version = "${workspaceFolder}/.."
    let ws17_toml = r#"
        [[config]]
        "$version" = "${workspaceFolder}/.."
    "#;
    ws_17_addons.child("odools.toml").write_str(ws17_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws17"), ws_17_addons.path().sanitize().to_string());

    let (config_map, _) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    let expected_odoo_path = temp.child("17.0").child("odoo").path().sanitize();
    assert_eq!(
        config.odoo_path.as_ref().unwrap(),
        &expected_odoo_path,
        "odoo_path should resolve to 17.0/odoo when workspace is 17.0/addons"
    );
}

#[test]
fn test_odoo_path_with_version_from_manifest_file() {
    let temp = TempDir::new().unwrap();

    // Write odools.toml in temp with odoo_path using $version
    let toml_content = r#"
        [[config]]
        name = "default"
        odoo_path = "./${version}/odoo"
    "#;
    temp.child("odools.toml").write_str(toml_content).unwrap();

    // Create temp/18.0/odoo/release.py
    let odoo_18 = temp.child("18.0").child("odoo").child("odoo");
    odoo_18.create_dir_all().unwrap();
    odoo_18.child("release.py").touch().unwrap();

    // Create temp/17.0/odoo/release.py
    let odoo_17 = temp.child("17.0").child("odoo").child("odoo");
    odoo_17.create_dir_all().unwrap();
    odoo_17.child("release.py").touch().unwrap();

    // --- 18.0 workspace ---
    let ws_18_addons = temp.child("18.0").child("addons");
    let addon1_18 = ws_18_addons.child("addon1");
    addon1_18.create_dir_all().unwrap();
    // Write __manifest__.py with version = "18.0.1.0.0"
    addon1_18.child("__manifest__.py").write_str("{'version': '18.0.1.0.0'}").unwrap();

    // Write odools.toml in temp/18.0/addons/ with $version = "${workspaceFolder}/addon1/__manifest__.py"
    let ws18_toml = r#"
        [[config]]
        "$version" = "${workspaceFolder}/addon1/__manifest__.py"
    "#;
    ws_18_addons.child("odools.toml").write_str(ws18_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws18"), ws_18_addons.path().sanitize().to_string());

    let (config_map, _) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    let expected_odoo_path = temp.child("18.0").child("odoo").path().sanitize();
    assert_eq!(
        config.odoo_path.as_ref().unwrap(),
        &expected_odoo_path,
        "odoo_path should resolve to 18.0/odoo when manifest version is 18.0.1.0.0"
    );

    // --- 17.0 workspace ---
    let ws_17_addons = temp.child("17.0").child("addons");
    let addon1_17 = ws_17_addons.child("addon1");
    addon1_17.create_dir_all().unwrap();
    // Write __manifest__.py with version = "17.0.1.0.0"
    addon1_17.child("__manifest__.py").write_str("{'version': '17.0.1.0.0'}").unwrap();

    // Write odools.toml in temp/17.0/addons/ with $version = "${workspaceFolder}/addon1/__manifest__.py"
    let ws17_toml = r#"
        [[config]]
        "$version" = "${workspaceFolder}/addon1/__manifest__.py"
    "#;
    ws_17_addons.child("odools.toml").write_str(ws17_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws17"), ws_17_addons.path().sanitize().to_string());

    let (config_map, _) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    let expected_odoo_path = temp.child("17.0").child("odoo").path().sanitize();
    assert_eq!(
        config.odoo_path.as_ref().unwrap(),
        &expected_odoo_path,
        "odoo_path should resolve to 17.0/odoo when manifest version is 17.0.1.0.0"
    );
}

#[test]
fn test_addons_paths_unset_vs_empty_behavior() {
    let temp = TempDir::new().unwrap();
    let ws = temp.child("ws");
    ws.create_dir_all().unwrap();

    // Make ws a valid addons path (contains a module)
    let addon = ws.child("mod1");
    addon.create_dir_all().unwrap();
    addon.child("__manifest__.py").touch().unwrap();

    // Case 1: addons_paths is unset (should add workspace if valid)
    let toml_unset = r#"
        [[config]]
        name = "default"
    "#;
    ws.child("odools.toml").write_str(toml_unset).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws"), ws.path().sanitize().to_string());

    let (config_map, _) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert!(config.addons_paths.contains(&ws.path().sanitize()), "Workspace should be added to addons_paths when addons_paths is not set");

    // Case 2: addons_paths is set to empty list (should NOT add workspace)
    let toml_empty = r#"
        [[config]]
        name = "default"
        addons_paths = []
    "#;
    ws.child("odools.toml").write_str(toml_empty).unwrap();

    let (config_map2, _) = get_configuration(&ws_folders, &None).unwrap();
    let config2 = config_map2.get("default").unwrap();
    assert!(!config2.addons_paths.contains(&ws.path().sanitize()), "Workspace should NOT be added to addons_paths when addons_paths is set to []");
}


#[test]
fn test_addons_merge_override_cases() {
    let temp = TempDir::new().unwrap();
    let ws = temp.child("ws");
    ws.create_dir_all().unwrap();

    // Parent: parent_addons/mod1/__manifest__.py
    let parent_addons = temp.child("parent_addons");
    let parent_mod1 = parent_addons.child("mod1");
    parent_mod1.create_dir_all().unwrap();
    parent_mod1.child("__manifest__.py").touch().unwrap();

    // Child: ws_addons/mod2/__manifest__.py
    let ws_addons = ws.child("ws_addons");
    let ws_mod2 = ws_addons.child("mod2");
    ws_mod2.create_dir_all().unwrap();
    ws_mod2.child("__manifest__.py").touch().unwrap();

    // --- Case 1: Parent gives one valid addons path, child gives another valid addons with addons_merge = override ---
    let parent_toml = format!(
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}"
        ]
    "#,
        parent_addons.path().sanitize()
    );
    temp.child("odools.toml").write_str(&parent_toml).unwrap();

    let child_toml = format!(
        r#"
        [[config]]
        name = "default"
        addons_paths = [
            "{}"
        ]
        addons_merge = "override"
    "#,
        ws_addons.path().sanitize()
    );
    ws.child("odools.toml").write_str(&child_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws"), ws.path().sanitize().to_string());

    let (config_map, _) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(
        config.addons_paths,
        vec![ws_addons.path().sanitize()].into_iter().collect(),
        "With addons_merge=override, only child addons_paths should be present"
    );

    // --- Case 2: Child sets addons_merge = override, but does not set addons_paths, should add ws as addons_path if valid ---
    let child_toml2 = r#"
        [[config]]
        name = "default"
        addons_merge = "override"
    "#;
    ws.child("odools.toml").write_str(child_toml2).unwrap();

    // Make ws itself a valid addons path
    let ws_mod3 = ws.child("mod3");
    ws_mod3.create_dir_all().unwrap();
    ws_mod3.child("__manifest__.py").touch().unwrap();

    let (config_map2, _) = get_configuration(&ws_folders, &None).unwrap();
    let config2 = config_map2.get("default").unwrap();
    assert!(
        config2.addons_paths.contains(&ws.path().sanitize()),
        "With addons_merge=override and no addons_paths, workspace should be added if valid"
    );

    // --- Case 3: Child sets addons_merge = override and sets addons_paths as [] ---
    let child_toml3 = r#"
        [[config]]
        name = "default"
        addons_paths = []
        addons_merge = "override"
    "#;
    ws.child("odools.toml").write_str(child_toml3).unwrap();

    let (config_map3, _) = get_configuration(&ws_folders, &None).unwrap();
    let config3 = config_map3.get("default").unwrap();
    assert!(
        config3.addons_paths.is_empty(),
        "With addons_merge=override and addons_paths=[], no addons paths should be present"
    );
}


#[test]
fn test_detect_version_variable_creates_profiles_for_each_version() {
    let temp = TempDir::new().unwrap();
    let ws1 = temp.child("ws1");
    ws1.create_dir_all().unwrap();

    // Create ws1/17.0/addon17/__manifest__.py
    let v17 = ws1.child("17.0");
    let addon17 = v17.child("addon17");
    addon17.create_dir_all().unwrap();
    addon17.child("__manifest__.py").touch().unwrap();

    // Create ws1/18.0/addon18/__manifest__.py
    let v18 = ws1.child("18.0");
    let addon18 = v18.child("addon18");
    addon18.create_dir_all().unwrap();
    addon18.child("__manifest__.py").touch().unwrap();

    // Write odools.toml in ws1 with $version = "${workspaceFolder}${splitVersion}"
    let toml_content = r#"
        [[config]]
        name = "root"
        "$version" = "${workspaceFolder}/${splitVersion}"
        addons_paths = [
            "./${version}"
        ]
    "#;
    ws1.child("odools.toml").write_str(toml_content).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws1.path().sanitize().to_string());

    let (config_map, config_file) = get_configuration(&ws_folders, &None).unwrap();

    // There should be three profiles: "root" (abstract), "root-17.0", "root-18.0"
    assert!(config_map.contains_key("root"), "Should contain abstract root profile");
    assert!(config_map.contains_key("root-17.0"), "Should contain root-17.0 profile");
    assert!(config_map.contains_key("root-18.0"), "Should contain root-18.0 profile");

    // The abstract profile should be marked as abstract
    let abstract_entry = config_file.config.iter().find(|c| c.name == "root").unwrap();
    assert!(abstract_entry.is_abstract(), "Abstract profile should be marked as abstract");

    // The versioned profiles should not be abstract and should have correct version
    let v17_entry = config_file.config.iter().find(|c| c.name == "root-17.0").unwrap();
    let v18_entry = config_file.config.iter().find(|c| c.name == "root-18.0").unwrap();
    assert!(!v17_entry.is_abstract(), "root-17.0 should not be abstract");
    assert!(!v18_entry.is_abstract(), "root-18.0 should not be abstract");

    // The addons_paths for each versioned profile should point to the correct version folder
    let v17_path = v17.path().sanitize();
    let v18_path = v18.path().sanitize();
    assert!(v17_entry.addons_paths_sourced().as_ref().unwrap().iter().any(|s| s.value() == &v17_path));
    assert!(v18_entry.addons_paths_sourced().as_ref().unwrap().iter().any(|s| s.value() == &v18_path));
}

#[test]
fn test_config_file_path_priority() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    // Create two addon dirs for testing merge
    let ws_addon = ws_folder.child("ws_addons");
    ws_addon.create_dir_all().unwrap();
    ws_addon.child("mod").child("__manifest__.py").touch().unwrap();

    let ext_addon = temp.child("ext_addons");
    ext_addon.create_dir_all().unwrap();
    ext_addon.child("mod").child("__manifest__.py").touch().unwrap();

    // Workspace config with one addons_path
    let ws_toml = format!(r#"
        [[config]]
        name = "default"
        python_path = "python"
        addons_paths = ["{}"]
    "#, ws_addon.path().sanitize());
    ws_folder.child("odools.toml").write_str(&ws_toml).unwrap();

    // External config with a different addons_path
    let ext_toml = format!(r#"
        [[config]]
        name = "default"
        file_cache = true
        auto_refresh_delay = 4321
        addons_paths = ["{}"]
    "#, ext_addon.path().sanitize());
    let ext_config = temp.child("external_config.toml");
    ext_config.write_str(&ext_toml).unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    let config_path = ext_config.path().sanitize();
    let (config_map, _config_file) = get_configuration(&ws_folders, &Some(config_path)).unwrap();
    let config = config_map.get("default").unwrap();

    // Should use values from external config
    assert_eq!(config.file_cache, true);
    assert_eq!(config.auto_refresh_delay, 4321);

    // Should merge addons_paths from both configs
    let ws_addon_path = ws_addon.path().sanitize();
    let ext_addon_path = ext_addon.path().sanitize();
    assert!(config.addons_paths.iter().any(|p| p == &ws_addon_path), "Should contain ws_addon path");
    assert!(config.addons_paths.iter().any(|p| p == &ext_addon_path), "Should contain ext_addon path");
}

#[test]
fn test_config_file_path_nonexistent_errors() {
    let temp = TempDir::new().unwrap();
    let ws_folder = temp.child("workspace1");
    ws_folder.create_dir_all().unwrap();

    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), ws_folder.path().sanitize().to_string());

    // Provide a non-existent config file path
    let non_existent = temp.child("does_not_exist.toml");
    let config_path = non_existent.path().sanitize();
    let result = get_configuration(&ws_folders, &Some(config_path));
    assert!(result.is_err(), "Expected error when config file path does not exist");
}

#[test]
fn test_base_and_version_resolve_for_workspace_subpaths() {
    let temp = TempDir::new().unwrap();
    let vdir = temp.child("17.0");
    vdir.create_dir_all().unwrap();
    let odoo_dir = vdir.child("odoo");
    odoo_dir.create_dir_all().unwrap();
    odoo_dir.child("odoo").child("release.py").touch().unwrap();
    let addon_dir = vdir.child("addon-path");
    addon_dir.create_dir_all().unwrap();
    addon_dir.child("mod1").create_dir_all().unwrap();
    addon_dir.child("mod1").child("__manifest__.py").touch().unwrap();

    // Write odools.toml in /temp with $base and $version logic
    let toml_content = format!(r#"
        [[config]]
        name = "default"
        "$base" = "{}/${{detectVersion}}"
        odoo_path = "${{base}}/odoo"
        addons_paths = [ "${{base}}/addon-path" ]
    "#, temp.path().sanitize());
    temp.child("odools.toml").write_str(&toml_content).unwrap();

    // Test with workspace at /temp/17.0
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), vdir.path().sanitize().to_string());
    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.odoo_path.as_ref().unwrap(), &odoo_dir.path().sanitize());
    assert!(config.addons_paths.contains(&addon_dir.path().sanitize()));

    // Test with workspace at /temp/17.0/odoo
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws2"), odoo_dir.path().sanitize().to_string());
    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.odoo_path.as_ref().unwrap(), &odoo_dir.path().sanitize());
    assert!(config.addons_paths.contains(&addon_dir.path().sanitize()));

    // Test with workspace at /temp/17.0/addon-path
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws3"), addon_dir.path().sanitize().to_string());
    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.odoo_path.as_ref().unwrap(), &odoo_dir.path().sanitize());
    assert!(config.addons_paths.contains(&addon_dir.path().sanitize()));

    // --- Scenario: use /temp/${version}/odoo and /temp/${version}/addon-path instead of ${base} ---
    let toml_content_version = format!(r#"
        [[config]]
        name = "default"
        "$base" = "{0}/${{detectVersion}}"
        odoo_path = "{0}/${{version}}/odoo"
        addons_paths = [ "{0}/${{version}}/addon-path" ]
    "#, temp.path().sanitize());
    temp.child("odools.toml").write_str(&toml_content_version).unwrap();

    // Test with workspace at /temp/17.0
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws1"), vdir.path().sanitize().to_string());
    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.odoo_path.as_ref().unwrap(), &odoo_dir.path().sanitize());
    assert!(config.addons_paths.contains(&addon_dir.path().sanitize()));

    // Test with workspace at /temp/17.0/odoo
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws2"), odoo_dir.path().sanitize().to_string());
    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.odoo_path.as_ref().unwrap(), &odoo_dir.path().sanitize());
    assert!(config.addons_paths.contains(&addon_dir.path().sanitize()));

    // Test with workspace at /temp/17.0/addon-path
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws3"), addon_dir.path().sanitize().to_string());
    let (config_map, _config_file) = get_configuration(&ws_folders, &None).unwrap();
    let config = config_map.get("default").unwrap();
    assert_eq!(config.odoo_path.as_ref().unwrap(), &odoo_dir.path().sanitize());
    assert!(config.addons_paths.contains(&addon_dir.path().sanitize()));
    // --- Crash scenario: $base is an absolute path (should error) ---
    let toml_content_abs = format!(r#"
        [[config]]
        name = "default"
        "$base" = "/not/a/real/path/${{detectVersion}}"
        odoo_path = "${{base}}/odoo"
        addons_paths = [ "${{base}}/addon-path" ]
    "#);
    temp.child("odools.toml").write_str(&toml_content_abs).unwrap();
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws_abs"), vdir.path().sanitize().to_string());
    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_err(), "Expected error when $base is an absolute path");

    // --- Crash scenario: $base is not a valid path (should error) ---
    let toml_content_invalid = r#"
        [[config]]
        name = "default"
        "$base" = "invalid_path/${detectVersion}"
        odoo_path = "${base}/odoo"
        addons_paths = [ "${base}/addon-path" ]
    "#;
    temp.child("odools.toml").write_str(toml_content_invalid).unwrap();
    let mut ws_folders = HashMap::new();
    ws_folders.insert(S!("ws_invalid"), vdir.path().sanitize().to_string());
    let result = get_configuration(&ws_folders, &None);
    assert!(result.is_err(), "Expected error when $base is not a valid path");
}