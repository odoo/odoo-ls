/// A comma-separated item of an order string, e.g. ` partner_id desc nulls last`.
pub struct OrderItem<'a> {
    pub text: &'a str,
    /// Start index of the item in the string
    pub start: usize,
}

impl<'a> OrderItem<'a> {
    /// Split an order string into its items, e.g. `"name desc, id"`.
    pub fn order_items(order: &str) -> Vec<OrderItem<'_>> {
        let mut start = 0;
        order
            .split(',')
            .map(|text| {
                let item = OrderItem { text, start };
                start += text.len() + 1;
                item
            })
            .collect()
    }

    /// End index of the item in the string
    pub fn end(&self) -> usize {
        self.start + self.text.len()
    }

    /// Whether `index` of the string is in the item, bounds included, e.g. a cursor right before a comma
    pub fn contains(&self, index: usize) -> bool {
        self.start <= index && index <= self.end()
    }

    /// The field of the item, i.e. its first word without the `.property`, with its start and end indexes in the string.
    /// e.g. `partner_id` in `partner_id.id desc`
    pub fn field(&self) -> Option<(&'a str, usize, usize)> {
        let (word, start) = self.first_word()?;
        let field = word.split('.').next().filter(|field| !field.is_empty())?;
        Some((field, start, start + field.len()))
    }

    /// The property of the field, with its start and end indexes in the string.
    /// e.g. `id` in `partner_id.id`
    pub fn property(&self) -> Option<(&'a str, usize, usize)> {
        let (word, start) = self.first_word()?;
        let (field, property) = word.split_once('.')?;
        if field.is_empty() || property.is_empty() {
            return None;
        }
        let property_start = start + field.len() + 1;
        Some((property, property_start, property_start + property.len()))
    }

    /// The keywords after the field, each with its start and end indexes in the string.
    /// e.g. `desc`, `nulls`, `last`
    pub fn keywords(&self) -> Vec<(&'a str, usize, usize)> {
        self.words()
            .skip(1)
            .map(|(word, start)| (word, start, start + word.len()))
            .collect()
    }

    /// The keywords allowed after the `previous` keywords of an item.
    /// A direction comes first, then where nulls go. Case-insensitive, as in Odoo.
    pub fn next_keywords(previous: &[&str]) -> &'static [&'static str] {
        let is = |word: &str, keyword: &str| word.eq_ignore_ascii_case(keyword);
        let is_direction = |word: &str| is(word, "asc") || is(word, "desc");
        match previous {
            [] => &["asc", "desc", "nulls"],
            [direction] if is_direction(direction) => &["nulls"],
            [nulls] if is(nulls, "nulls") => &["first", "last"],
            [direction, nulls] if is_direction(direction) && is(nulls, "nulls") => {
                &["first", "last"]
            }
            _ => &[],
        }
    }

    /// The first word of the item, with its start index in the string.
    pub fn first_word(&self) -> Option<(&'a str, usize)> {
        self.words().next()
    }

    /// The words of the item, each with its start index in the string.
    fn words(&self) -> impl Iterator<Item = (&'a str, usize)> {
        let mut end = 0;
        self.text.split_whitespace().map(move |word| {
            // Words come in order, so search each one after the previous one
            let start = end + self.text[end..].find(word).unwrap_or(0);
            end = start + word.len();
            (word, self.start + start)
        })
    }
}
