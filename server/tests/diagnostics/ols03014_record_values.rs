use super::case_failures;

/// Test OLS03014 on the keys of the values given to `create`, `write` and `update`, on the model the
/// method is called on. `create` takes a dict, a list of dicts or a list comprehension of one.
/// Each case is a call of `record_values.py`, found by its line.
#[test]
fn test_ols03014_record_values() {
    // (the call as written, then the expected code and the text it points at, if invalid)
    let cases: &[(&str, Option<(&str, &str)>)] = &[
        ("self.create({'name': 'x', 'parent_id': False})", None),
        ("self.create({'unknown_create_field': 'x'})", Some(("OLS03014", "'unknown_create_field'"))),
        ("self.create([{'name': 'x'}, {'unknown_create_list_field': 'x'}])", Some(("OLS03014", "'unknown_create_list_field'"))),
        ("self.create([{'unknown_create_comp_field': name} for name in names])", Some(("OLS03014", "'unknown_create_comp_field'"))),
        ("self.write({'unknown_write_field': 'x'})", Some(("OLS03014", "'unknown_write_field'"))),
        ("self.update({'unknown_update_field': 'x'})", Some(("OLS03014", "'unknown_update_field'"))),
        // The magic fields are fields of every model
        ("self.write({'write_date': False, 'create_uid': False})", None),
        // Only the string keys are checked: not the values, a spread or a computed key
        ("self.write({'name': 'unknown_value_field', **other_values, names[0]: 'x'})", None),
        // The fields of the model the method is called on, not of the class the call is in
        ("self.env['record_values.other'].create({'parent_id': False})", Some(("OLS03014", "'parent_id'"))),
        // The values of a x2many command are not checked against the model the method is called on
        ("self.create({'other_ids': [(0, 0, {'other_name': 'x'})]})", None),
        // The values given by keyword
        ("self.create(vals_list=[{'name': 'x'}, {'unknown_create_keyword_field': 'x'}])", Some(("OLS03014", "'unknown_create_keyword_field'"))),
        ("self.write(vals={'unknown_write_keyword_field': 'x'})", Some(("OLS03014", "'unknown_write_keyword_field'"))),
        ("self.update(values={'unknown_update_keyword_field': 'x'})", Some(("OLS03014", "'unknown_update_keyword_field'"))),
        // The values given to `super()` are on the model of the class
        ("return super().write({**vals, 'other_name': 'x', 'unknown_super_field': 'x'})", Some(("OLS03014", "'unknown_super_field'"))),
        // `write` takes no list of values
        ("self.write([{'unknown_write_list_field': 'x'}])", None),
        // Not a model's update
        ("return {}.update({'unknown_dict_field': 'x'})", None),
    ];
    let failures = case_failures("record_values.py", cases);
    assert!(
        failures.is_empty(),
        "record values diagnostics:\n{}",
        failures.join("\n")
    );
}
