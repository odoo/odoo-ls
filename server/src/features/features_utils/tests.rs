
use super::order_item::OrderItem;

#[test]
fn test_order_items() {
    let order = " name desc,  id ,, partner_id.id nulls last,";
    let items = OrderItem::order_items(order);
    let texts: Vec<_> = items.iter().map(|item| &order[item.start..item.end()]).collect();
    assert_eq!(texts, [" name desc", "  id ", "", " partner_id.id nulls last", ""]);
    // The field and its property, with the text found at their indexes
    let fields: Vec<_> = items.iter().map(|item| item.field().map(|(field, start, end)| (field, &order[start..end]))).collect();
    assert_eq!(fields, [Some(("name", "name")), Some(("id", "id")), None, Some(("partner_id", "partner_id")), None]);
    let properties: Vec<_> = items.iter().map(|item| item.property().map(|(property, start, end)| (property, &order[start..end]))).collect();
    assert_eq!(properties, [None, None, None, Some(("id", "id")), None]);
}

#[test]
fn test_order_item_keywords() {
    // `desc` also appears inside the field, and the words are separated by various spaces
    let order = "name, desc_date  desc nulls\tlast";
    let items = OrderItem::order_items(order);
    // Each keyword, and the text found at its indexes
    let keywords: Vec<Vec<_>> = items.iter()
        .map(|item| item.keywords().into_iter().map(|(keyword, start, end)| (keyword, &order[start..end])).collect())
        .collect();
    assert_eq!(keywords, [vec![], vec![("desc", "desc"), ("nulls", "nulls"), ("last", "last")]]);
}

#[test]
fn test_order_next_keywords() {
    assert_eq!(OrderItem::next_keywords(&[]), ["asc", "desc", "nulls"]);
    assert_eq!(OrderItem::next_keywords(&["DESC"]), ["nulls"]);
    assert_eq!(OrderItem::next_keywords(&["nulls"]), ["first", "last"]);
    assert_eq!(OrderItem::next_keywords(&["asc", "Nulls"]), ["first", "last"]);
    // A complete item, and a wrong keyword: nothing can follow
    assert!(OrderItem::next_keywords(&["desc", "nulls", "last"]).is_empty());
    assert!(OrderItem::next_keywords(&["dsc"]).is_empty());
}
