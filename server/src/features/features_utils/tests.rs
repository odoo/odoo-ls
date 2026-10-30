
use super::field_params::FieldParam;
use crate::odoo_version::OdooVersion;

#[test]
fn test_field_param_of_kwarg_name() {
    let v19_0 = OdooVersion::new(19, 0, 0);
    assert_eq!(FieldParam::of_kwarg_name(v19_0, "related"), Some(FieldParam::Related));
    assert_eq!(FieldParam::of_kwarg_name(v19_0, "depends"), Some(FieldParam::Depends));
    assert_eq!(FieldParam::of_kwarg_name(v19_0, "group_expand"), Some(FieldParam::ModelMethod));
    // Gated by the version
    assert_eq!(FieldParam::of_kwarg_name(v19_0, "compute_sql"), None);
    assert_eq!(FieldParam::of_kwarg_name(OdooVersion::new(19, 1, 0), "compute_sql"), Some(FieldParam::ModelMethod));
    assert_eq!(FieldParam::of_kwarg_name(OdooVersion::new(19, 1, 0), "init_storage"), None);
    assert_eq!(FieldParam::of_kwarg_name(OdooVersion::new(20, 0, 0), "init_storage"), Some(FieldParam::ModelMethod));
    // Declared by the field classes: marked, not known by their name
    assert_eq!(FieldParam::of_kwarg_name(v19_0, "comodel_name"), None);
    assert_eq!(FieldParam::of_kwarg_name(v19_0, "selection"), None);
}
