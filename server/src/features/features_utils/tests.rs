use super::field_params::FieldParam;
use super::special_args::{SpecialArg, special_params_named};
use crate::odoo_version::OdooVersion;

/// Returns the kinds of the special parameters named `name` in `version`, whatever their owner.
fn kinds(version: OdooVersion, name: &str) -> Vec<SpecialArg> {
    special_params_named(version, name)
        .map(|special_param| special_param.kind)
        .collect()
}

#[test]
fn test_special_params_named() {
    let v19_0 = OdooVersion::new(19, 0, 0);
    let v19_1 = OdooVersion::new(19, 1, 0);
    let v20_0 = OdooVersion::new(20, 0, 0);
    let model_method = SpecialArg::Field(FieldParam::ModelMethod);
    assert_eq!(kinds(v19_0, "domain"), [SpecialArg::Domain]);
    assert_eq!(
        kinds(v19_0, "comodel_name"),
        [SpecialArg::Field(FieldParam::Comodel)]
    );
    assert_eq!(
        kinds(v19_0, "related"),
        [SpecialArg::Field(FieldParam::Related)]
    );
    assert_eq!(kinds(v19_0, "group_expand"), [model_method]);
    // Gated by the version
    assert!(kinds(v19_0, "compute_sql").is_empty());
    assert_eq!(kinds(v19_1, "compute_sql"), [model_method]);
    assert!(kinds(v19_1, "init_storage").is_empty());
    assert_eq!(kinds(v20_0, "init_storage"), [model_method]);
    assert!(kinds(v19_0, "string").is_empty());
}
