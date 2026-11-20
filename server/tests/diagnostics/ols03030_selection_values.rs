use super::case_failures;

/// Test OLS03030 on the values of Selection fields given to `create`, `write` and `update`.
/// Each case is a call of `selection_values.py`, found by its line.
#[test]
fn test_ols03030_selection_values() {
    // (the call as written, then the expected code and the text it points at, if invalid)
    let cases: &[(&str, Option<(&str, &str)>)] = &[
        ("self.write({'state': 'draft'})", None),
        ("self.write({'state': 'unknown_state'})", Some(("OLS03030", "'unknown_state'"))),
        ("self.create({'constant_state': 'unknown_constant'})", Some(("OLS03030", "'unknown_constant'"))),
        ("self.create([{'state': 'done'}, {'state': 'unknown_list'}])", Some(("OLS03030", "'unknown_list'"))),
        // Odoo builds the values of a method at runtime
        ("self.write({'method_state': 'unknown_method'})", None),
        // A related field has the values of its target
        ("self.write({'related_state': 'done'})", None),
        ("self.write({'related_state': 'unknown_related'})", Some(("OLS03030", "'unknown_related'"))),
        // An extension adds values
        ("self.write({'added_state': 'extra'})", None),
        ("self.write({'added_state': 'unknown_added'})", Some(("OLS03030", "'unknown_added'"))),
        // Only string values are checked
        ("self.write({'state': False, 'added_state': value})", None),
        // Not a Selection field
        ("self.write({'name': 'unknown_name'})", None),
    ];
    let failures = case_failures("selection_values.py", cases);
    assert!(
        failures.is_empty(),
        "selection values diagnostics:\n{}",
        failures.join("\n")
    );
}
