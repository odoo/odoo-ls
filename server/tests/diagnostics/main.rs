#[path = "../common/mod.rs"]
mod common;

pub mod ols01000;
pub mod ols01001;
pub mod ols01002;
pub mod ols01003;
pub mod ols01004;
pub mod ols01005;
pub mod ols01006;
pub mod ols01007;
pub mod ols01008;
pub mod ols01009;
pub mod ols01010;
pub mod ols01011;
pub mod ols02001;
pub mod ols02002;
pub mod ols03000_1_to_23;
pub mod ols03009_operators;
pub mod ols03018_compute_sql;
pub mod ols03025_to_27;
pub mod ols03028;
pub mod ols03301;
pub mod ols03302;
pub mod ols04000_1_to_20;
pub mod ols05000s;
pub mod ols05073_74;
pub mod implicit_class_method_no_ols_01007;
pub mod magic_fields_comodel;
pub mod panic_super_arg;
pub mod test_csv_diagnostics;
pub mod test_duplicate_template;

use crate::{
    implicit_class_method_no_ols_01007::implicit_class_method_no_ols_01007,
    magic_fields_comodel::test_magic_fields_comodel,
    ols01000::test_ols01000,
    ols01001::test_ols01001,
    ols01002::test_ols01002,
    ols01003::test_ols01003,
    ols01004::test_ols01004,
    ols01005::test_ols01005,
    ols01006::test_ols01006,
    ols01007::test_ols01007,
    ols01008::test_ols01008,
    ols01009::test_ols01009,
    ols01010::test_ols01010,
    ols01011::test_ols01011,
    ols02001::test_ols02001,
    ols02002::test_ols02002,
    ols03000_1_to_23::test_ols03001_23,
    ols03009_operators::test_ols03009_version_operators,
    ols03018_compute_sql::test_ols03018_compute_sql_method,
    ols03025_to_27::test_ols03025_to_27_access_operator,
    ols03028::test_ols03028_indirect_method_inheritance,
    ols03301::test_ols03301,
    ols03302::test_ols03302,
    ols04000_1_to_20::test_ols04001_to_20,
    ols05000s::{test_ols05000_2_3_py_file, test_ols05000s_manifest, test_ols05000s_xml_file},
    ols05073_74::{test_ols05073_05074_backend_template, test_ols05074_frontend_template},
    panic_super_arg::{
        test_super_first_arg_any_does_not_panic, test_super_first_arg_none_does_not_panic,
        test_super_first_arg_unbound_does_not_panic,
    },
    test_csv_diagnostics::test_csv_diagnostics,
    common::{build_once_run_isolated, setup_server},
};

#[test]
fn test_diagnostics_with_odoo() {
    let (odoo, config) = setup_server(true);
    build_once_run_isolated(odoo, config, &[
        &test_ols03001_23,
        &test_ols03009_version_operators,
        &test_ols03018_compute_sql_method,
        &test_ols03025_to_27_access_operator,
        &test_ols03028_indirect_method_inheritance,
        &test_ols03301,
        &test_ols03302,
        &test_ols04001_to_20,
        &test_ols05000_2_3_py_file,
        &test_ols05000s_xml_file,
        &test_ols05000s_manifest,
        &test_ols05073_05074_backend_template,
        &test_ols05074_frontend_template,
        &test_magic_fields_comodel,
        &test_csv_diagnostics,
    ]);
}

/// Same as `test_diagnostics_isolated`, on a server built without Odoo.
#[test]
fn test_diagnostics_no_odoo() {
    let (odoo, config) = setup_server(false);
    build_once_run_isolated(odoo, config, &[
        &test_ols01000,
        &test_ols01001,
        &test_ols01002,
        &test_ols01003,
        &test_ols01004,
        &test_ols01005,
        &test_ols01006,
        &test_ols01007,
        &test_ols01008,
        &test_ols01009,
        &test_ols01010,
        &test_ols01011,
        &test_ols02001,
        &test_ols02002,
        &implicit_class_method_no_ols_01007,
        &test_super_first_arg_any_does_not_panic,
        &test_super_first_arg_none_does_not_panic,
        &test_super_first_arg_unbound_does_not_panic,
    ]);
}
