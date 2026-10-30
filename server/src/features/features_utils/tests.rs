use super::field_params::FieldParam;
use super::order_item::OrderItem;
use super::special_args::{SpecialArg, special_params_named};
use crate::odoo_version::OdooVersion;

#[test]
fn test_order_items() {
    let order = " name desc,  id ,, partner_id.id nulls last,";
    let items = OrderItem::order_items(order);
    let texts: Vec<_> = items
        .iter()
        .map(|item| &order[item.start..item.end()])
        .collect();
    assert_eq!(
        texts,
        [" name desc", "  id ", "", " partner_id.id nulls last", ""]
    );
    // Check the field and its property against the text at their indexes
    let fields: Vec<_> = items
        .iter()
        .map(|item| {
            item.field()
                .map(|(field, start, end)| (field, &order[start..end]))
        })
        .collect();
    assert_eq!(
        fields,
        [
            Some(("name", "name")),
            Some(("id", "id")),
            None,
            Some(("partner_id", "partner_id")),
            None
        ]
    );
    let properties: Vec<_> = items
        .iter()
        .map(|item| {
            item.property()
                .map(|(property, start, end)| (property, &order[start..end]))
        })
        .collect();
    assert_eq!(properties, [None, None, None, Some(("id", "id")), None]);
}

#[test]
fn test_order_item_keywords() {
    // `desc` also appears inside the field, and the words are separated by various spaces
    let order = "name, desc_date  desc nulls\tlast";
    let items = OrderItem::order_items(order);
    // Check each keyword against the text at its indexes
    let keywords: Vec<Vec<_>> = items
        .iter()
        .map(|item| {
            item.keywords()
                .into_iter()
                .map(|(keyword, start, end)| (keyword, &order[start..end]))
                .collect()
        })
        .collect();
    assert_eq!(
        keywords,
        [
            vec![],
            vec![("desc", "desc"), ("nulls", "nulls"), ("last", "last")]
        ]
    );
}

#[test]
fn test_order_next_keywords() {
    assert_eq!(OrderItem::next_keywords(&[]), ["asc", "desc", "nulls"]);
    assert_eq!(OrderItem::next_keywords(&["DESC"]), ["nulls"]);
    assert_eq!(OrderItem::next_keywords(&["nulls"]), ["first", "last"]);
    assert_eq!(
        OrderItem::next_keywords(&["asc", "Nulls"]),
        ["first", "last"]
    );
    // Nothing can follow a complete item or a wrong keyword
    assert!(OrderItem::next_keywords(&["desc", "nulls", "last"]).is_empty());
    assert!(OrderItem::next_keywords(&["dsc"]).is_empty());
}

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
    // Of the model methods, and of the ones `web` adds to all the models
    assert_eq!(kinds(v19_0, "domain"), [SpecialArg::Domain; 2]);
    assert_eq!(kinds(v19_0, "args"), [SpecialArg::Domain]);
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
