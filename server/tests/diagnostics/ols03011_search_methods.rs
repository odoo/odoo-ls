use super::case_failures;

/// Test OLS03011 on the `domain` of the search methods other than `search`, and of the other methods taking one.
/// The domain is given by position or by keyword, on the model the method is called on.
/// Each case is a call of `search_domain.py`, found by its line.
/// A call to an override also evaluates to the base method, which declares `domain`,
/// so each error is reported once, whether the override declares `domain` or not.
#[test]
fn test_ols03011_search_methods() {
    // (the call as written, then the expected code and the text it points at, if invalid)
    let cases: &[(&str, Option<(&str, &str)>)] = &[
        ("self.search_count([('parent_id.name', '=', 'x')])", None),
        ("self.search_count([('unknown_count_field', '=', 'x')])", Some(("OLS03011", "'unknown_count_field'"))),
        ("self.search_fetch([('unknown_fetch_field', '=', 'x')], ['name'])", Some(("OLS03011", "'unknown_fetch_field'"))),
        ("self.search_read([('unknown_read_field', '=', 'x')])", Some(("OLS03011", "'unknown_read_field'"))),
        ("self._search([('unknown_private_field', '=', 'x')])", Some(("OLS03011", "'unknown_private_field'"))),
        // By keyword
        ("self.search_count(domain=[('unknown_count_kwarg_field', '=', 'x')])", Some(("OLS03011", "'unknown_count_kwarg_field'"))),
        ("self.search_read(domain=[('unknown_read_kwarg_field', '=', 'x')])", Some(("OLS03011", "'unknown_read_kwarg_field'"))),
        // The fields of the model searched, not of the class the call is in
        ("self.env['search_domain.kwargs'].search_count([('parent_id', '=', False)])", Some(("OLS03011", "'parent_id'"))),
        // Not a search method
        ("return self.with_context(domain=[('unknown_context_field', '=', 'x')])", None),
        // The other methods taking a domain
        ("self.filtered_domain([('unknown_filtered_field', '=', 'x')])", Some(("OLS03011", "'unknown_filtered_field'"))),
        ("self._read_group([('unknown_private_group_field', '=', 'x')], ['name'])", Some(("OLS03011", "'unknown_private_group_field'"))),
        ("self.name_search('x', [('unknown_name_search_field', '=', 'x')])", Some(("OLS03011", "'unknown_name_search_field'"))),
        // An override that does not declare `domain`, taking it in `*args` by position or in `**kwargs` by keyword
        ("self._search([('name', '=', 'x')])", None),
        ("self._search([('unknown_args_field', '=', 'x')])", Some(("OLS03011", "'unknown_args_field'"))),
        ("self._search(domain=[('unknown_kwargs_field', '=', 'x')])", Some(("OLS03011", "'unknown_kwargs_field'"))),
        // An override that declares `domain` too
        ("self.search_count([('unknown_declared_field', '=', 'x')])", Some(("OLS03011", "'unknown_declared_field'"))),
        ("self.search_count(domain=[('unknown_declared_kwarg_field', '=', 'x')])", Some(("OLS03011", "'unknown_declared_kwarg_field'"))),
    ];
    let failures = case_failures("search_domain.py", cases);
    assert!(
        failures.is_empty(),
        "search domain diagnostics:\n{}",
        failures.join("\n")
    );
}
