//! Lexing and compiling of the JS expressions embedded in OWL templates.
//!
//! A port of OWL's own `inline_expressions.ts` — its tokenizer, its `${…}` /
//! `{{…}}` scans, and its word-operator rewrite — kept deliberately faithful, including
//! where OWL is naive: an expression OWL fails to compile is broken in the browser, so
//! compiling it any better here would only advertise features for code that cannot run.
//!
//! With one pass on top: OWL reads its attributes off a parsed DOM, so the XML entities are
//! long decoded by the time its compiler runs. We read raw XML, and decode them ourselves
//! ([`decode_entities`]).
//!
//! Everything is byte-oriented, and the rewrite is length-preserving, so a compiled
//! expression stays byte-for-byte aligned with the XML attribute value it came from —
//! which is what lets [`crate::features::owl_virtual`] map positions between the two with
//! a constant offset.

/// OWL's word-operators and their JS equivalents (`WORD_REPLACEMENT`) — exactly these six.
///
/// OWL swaps them inside its tokenizer, the instant an identifier is scanned, so the
/// rewrite is context-free: `foo.or` compiles to `foo.||` and `{or: 1}` to `{||: 1}`, both
/// syntax errors. The six words are effectively reserved everywhere in an OWL expression,
/// and [`compile_owl_expr`] reproduces that.
///
/// Replacements are padded to the word's length so the compiled text stays byte-aligned
/// with the XML source.
const OWL_WORD_OPS: &[(&str, &str)] = &[
    ("and", "&&"),
    ("or", "||"),
    ("gt", ">"),
    ("gte", ">="),
    ("lt", "<"),
    ("lte", "<="),
];

/// XML's five predefined entities. Character references (`&#8203;`, `&#x200b;`) are decoded
/// too; anything else is left alone, as XML would reject it anyway.
const XML_ENTITIES: &[(&str, char)] = &[
    ("&amp;", '&'),
    ("&lt;", '<'),
    ("&gt;", '>'),
    ("&quot;", '"'),
    ("&apos;", '\''),
];

/// The character an entity reference at byte `i` decodes to, and the reference's byte
/// length. `None` when `i` does not open one (a bare `&` is not an entity).
fn entity_at(expr: &str, i: usize) -> Option<(char, usize)> {
    let rest = expr.get(i..)?;
    for (name, decoded) in XML_ENTITIES {
        if rest.starts_with(name) {
            return Some((*decoded, name.len()));
        }
    }
    let digits = rest.strip_prefix("&#")?;
    let (radix, digits) = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => (16, hex),
        None => (10, digits),
    };
    let end = digits.find(';')?;
    let decoded = char::from_u32(u32::from_str_radix(&digits[..end], radix).ok()?)?;
    Some((decoded, rest.len() - digits.len() + end + 1))
}

/// Whether `byte` can be part of a multi-character JS operator — the one place padding may
/// never be inserted, since it would split the operator in two.
fn is_operator_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'=' | b'<' | b'>' | b'&' | b'|' | b'!' | b'?' | b'.' | b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'~'
    )
}

