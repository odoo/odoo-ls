use odoo_ls_server::core::build_scheduler::BuildScheduler;
use std::{env, fs};
use std::path::Path;
use lsp_types::TextDocumentContentChangeEvent;
use odoo_ls_server::S;
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::utils::get_python_command;
use odoo_ls_server::{core::{config::{ConfigKey, ConfigEntry}, entry_point::EntryPointMgr, odoo::SyncOdoo}, threads::{SessionInfo, ThreadMessage}, utils::PathSanitizer as _};

use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::FmtSubscriber;

pub fn setup_server(with_odoo: bool) -> (SyncOdoo, ConfigEntry) {

    let subscriber = FmtSubscriber::builder()
        .with_max_level(LevelFilter::INFO)
        .with_ansi(false)
        .with_test_writer()
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let community_path = if with_odoo {
        Some(env::var("COMMUNITY_PATH").expect("Please provide COMMUNITY_PATH environment variable with a valid path to your Odoo Community folder"))
    } else {
        None
    };
    let mut server = SyncOdoo::new();
    server.load_odoo_addons = false;

    let test_addons_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("addons");
    info!("Test addons path: {:?}", test_addons_path);

    let mut config = ConfigEntry::new();
    config.set_string_list(ConfigKey::AddonsPaths, [test_addons_path.sanitize()]);
    server.file_mgr.add_workspace_folder(S!("test_addons_path"), FileMgr::pathname2uri(&test_addons_path.sanitize()));
    if let Some(odoo_path) = community_path.map(|x| Path::new(&x).sanitize()) {
        config.set_str(ConfigKey::OdooPath, odoo_path);
    }
    let Some(python_cmd) = get_python_command() else {
        panic!("Python not found")
    };
    config.set_str(ConfigKey::PythonPath, python_cmd);
    config.set_str(ConfigKey::DiagMissingImports, S!("all"));
    (server, config)
}

pub fn create_init_session<'a>(odoo: &'a mut SyncOdoo, config: ConfigEntry) -> SessionInfo<'a> {
    let (s, r) = crossbeam_channel::unbounded();
    let mut session = SessionInfo::new_from_custom_channel(s.clone(), r.clone(), None, odoo);
    session.sync_odoo.test_mode = true;
    SyncOdoo::init(&mut session, config);
    session
}

/// Same as [`create_init_session`], but with the channel to the main thread the tsserver bridge
/// needs: `SyncOdoo::init` only starts tsserver when the session can send to it. The returned
/// receiver carries the diagnostics tsserver pushes; keep it alive for as long as the session,
/// since dropping it makes the bridge's reader thread fail on every send.
pub fn create_init_session_with_tsserver<'a>(odoo: &'a mut SyncOdoo, config: ConfigEntry) -> (SessionInfo<'a>, crossbeam_channel::Receiver<ThreadMessage>) {
    let (s, r) = crossbeam_channel::unbounded();
    let (sender_to_main, receiver_from_threads) = crossbeam_channel::unbounded();
    let mut session = SessionInfo::new_from_custom_channel(s.clone(), r.clone(), Some(sender_to_main), odoo);
    session.sync_odoo.test_mode = true;
    SyncOdoo::init(&mut session, config);
    (session, receiver_from_threads)
}

pub fn prepare_custom_entry_point(session: &mut SessionInfo, path: &str){
    let ep_path = Path::new(path).sanitize_cow();
    let text = fs::read_to_string(path).expect("unable to read provided path");
    let event = [TextDocumentContentChangeEvent{
        range: None,
        range_length: None,
        text}];
    let content = Some(event.as_slice());
    EntryPointMgr::create_new_custom_entry_for_path(session, &ep_path, &ep_path);
    let (_file_updated, _file_info) = FileMgr::update_file_info(session, path, content, Some(1), false);
    BuildScheduler::process_rebuilds(session, false);
}
