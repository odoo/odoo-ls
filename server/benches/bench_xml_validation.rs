use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use odoo_ls_server::core::entry_point::EntryPoint;
use odoo_ls_server::core::odoo::SyncOdoo;
use odoo_ls_server::core::symbols::symbol::Symbol;
use odoo_ls_server::core::xml_validation::XmlValidator;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer as _;

use iai_callgrind::{library_benchmark, library_benchmark_group, main, LibraryBenchmarkConfig};
use std::hint::black_box;

// Reuse the test setup utilities from the tests/ directory.
#[path = "../tests/setup/mod.rs"]
mod setup;

/*
To run this benchmark:
  1. Install valgrind
  2. cargo install --version 0.14.2 iai-callgrind-runner
  3. COMMUNITY_PATH=~/path/to/odoo cargo bench --bench bench_xml_validation
*/

/// Initialise the server, build symbols, and locate the bench_records.xml symbol + its entry point.
fn setup_xml_validation() -> (SyncOdoo, Rc<RefCell<Symbol>>, Rc<RefCell<EntryPoint>>) {
    let (mut server, config) = setup::setup::setup_server(true);
    let session = setup::setup::create_init_session(&mut server, config);

    // -- find the bench_records.xml symbol via entry-point data_symbols ------------------
    let mut test_addons_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_addons_path = test_addons_path.join("tests").join("data").join("addons");

    let bench_xml_path = test_addons_path
        .join("module_for_diagnostics")
        .join("data")
        .join("bench_records.xml")
        .sanitize();

    let ep_mgr = session.sync_odoo.entry_point_mgr.clone();
    for ep in ep_mgr.borrow().iter_all() {
        if let Some(sym) = ep.borrow().data_symbols.get(&bench_xml_path).and_then(|w| w.upgrade()) {
            return (server, sym, ep.clone());
        }
    }

    panic!(
        "Could not find bench_records.xml symbol in any entry point (looked for path: {})",
        bench_xml_path
    );
}

#[library_benchmark]
#[bench::validate_xml(setup = setup_xml_validation)]
fn bench_xml_validate(
    (mut server, symbol, entry): (SyncOdoo, Rc<RefCell<Symbol>>, Rc<RefCell<EntryPoint>>),
) {
    let (s, r) = crossbeam_channel::unbounded();
    let mut session = SessionInfo::new_from_custom_channel(s, r, &mut server);
    session.sync_odoo.test_mode = true;
    let mut validator = XmlValidator::new(&entry, symbol);
    black_box(validator.validate(&mut session));
}

library_benchmark_group!(name = xml_validation_group; benchmarks = bench_xml_validate);

main!(
    config = LibraryBenchmarkConfig::default().env_clear(false);
    library_benchmark_groups = xml_validation_group
);
