use super::case_failures;

/// OLS03014 and OLS03029 on `_order`: each case is a class of `model_order.py`, found by its
/// `_order` line. An invalid `_order` gets exactly one Error, pointing at the given text.
#[test]
fn test_ols03029_model_order() {
    // (the `_order` statement as written, then the expected code and the text it points at, if invalid)
    let cases: &[(&str, Option<(&str, &str)>)] = &[
        ("_order = 'name desc, id'", None),
        ("_order = 'name DESC NULLS LAST'", None),
        ("_order = 'parent_id.id desc'", None),
        ("_order = 'na' 'me'", None),
        (r#"_order = "name,\tid""#, None),
        ("_order = ''", None),
        ("_order = 'unknown_field'", Some(("OLS03014", "unknown_field"))),
        // Quotes and `:func` are not supported: looked up as written
        (r#"_order = '"name"'"#, Some(("OLS03014", r#""name""#))),
        ("_order = 'name:month desc'", Some(("OLS03014", "name:month"))),
        ("_order = 'name dsc'", Some(("OLS03029", "dsc"))),
        ("_order = 'name nulls'", Some(("OLS03029", "nulls"))),
        ("_order = 'name nulls firts'", Some(("OLS03029", "firts"))),
        ("_order = 'name nulls first desc'", Some(("OLS03029", "desc"))),
        ("_order = 'name nulls  first'", Some(("OLS03029", "nulls  first"))),
        ("_order = 'name,,id'", Some(("OLS03029", ","))),
        ("_order = 'name,'", Some(("OLS03029", ","))),
        // Two empty items around one comma: reported once
        ("_order = ','", Some(("OLS03029", ","))),
        // No comma: the empty item is reported at the start of the string
        ("_order = ' '", Some(("OLS03029", " "))),
        // Properties of other field types than many2one are not checked
        ("_order = 'name.id'", None),
        ("_order = 'parent_id.name'", Some(("OLS03029", "name"))),
        ("_order = '.id desc'", Some(("OLS03029", ".id"))),
        // A property is required after the dot, whatever the field
        ("_order = 'parent_id.'", Some(("OLS03029", "."))),
        ("_order = 'parent_id. desc'", Some(("OLS03029", "."))),
        ("_order = 'name.'", Some(("OLS03029", "."))),
        // Annotated assignment: validated like the plain one
        ("_order: str = 'unknown_field'", Some(("OLS03014", "unknown_field"))),
        // A constant is not validated: no diagnostic on `_order`, nor on the constant's string
        ("_order = ORDER", None),
        ("ORDER = 'unknown_field'", None),
        // `# noqa: OLS03014` on the class: only the wrong direction is reported
        ("_order = 'unknown_noqa_field dsc'", Some(("OLS03029", "dsc"))),
    ];
    let failures = case_failures("model_order.py", cases);
    assert!(failures.is_empty(), "_order diagnostics:\n{}", failures.join("\n"));
}

/// OLS03014 and OLS03029 on the `order` of a search method, by keyword or by position, on the model
/// searched: each case is a call of `search_order.py`, found by its line. Only a string literal
/// given to a search method is validated. A call to an override also evaluates to the base method,
/// which declares `order`: each error is reported once, whether the override declares `order` or not.
#[test]
fn test_ols03029_search_order() {
    // (the call as written, then the expected code and the text it points at, if invalid)
    let cases: &[(&str, Option<(&str, &str)>)] = &[
        ("self.search([], order='name desc, parent_id.id')", None),
        ("self.search([], order='name dsc')", Some(("OLS03029", "dsc"))),
        ("self.search([], order='unknown_search_field')", Some(("OLS03014", "unknown_search_field"))),
        // By position
        ("self.search([], 0, None, 'parent_id.name')", Some(("OLS03029", "name"))),
        // The other search methods
        ("self.search_fetch([], ['name'], order='name nulls')", Some(("OLS03029", "nulls"))),
        ("self.search_read([], order='name,,id')", Some(("OLS03029", ","))),
        // An empty order is the default order of the model
        ("self.search([], order='')", None),
        // Not a string literal
        ("self.search([], order=self._order)", None),
        // The fields of the model searched, not of the class the call is in
        ("self.env['search_order.kwargs'].search([], order='parent_id')", Some(("OLS03014", "parent_id"))),
        // Not a search method
        ("return self.with_context(order='unknown_context_field')", None),
        // Named like a search method, on a class that is not a model
        ("self.search('x', order='unknown_plain_field desc')", None),
        // An override that takes the order in `**kwargs`, or in `*args` by position
        ("self._search([], order='name desc')", None),
        ("self._search([], order='name dsc')", Some(("OLS03029", "dsc"))),
        ("self._search([], order='unknown_kwargs_field')", Some(("OLS03014", "unknown_kwargs_field"))),
        ("self._search([], 0, None, 'unknown_args_field')", Some(("OLS03014", "unknown_args_field"))),
        // An override that declares `order` too
        ("self.search_read([], order='unknown_declared_field')", Some(("OLS03014", "unknown_declared_field"))),
        ("self.search_read([], None, 0, None, 'name dsc')", Some(("OLS03029", "dsc"))),
    ];
    let failures = case_failures("search_order.py", cases);
    assert!(failures.is_empty(), "search order diagnostics:\n{}", failures.join("\n"));
}