/// Decode the XML entities of an attribute value, padding with spaces to keep the source's
/// byte length.
///
/// OWL never sees an entity: it reads attributes off a parsed DOM, so `&amp;&amp;` is
/// already `&&` by the time its compiler runs. We slice the raw XML instead — that is what
/// keeps expression offsets equal to file offsets — so the decode OWL gets for free has to
/// happen here, and before the lexer, since `&quot;` is a string delimiter.
///
/// Padding trails the whole *operator cluster* an entity belongs to, never lands inside it:
/// `&amp;&amp;` decodes to `&&` and `&lt;=` to `<=`, not to `&    &` and `<   =`.
fn decode_entities(expr: &str) -> String {
    let mut out = String::with_capacity(expr.len());
    let mut i = 0;
    while i < expr.len() {
        if entity_at(expr, i).is_none() {
            let c = expr[i..].chars().next().expect("char boundary");
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        let (source_start, decoded_start) = (i, out.len());
        loop {
            if let Some((decoded, len)) = entity_at(expr, i) {
                out.push(decoded);
                i += len;
            } else if expr.as_bytes().get(i).is_some_and(|&b| is_operator_byte(b)) {
                out.push(expr.as_bytes()[i] as char);
                i += 1;
            } else {
                break;
            }
        }
        let padding = (i - source_start) - (out.len() - decoded_start);
        out.extend(std::iter::repeat_n(' ', padding));
    }
    out
}

/// Inner byte ranges of the `OPENER … CLOSER` chunks in `text`, delimiters excluded.
///
/// The shape of every scan OWL runs over a raw template: non-greedy — it stops at the
/// *first* closer, with no brace balancing — and never crossing a newline, since the
/// regexes it uses have no `s` flag. `closer_for` recognizes a two-byte opener and picks
/// the closer that matches it. All delimiters are ASCII, so a byte scan never splits a
/// multi-byte character.
fn owl_chunks(text: &str, closer_for: impl Fn(u8, u8) -> Option<&'static [u8]>) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let n = b.len();
    let mut out = vec![];
    let mut i = 0;
    while i + 1 < n {
        // Whichever opener (if any) sits at `i` fixes the matching closer.
        let Some(closer) = closer_for(b[i], b[i + 1]) else {
            i += 1;
            continue;
        };
        let inner = i + 2;
        let mut j = inner;
        let close = loop {
            if j + closer.len() > n || b[j] == b'\n' {
                break None;
            }
            if &b[j..j + closer.len()] == closer {
                break Some(j);
            }
            j += 1;
        };
        match close {
            Some(close) => {
                out.push((inner, close));
                i = close + closer.len();
            }
            None => i += 1,
        }
    }
    out
}

/// Inner byte ranges of each `{{ … }}` / `#{ … }` chunk of a string interpolation — one
/// embedded expression each. Faithful to OWL's `INTERP_REGEXP`.
pub(super) fn interp_chunk_ranges(value: &str) -> Vec<(usize, usize)> {
    owl_chunks(value, |a, b| match (a, b) {
        (b'{', b'{') => Some(b"}}".as_slice()),
        (b'#', b'{') => Some(b"}".as_slice()),
        _ => None,
    })
}

/// Inner byte ranges of each `${ … }` substitution of a template literal — the only part of
/// one that is code. Faithful to the regex OWL compiles them with in `processExpr`, which
/// runs over the raw token and is therefore escape-blind: `\${…}` is a substitution too.
fn subst_ranges(template: &str) -> Vec<(usize, usize)> {
    owl_chunks(template, |a, b| (a == b'$' && b == b'{').then_some(b"}".as_slice()))
}

/// Index just past the string literal opening at `open` (whose delimiter — `'`, `"` or a
/// backtick — is `bytes[open]`), or the end of input if it is unterminated. A `\` escapes
/// the next byte. Mirrors OWL's `tokenizeString`, which scans for the same delimiter and
/// nothing else: a backtick literal ends at the first unescaped backtick, so a *nested*
/// template literal terminates the outer one early, exactly as it does in OWL.
fn string_literal_end(bytes: &[u8], open: usize) -> usize {
    let delim = bytes[open];
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            c if c == delim => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Byte ranges of the identifier tokens in `expr`: the contents of a string literal are
/// skipped, but a template literal's `${ … }` substitutions are recursed into, since those
/// are code. Mirrors OWL, where `tokenizeString` runs before every other rule — so an
/// operator word inside a string is never seen — while a template literal's substitutions
/// are handed back to the compiler.
///
/// Ranges come out ascending and disjoint. A lightweight scan, not a full JS lexer:
/// comments and regex literals are not recognized, and neither are they by OWL.
fn ident_tokens(expr: &str) -> Vec<(usize, usize)> {
    let bytes = expr.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if matches!(c, b'\'' | b'"' | b'`') {
            let end = string_literal_end(bytes, i);
            if c == b'`' {
                for (start, stop) in subst_ranges(&expr[i..end]) {
                    let (start, stop) = (i + start, i + stop);
                    out.extend(
                        ident_tokens(&expr[start..stop])
                            .into_iter()
                            .map(|(s, e)| (start + s, start + e)),
                    );
                }
            }
            i = end;
        } else if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$') {
                i += 1;
            }
            out.push((start, i));
        } else {
            i += 1;
        }
    }
    out
}

/// Whether byte `offset` into `expr` lands on a bare `this` keyword — not a member name
/// (`foo.this`), not the literal text of a string. Both token edges count, matching how an
/// editor reports a cursor placed just before or just after a word.
pub(super) fn this_token_at(expr: &str, offset: usize) -> bool {
    let Some(&(start, end)) = ident_tokens(expr).iter().find(|&&(s, e)| s <= offset && offset <= e) else {
        return false;
    };
    if &expr[start..end] != "this" {
        return false;
    }
    // A single preceding `.` is a member access (`foo.this`, `foo?.this`); a `...` is a
    // spread, which leaves the keyword intact.
    let before = expr[..start].trim_end();
    !before.ends_with('.') || before.ends_with("...")
}

/// Compile one OWL expression, as sliced from the XML, to JS: decode the XML entities
/// ([`decode_entities`]), then rewrite OWL's word-operators (`and`/`or`/`gt`/…) to their JS
/// equivalents — context-free, like OWL's own rewrite (see [`OWL_WORD_OPS`]).
///
/// Both passes pad their replacements, so the result is byte-for-byte the length of the
/// source and an offset into one is an offset into the other.
pub(super) fn compile_owl_expr(expr: &str) -> String {
    let expr = decode_entities(expr);
    let mut out = String::with_capacity(expr.len());
    let mut copied = 0;
    for (start, end) in ident_tokens(&expr) {
        out.push_str(&expr[copied..start]);
        copied = end;
        let word = &expr[start..end];
        match OWL_WORD_OPS.iter().find(|(k, _)| *k == word) {
            Some((_, op)) => {
                out.push_str(op);
                // Pad with spaces so the replacement keeps the word's byte length.
                out.extend(std::iter::repeat_n(' ', word.len() - op.len()));
            }
            None => out.push_str(word),
        }
    }
    out.push_str(&expr[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::owl_xml_utils::{is_owl_expression_attr, is_owl_interpolation_attr, is_prop_expr_attr, tag_is_component};
    use oxc::{allocator::Allocator, parser::Parser, span::SourceType};

    /// Every compiled expression must keep its source's byte length, or the XML offsets the
    /// virtual doc maps through would drift.
    #[track_caller]
    fn assert_compiles_to(expr: &str, expected: &str) {
        let compiled = compile_owl_expr(expr);
        assert_eq!(compiled, expected);
        assert_eq!(compiled.len(), expr.len(), "byte-length drift for {expr:?}");
    }

    #[test]
    fn word_ops_are_length_preserving_and_correct() {
        // `or` (2) -> `||` (2): identical length, value swapped.
        assert_compiles_to("a or b", "a || b");
        // `and` (3) -> `&&` (2) padded to 3 with a trailing space.
        assert_compiles_to("x and y", "x &&  y");
        // `gt`/`lt` (2) -> `>`/`<` (1) padded to 2.
        assert_compiles_to("a gt b", "a >  b");
        assert_compiles_to("a lte b", "a <=  b");
        // Only whole identifiers, and never a string's contents.
        assert_compiles_to("orange and android", "orange &&  android");
        assert_compiles_to("'a or b'", "'a or b'");
        assert_compiles_to("\"a or b\"", "\"a or b\"");
        assert_compiles_to("'\\'and' or x", "'\\'and' || x");
    }

    #[test]
    fn word_ops_are_reserved_everywhere_like_owl() {
        // OWL swaps word-ops in its tokenizer, before anything knows what precedes them, so
        // a property named after one compiles to invalid JS (`ctx['foo'].||`) and the
        // template cannot run. We reproduce it rather than accepting code OWL rejects.
        assert_compiles_to("a.or", "a.||");
        assert_compiles_to("{or: 1}", "{||: 1}");
        // The identifier scan is maximal-munch, so only the exact word is an operator.
        assert_compiles_to("state.or_flag", "state.or_flag");
    }

    #[test]
    fn xml_entities_are_decoded_and_padded_past_the_operator() {
        // The workhorse: `&amp;&amp;` is one `&&`, not `&    &`.
        assert_compiles_to("a &amp;&amp; b", &format!("a &&{} b", " ".repeat(8)));
        // A lone entity, and one whose operator is completed by a literal char: the padding
        // clears the whole cluster, so `<=` does not come out as `<   =`.
        assert_compiles_to("a &lt; b", &format!("a <{} b", " ".repeat(3)));
        assert_compiles_to("a &lt;= b", &format!("a <={} b", " ".repeat(3)));
        assert_compiles_to("a &gt;= b", &format!("a >={} b", " ".repeat(3)));
        // Character references, decimal and hex, padded to the reference's byte length.
        assert_compiles_to("x || '&#8203;'", &format!("x || '\u{200b}{}'", " ".repeat(4)));
        assert_compiles_to("x || '&#x200b;'", &format!("x || '\u{200b}{}'", " ".repeat(5)));
        // `&quot;` is a string delimiter, so it has to be decoded before the lexer runs: the
        // `or` here is a string's contents, not an operator.
        assert_compiles_to("x == &quot;a or b&quot;", &format!("x == \"{}a or b\"{}", " ".repeat(5), " ".repeat(5)));
        // A bare `&` and an undeclared entity are not references; they pass through.
        assert_compiles_to("a & b", "a & b");
        assert_compiles_to("a &nope; b", "a &nope; b");
    }

    #[test]
    fn entities_are_decoded_before_the_word_ops() {
        // account/grouped_view_widget.xml-flavoured: both passes on one expression, each
        // padding its own replacement, and the total length is the source's.
        assert_compiles_to("a &lt; b and c", &format!("a <{} b &&  c", " ".repeat(3)));
    }

    #[test]
    fn poc_expression_compiles_verbatim_except_or() {
        assert_compiles_to(
            "this.props.record.data[this.props.name] or ''",
            "this.props.record.data[this.props.name] || ''",
        );
    }

    #[test]
    fn word_ops_inside_template_literal_substitutions_are_compiled() {
        // base_import/static/src/import_data_content/import_data_content.xml:83 — a live
        // template. The `and` is inside `${…}`, i.e. code, and must be rewritten; the string
        // literal alongside it must not be.
        assert_compiles_to(
            "`${choice.value and choice.required ? 'fw-bolder' : ''}`",
            "`${choice.value &&  choice.required ? 'fw-bolder' : ''}`",
        );
        // Literal text of a template literal is not code.
        assert_compiles_to("`a or b`", "`a or b`");
        // Several substitutions, and text around them.
        assert_compiles_to("`${a or b} x ${c and d}`", "`${a || b} x ${c &&  d}`");
    }

    #[test]
    fn template_literal_scan_is_as_naive_as_owls() {
        // Non-greedy: the substitution ends at the *first* `}`, so a brace inside it cuts the
        // scan short and the tail is left uncompiled. OWL emits the same broken code.
        assert_compiles_to("`${ {a: 1}.a or b }`", "`${ {a: 1}.a or b }`");
        // No `s` flag on OWL's regex: a substitution spanning a newline is not compiled.
        assert_compiles_to("`${a\nor b}`", "`${a\nor b}`");
        // A nested template literal closes the outer one early (`tokenizeString` scans for its
        // delimiter and nothing else), so what follows it is lexed as bare code and the word-op
        // in it *is* rewritten. OWL lands in the same place, for the same reason:
        // `` `a${`b or c`}d` `` compiles to `` `a${`ctx['b']||ctx['c']`}d` ``.
        assert_compiles_to("`a${`b or c`}d`", "`a${`b || c`}d`");
        // An unterminated literal swallows the rest of the expression.
        assert_compiles_to("'a or b", "'a or b");
    }

    #[test]
    fn subst_ranges_matches_owls_replace_regexp() {
        let slices = |v: &str| -> Vec<String> {
            subst_ranges(v).into_iter().map(|(s, e)| v[s..e].to_string()).collect()
        };
        assert_eq!(slices("`a ${x.y} b ${z}`"), vec!["x.y", "z"]);
        // Non-greedy, no brace balancing: it cuts at the first `}`.
        assert_eq!(slices("`${ {a: 1}.a }`"), vec![" {a: 1"]);
        // Escape-blind, like the regex: it runs over the raw token.
        assert_eq!(slices("`\\${x}`"), vec!["x"]);
        // No opener, unterminated, or newline-crossing -> no substitution.
        assert!(subst_ranges("`plain`").is_empty());
        assert!(subst_ranges("`${ oops`").is_empty());
        assert!(subst_ranges("`${ a\n}`").is_empty());
    }

    #[test]
    fn interp_chunk_ranges_matches_owl_regexp() {
        let slices = |v: &str| -> Vec<String> {
            interp_chunk_ranges(v).into_iter().map(|(s, e)| v[s..e].to_string()).collect()
        };
        // Both delimiter forms, delimiters excluded, literal text between them ignored.
        assert_eq!(slices("a {{x.y}} b #{z} c"), vec!["x.y", "z"]);
        // Whole value is one chunk.
        assert_eq!(interp_chunk_ranges("{{expr}}"), vec![(2, 6)]);
        assert_eq!(slices("{{expr}}"), vec!["expr"]);
        // Adjacent chunks.
        assert_eq!(slices("{{a}}{{b}}"), vec!["a", "b"]);
        // Non-greedy: stops at the first closer (an object literal still works — its single `}`
        // is not `}}`).
        assert_eq!(slices("{{ {'k': v} }}"), vec![" {'k': v} "]);
        // Literal-only, and unterminated / newline-crossing openers → no chunk (faithful to OWL).
        assert!(interp_chunk_ranges("plain class").is_empty());
        assert!(interp_chunk_ranges("{{ oops").is_empty());
        assert!(interp_chunk_ranges("{{ a\n}}").is_empty());
    }

    #[test]
    fn this_token_at_matches_only_the_bare_keyword() {
        // Every offset within the keyword, and both its edges, count.
        for offset in 0..=4 {
            assert!(this_token_at("this.x", offset), "offset {offset}");
        }
        assert!(!this_token_at("this.x", 5)); // on `x`, a member name
        assert!(this_token_at("this", 0));
        assert!(this_token_at("(this)", 2));

        // A member named `this` is not the keyword; the keyword before the dot still is.
        assert!(!this_token_at("foo.this", 5));
        assert!(!this_token_at("foo ?. this", 8));
        assert!(this_token_at("this.this", 1));
        assert!(!this_token_at("this.this", 6));

        // A spread is not a member access: the keyword survives the leading dots.
        assert!(this_token_at("{...this.props}", 5));

        // Inside a string literal there is no token at all.
        assert!(!this_token_at("'this'", 2));
        assert!(!this_token_at("\"this\"", 2));
        assert!(!this_token_at("`this`", 2));
        assert!(!this_token_at("'\\'this'", 4));
        // But a real `this` after a string still resolves.
        assert!(this_token_at("'a' + this", 7));

        // Words merely containing `this` are other identifiers.
        assert!(!this_token_at("things", 2));
        assert!(!this_token_at("_this", 2));
        assert!(!this_token_at("this_x", 2));
    }

    #[test]
    fn this_token_at_reaches_into_template_literal_substitutions() {
        // `${…}` is code: the keyword in it is the real one.
        assert!(this_token_at("`${this.props.x}`", 4));
        assert!(!this_token_at("`${this.props.x}`", 9)); // on `props`, a member name
        // Text outside the substitution stays literal.
        assert!(!this_token_at("`this ${this.x}`", 2));
        assert!(this_token_at("`this ${this.x}`", 9));
    }

    /// Two `t-ref` values Odoo ships that OWL cannot compile either. Owl 3 compiles `t-ref` as
    /// a plain expression (`code_generator.ts:688`), not an interpolation, so its own output is
    /// `ctx['fullCalendar']-{{month:ctx['month']}}` — a SyntaxError in the browser. Owl-2
    /// templates the migration has not reached; we are faithfully broken alongside them.
    const BROKEN_IN_OWL_TOO: &[&str] = &[
        "fullCalendar-{{ month }}",
        "{{tagEquals(tag, state.tagToUpdate) ? 'tagToUpdate' : `tag_${tag_index}`}}",
    ];

    /// Whether OWL compiles `expr` by a rewrite our virtual docs have no room for — verified by
    /// running Odoo's `owl.js` (3.0.0-alpha.41) on each form:
    ///
    /// - **a JS keyword as a plain name.** OWL routes bare identifiers through `ctx['…']`
    ///   (`inline_expressions.ts:377`), so `class` is a perfectly good name in a template
    ///   (`t-att-class="class"`, `{ 'class': class }` → `ctx['class']`) and can never be one in
    ///   the JS we splice. A member (`props.class`) or a key (`{class: 1}`) is plain JS, and
    ///   stays checked.
    /// - **`this` as an object-literal shorthand.** OWL splices the missing `: value` into a
    ///   shorthand key (`:317-338`), so `{ this, props: … }` compiles for it and not for JS.
    ///
    /// Neither rewrite can be done byte-aligned — `{ this }` → `{ this: this }` is longer than
    /// what it replaces, and a keyword has no shorter name to take.
    fn is_owl_only(expr: &str) -> bool {
        ident_tokens(expr).into_iter().any(|(start, end)| {
            let before = expr[..start].trim_end();
            let after = expr[end..].trim_start();
            match &expr[start..end] {
                "class" => !before.ends_with('.') && !after.starts_with(':'),
                "this" => {
                    (before.ends_with('{') || before.ends_with(','))
                        && (after.starts_with('}') || after.starts_with(','))
                }
                _ => false,
            }
        })
    }

    fn parses_as_js(compiled: &str) -> bool {
        let allocator = Allocator::default();
        Parser::new(&allocator, compiled, SourceType::mjs()).parse_expression().is_ok()
    }

    /// Keeps the corpus check's exclusions honest, in both directions: an excluded expression
    /// that starts compiling is masking the check for nothing, and an excluded *shape* that
    /// swallows ordinary JS would hide real defects.
    #[test]
    fn known_unrepresentable_expressions_stay_that_way() {
        for expr in BROKEN_IN_OWL_TOO {
            assert!(!parses_as_js(&compile_owl_expr(expr)), "{expr:?} compiles now — drop it");
        }
        for expr in [
            "class",
            "{ 'class': class }",
            "{ __comp__: Object.assign(Object.create(this), { this, props: { ...this.props, record: this.model.root } }) }",
        ] {
            assert!(is_owl_only(expr), "{expr:?} no longer reads as OWL-only");
            assert!(!parses_as_js(&compile_owl_expr(expr)), "{expr:?} compiles now — drop the exclusion");
        }
        // Ordinary JS that merely mentions `class` or `this` must stay in the corpus.
        for expr in ["props.class", "{class: 1}", "this.props.x", "{...this.props}", "f(a, this)", "[this, x]"] {
            assert!(!is_owl_only(expr), "{expr:?} wrongly excluded as OWL-only");
            assert!(parses_as_js(&compile_owl_expr(expr)), "{expr:?} should compile");
        }
    }

    /// The expression texts under `node`, by the attribute rules the virtual docs are built
    /// with (`owl_virtual::collect_exprs`) — a corpus source for [`compile_owl_expr`].
    fn template_exprs(node: roxmltree::Node, xml: &str, in_template: bool, out: &mut Vec<String>) {
        if !node.is_element() {
            return;
        }
        let in_template = in_template || node.has_attribute("t-name");
        if in_template {
            let is_component = tag_is_component(node);
            for attr in node.attributes() {
                let name = attr.name();
                let range = attr.range_value();
                if range.end <= range.start {
                    continue;
                }
                let value = &xml[range.start..range.end];
                if is_owl_interpolation_attr(name) {
                    out.extend(
                        interp_chunk_ranges(value)
                            .into_iter()
                            .map(|(s, e)| value[s..e].to_string())
                            .filter(|text| !text.trim().is_empty()),
                    );
                } else if is_owl_expression_attr(name) || (is_component && is_prop_expr_attr(name)) {
                    out.push(value.to_string());
                }
            }
        }
        for child in node.children() {
            template_exprs(child, xml, in_template, out);
        }
    }

    /// Every expression in Odoo's own OWL templates must compile to *parseable* JS — the
    /// oracle for the whole module, since a compiled expression that does not parse is a
    /// virtual-doc function tsserver drops, silently taking hover/definition/completion for
    /// that expression with it. Needs `COMMUNITY_PATH`; skipped without one.
    #[test]
    fn community_templates_compile_to_parseable_js() {
        let Ok(community) = std::env::var("COMMUNITY_PATH") else {
            eprintln!("COMMUNITY_PATH unset — skipping the OWL template corpus check");
            return;
        };

        let mut checked = 0usize;
        let mut failures: Vec<String> = vec![];
        for path in glob::glob(&format!("{community}/**/static/src/**/*.xml")).expect("valid glob").flatten() {
            let Ok(xml) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(document) = roxmltree::Document::parse(&xml) else {
                continue;
            };
            let mut exprs = vec![];
            template_exprs(document.root_element(), &xml, false, &mut exprs);
            for expr in exprs {
                if BROKEN_IN_OWL_TOO.contains(&expr.as_str()) || is_owl_only(&expr) {
                    continue;
                }
                checked += 1;
                if !parses_as_js(&compile_owl_expr(&expr)) {
                    failures.push(format!("  {}\n    {expr}", path.display()));
                }
            }
        }

        assert!(checked > 0, "no OWL templates found under {community} — is COMMUNITY_PATH an Odoo checkout?");
        assert!(
            failures.is_empty(),
            "{} of {checked} expressions compile to unparseable JS:\n{}",
            failures.len(),
            failures.join("\n"),
        );
    }
}
