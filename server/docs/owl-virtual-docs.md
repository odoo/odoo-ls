# JS language features in OWL templates — virtual documents

OdooLS provides hover, go-to-definition, completion, find-references and semantic tokens for
the JavaScript *expressions* embedded in XML OWL templates (`t-if`, `t-out`, `t-att-*`,
`t-on-*`, component props, …). The host file is XML, so tsserver cannot see those expressions
natively — and only tsserver knows the real member types (they depend on imports, base
classes, and `setup()` assignments that our in-house OXC pass cannot resolve).

The mechanism: per component, build a small **virtual `.js` document** that reconstructs the
exact evaluation context of each template expression, open it in tsserver, forward the LSP
request at the mapped position, and map results back onto the XML.

Main code: `src/features/owl_virtual.rs` (doc assembly, mapping, feature entry points),
`src/features/owl_expr.rs` (lexing and compiling an OWL expression to JS),
`src/core/tsserver_bridge.rs` (tsserver protocol), `src/core/js_import_graph.rs` (references
expansion), `src/core/js_arch_builder.rs` + `src/core/file_mgr.rs::build_js_ast` (the OXC
pass feeding the indexes).

Targets **OWL 3**: `props` is a declared, natively-typed instance field; `env` is gone; all
member access in a template is explicit `this.*`.

## 1. The evaluation context of a template expression

Ground truth from the Owl compiler (`compileExpr` / `parseComponent` in owl.js):

- A template expression sees `this` (the component instance) and the **template locals** in
  scope (`t-set`, `t-foreach`/`t-as`) — *nothing else*. A bare identifier compiles to
  `ctx['name']`; module scope of the component's file is **not** visible.
- Word operators are rewritten before evaluation: `and`→`&&`, `or`→`||`, `gt`→`>`,
  `gte`→`>=`, `lt`→`<`, `lte`→`<=`. The swap happens inside OWL's *tokenizer*, the instant an
  identifier is scanned — so it is context-free, and those six words are reserved **everywhere**
  in an expression, property names and object keys included.
- OWL never sees an XML entity: it reads its attributes off a parsed DOM, so `&amp;&amp;` is
  already `&&` by the time `compileExpr` runs.
- A capitalized tag (`<ChangeLine/>`) or `<t t-component="expr">` is a sub-component. A
  static tag name resolves through the parent's `static components` mapping
  (`parent.constructor.components["Tag"]`), not through imports.
- On a component element, every non-`t-` attribute is a **prop** whose value is a JS
  expression evaluated in the *parent's* context (including `class`/`style`; sole exception:
  the `.translate` suffix, which is a plain string).
- `t-attf-*` and dynamic `t-call="{{…}}"` are string *interpolations*: literal text with one
  or more `{{ … }}` / `#{ … }` chunks, each chunk an embedded expression.

## 2. The virtual document

One doc per **component** (not per file): an import of the component class plus one synthetic
function per expression, with the expression preceded by a preamble declaring its in-scope
template locals:

```js
import { Counter } from "./counter";
/** @this {Counter} */ function __ols_m0() { return (this.props.count); }
/** @this {Counter} */ function __ols_m1() { let rec = (this.records)[0]; …; return (rec.name); }
```

Why this exact shape:

- **`@this {Class}`** types `this` as the *imported real* class, so tsserver resolves every
  `this.member` natively — including expando members assigned in `setup()` — and a
  `this.member` reference binds to the real member symbol, not a copy. That symbol identity
  is what makes definition land in the real file and references work with a single query.
- **No copied real code.** Faithful to Owl (no module scope in templates), and it keeps the
  docs tiny — a references query stages dozens of them (§5).
- **Must be `.js`** (`scriptKindName: "JS"`). checkJs *expando inference* (`this.x = …` in
  `setup()` becoming a typed class property) is JS-only; a `.ts` doc silently degrades every
  setup-assigned member to `any`.
- The doc lives **next to the real file** (`<stem>.<Class>.__ols_owl__.js`) so relative
  imports and the `@module/*` alias map resolve identically. Keying the path on the class
  name gives each component of a shared file its own doc.
- Docs are opened without `geterr`: they must never publish diagnostics.

### Byte alignment and mapping

The compiled expression inside the doc has **byte-for-byte the length of** the XML attribute
value it came from: every replacement is padded (`and` → `"&& "`). So a position inside a
spliced expression maps between XML and virtual doc by adding/subtracting a single offset
(`SplicedExpr { v_byte_start, xml_byte_start, … }`). This is the constraint the whole
compilation step is built around — see below.

Three coordinate spaces exist: the **XML** (cursor in, template results out — session
encoding), the **real `.js`** (definition/reference targets — reached through the import, so
tsserver reports them there directly), and the **virtual doc** (tsserver speaks UTF-16;
converted at the boundary via `ruff_source_file::LineIndex`, everything internal is bytes).

Every tsserver result against a doc goes through one triage, `map_virtual_ref`:
a hit in another real file passes through; a hit inside a spliced expression maps onto the
XML; a hit on a preamble `let` maps to its `t-set`/`t-as` declaration; a hit in this doc's
shim identity-maps to the real file (§4); anything else virtual is dropped (leak guard —
internal paths must never reach the client).

### Compiling the expression (`features/owl_expr.rs`)

An OWL expression is not JavaScript, so getting one into the doc is a port of OWL's own
`inline_expressions.ts`. The guiding rule is **fidelity, not correctness**: an expression OWL
cannot compile is broken in the browser, so compiling it *better* than OWL would only light up
hover/definition/completion for code that cannot run. Where OWL is naive, we are naive in the
same places — and its own header calls it "an extremely naive tokenizer/parser".

Two passes, in this order, both length-preserving:

1. **XML entities** (`decode_entities`) — the one thing OWL does not do, because the DOM has
   already done it for it (§1). We slice raw XML, which is what keeps expression offsets equal
   to file offsets, so the decode has to happen here — and *before* the lexer, since `&quot;`
   is a string delimiter and nothing downstream can see the string until it is one. Around 102
   expressions in community depend on this; before it, each of them was a syntax error in the
   doc, silently dead for every feature.
2. **Word operators** (`compile_owl_expr`) — OWL's six, no more (`not` is not one of them).

Both pad with spaces to preserve byte length, and both have a trap worth stating:

- Padding must never land **inside an operator**. Padding each entity individually turns
  `&amp;&amp;` into `&    &` and `&lt;=` into `<   =`, neither of which is JS. Padding
  therefore trails the whole operator *cluster* an entity belongs to.
- Because OWL's word-op swap is context-free (§1), `foo.or` compiles to `foo.||` and
  `{or: 1}` to `{||: 1}` — SyntaxErrors here exactly as in the browser. We **reproduce** that
  rather than guard against it; a guard would advertise features for a template that crashes.

The lexer (`ident_tokens`) skips the contents of string literals but recurses into a template
literal's `${…}` substitutions, mirroring where OWL puts them: `tokenizeString` treats a
backtick like any other delimiter, and `processExpr` compiles the substitutions afterwards, by
regex (`/\$\{(.*?)\}/g`). That regex is non-greedy, brace-blind and newline-blind — a
substitution ends at the *first* `}` and never crosses a line — and `owl_chunks` reproduces
those exact rules, for `${…}` and for `INTERP_REGEXP`'s `{{…}}` / `#{…}` alike.

#### The corpus test

`community_templates_compile_to_parseable_js` (gated on `COMMUNITY_PATH`) compiles **every**
expression in Odoo's OWL templates and asserts the result parses under OXC. It is the oracle
for this module: a compiled expression that does not parse is a doc function tsserver drops,
silently taking hover, definition and completion for that expression with it — a failure mode
with no other symptom. It earned its keep immediately: both the entity bug and the `${…}` bug
were found by it, not by reading OWL's sources, where neither is visible.

Of 28,622 expressions, three shapes are excluded — each checked by running Odoo's own vendored
`owl.js` (3.0.0-alpha.41) on it:

| Shape | Why OWL accepts it | Why we cannot |
|---|---|---|
| `t-ref="fullCalendar-{{ month }}"` | **It doesn't.** Owl 3 compiles `t-ref` as a plain expression, not an interpolation (`code_generator.ts:688`), so OWL itself emits `ctx['fullCalendar']-{{month:ctx['month']}}` | Nothing to fix — two un-migrated Owl-2 templates, broken in Odoo today (`BROKEN_IN_OWL_TOO`) |
| `class`, `{ 'class': class }` | a bare identifier becomes `ctx['class']`, so a JS keyword is a perfectly good template *name* | our docs bind `this` and the locals directly; a keyword has no shorter name to take |
| `{ this, props: … }` | OWL splices the missing `: value` into a shorthand key | `{ this: this }` is longer than `{ this }` — no room, byte-aligned |

The last two are excluded by a **predicate** (`is_owl_only`), not a list of expression texts:
`class` is a *name* Odoo reuses, so new expressions carrying it keep appearing, and an
exact-text list would go red on upstream churn. `known_unrepresentable_expressions_stay_that_way`
pins the exclusion in both directions — an excluded form that starts compiling must be
un-excluded, and ordinary JS that merely mentions `class` or `this` (`props.class`,
`{class: 1}`, `f(a, this)`) must stay in the corpus, so the predicate cannot quietly shrink it.

### Template locals

`collect_exprs` threads scope through the XML walk, mirroring Owl's rules: a
`t-foreach`/`t-as` var is visible to the element's other attributes and descendants (but not
to the `t-foreach` collection expression itself); a `t-set` var is visible to *following*
siblings and their descendants. The preamble declares each local as a `let`:

- `t-set` + `t-value` → `let x = (EXPR);`
- `t-set` body form → `any` (the body is rendered markup)
- `t-foreach="COLL" t-as="x"` → the five Owl bindings: `x`, `x_value` (both typed
  `(COLL)[0]`), `x_index`, `x_first`, `x_last`. Object/Map *keys* are knowingly mistyped as
  the element (the faithful form needs conditional types; affects hover only).

Shadowed names are deduped (innermost wins) to avoid `let` redeclaration errors. Each emitted
`let` is recorded (`LocalDecl`) so go-to-definition on a local lands on its XML declaration.

### Component tags

A static tag `<ChangeLine/>` splices through a local:

```js
const ChangeLine = Class.components["ChangeLine"]; return (ChangeLine);
```

`Class.components["Tag"]` is how Owl itself resolves a tag, so an aliased entry
(`static components = { Line: Child }`), a spread, or one inherited from a superclass all
resolve — and the local *holds the class*, so tsserver types it `typeof Child` and classifies
it as a `class`. The anchor is the local's use in the `return`, not its declaration, which
would carry a `declaration` token modifier the XML tag must not have.

### Definition asks for the *type* on a tag and on `this`

Two cursors want the type of the expression rather than its declaration, and in both cases
that type is a component class: a **component tag** (whose declaration is the synthetic
`const Tag` above, not the class it holds) and a bare **`this`** (the `@this` tag types it but
declares nothing, so `definition` has no answer at all). `definition_xml_owl` sends
`typeDefinition` for those two and `definition` for everything else; the triage of the result
is the same either way. Both used to be answered in-house from `component_descriptors` — the
tag lookup was keyed on the tag name, so an aliased tag silently resolved to any same-named
class in the workspace.

One in-house answer remains: template-name navigation (`t-name`/`t-call`/`t-inherit` values,
resolved against `js_templates` / `js_component_by_template` without any virtual doc).

## 3. Feature wiring

All cursor features share one prologue: build the docs for the XML file, find the doc whose
spliced expression contains the cursor, open it (plus its shim, if any), query tsserver at
the mapped UTF-16 position.

| Feature | Entry point | Notes |
|---|---|---|
| Hover | `hover_xml_owl` | `quickinfo`, passed through as markdown. |
| Completion | `completion_xml_owl` | edits/resolve-data referencing the virtual doc are stripped. |
| Definition | `definition_xml_owl` | tsserver + triage; `typeDefinition` on a tag or a bare `this`, `definition` otherwise. |
| Semantic tokens | `semantic_tokens_xml` | whole-file: classify every doc, remap spans inside expressions. |
| References | `references_js_owl` / `references_xml_owl_member` | §5. |

Semantic-tokens caveat (tsserver-inherent): the classifier does not tokenize plain property
*reads* — `this.foo()` colors, `this.props.x` does not. Template-name strings
(`t-name`/`t-call`/`t-inherit`, and the JS `static template` string) get a `type` token
in-house, reference sites gated on actually resolving.

## 4. The shim (non-exported components)

~9% of components are not exported (or are exported under another name), so the doc cannot
`import { Class }` from the real module. For those files we build one **shim** per file:
the real source **verbatim** plus a trailing `export { A, B };` naming every non-exported
component. The doc then imports the class from the shim
(`<stem>.__ols_shim__.js`).

Consequences:

- The doc's scope stays minimal — the shim absorbs the module copy, so no module scope leaks
  into template expressions.
- The shim's prefix is byte-identical to the real file, so any tsserver hit inside the shim
  identity-maps back to the real file (a hit in the appended `export` suffix is dropped).
- **Symbol identity is lost**: the doc's `this.member` binds to the *shim's* member, a
  symbol distinct from the real one. This is the only case where a references query needs a
  second query (Query B, §5) — for exported components everything binds to the real symbol.

## 5. Find-references

### The problem

tsserver's program is `rootFiles` plus their **forward** import closure; module resolution
never runs backwards. We keep the project lean (only client-open files are pinned roots — no
whole-tree `jsconfig.json`), so the callers of a symbol are usually *not in the program* and
a plain `references` request misses them. Before each query we must expand the project with
every file that could contain a reference.

### The reverse import graph (`core/js_import_graph.rs`)

The OXC parser already records every module specifier while parsing
(`FileInfoAst::{js_imports, js_reexports}` — free to collect). From these the graph is
rebuilt on demand (~15k edges in milliseconds; nothing can go stale) with resolution
mirroring the `@module/*` alias map handed to tsserver.

Naively the files to add are all transitive importers — wildly over-inclusive. The insight:
a reference to `A.member` can only bind where a value of type `A` can arrive, and `A`-ness
propagates along exactly two edges — a **re-export** (`export … from "a"`) and a **subclass
declaration** (`class B extends A`). Everything else is a leaf consumer. So:

```
exposers(A) = closure of declFile(A) under re-export and subclass edges
roots       = exposers ∪ direct importers of any exposer
```

Measured on the community codebase this cuts the p99 program from ~2450 files to ~620.
Over-listing roots is harmless (tsserver dedups); under-listing loses references — so roots
are listed exhaustively and never capped.

### Anchoring at the declaring file

Expansion is anchored at the symbol's **declaring** file(s), resolved by one `definition`
round trip first, unioned with the cursor's file as a floor (a declaration in a `.d.ts`,
e.g. an Owl `Component` member, is outside the graph — the floor prevents the root set from
collapsing). Anchoring at the declaration is what finds the far-away callers of an imported
symbol, and reaches the *cousin* case for inherited members: member declared in ancestor
`A`, cursor in subclass `B`, use in sibling subclass `C`'s template.

### Template uses: subclass docs, Query A / Query B

Template uses of a member live in virtual docs, so the query also stages the origin
component's docs and the docs of every (transitive) subclass of the anchor classes — an
inherited `this.member` in a subclass template binds to the same real base member.

- **Query A** — `references` on the real member (at the real file / resolved anchors). With
  minimal docs, template `this.member` binds to the imported real member, so this single
  query returns the JS callers *and* every template use (arriving at doc paths, remapped
  onto their XML). Hits inside a *foreign* doc's real-code copy are dropped: the subclass's
  real file is itself a root (it imports the origin), so tsserver already reported the hit
  there.
- **Query B** — only for **shim-backed** components (§4): their doc member is a distinct
  symbol Query A cannot see, so the doc (XML origin) or the shim (JS origin — byte-identical
  prefix keeps the cursor valid) is queried too, and results de-duplicated.

### Staging, committing, and the budget

Every `openExternalProject` re-send triggers a synchronous whole-program rebuild, so all
staging (docs, shims, expansion roots) is batched and **committed once** per query
(`stage_* / commit_transient_roots` in the bridge). An XML-origin query needs two commits:
the cursor's doc alone first (so `definition` can resolve the member and produce the
anchors), then everything anchored on the result.

Expansion roots and virtual docs are **transient** roots, kept between queries so repeated
queries stay warm. `TRANSIENT_ROOT_BUDGET` bounds *retention only* — never the answer: when
the accumulated set outgrows it, the *next* query drops it first and re-expands (lazy, so
the rebuild never lands inside the request the user is waiting on). Eviction must `close`
the open virtual docs — an open file keeps its `ScriptInfo` pinned regardless of roots.

Expansion roots are never `open`ed (tsserver reads them from disk): no content transfer, no
diagnostics for files the user never opened.

## 6. Indexes involved

| Index | Where | Content / role |
|---|---|---|
| `component_descriptors` | `SyncOdoo`, fed by `js_arch_builder` | class name → file, name byte offset, `super_class_name`, `export_kind`. Drives doc building, subclass graph, tag/`this` definitions. |
| `js_component_by_template` | `SyncOdoo`, fed by `js_validator` | template name → component class (1:1; duplicates resolve to the base class — see the FIXME in `odoo.rs`). |
| `js_templates` | `SyncOdoo` | template name → XML `<t t-name>` symbols. Template-name navigation and doc↔XML pairing. |
| `js_template_refs` | per-file `FileInfoAst` | `static template = "name"` sites (range, name, class). Template-name references; XML files backing a JS file. |
| `js_imports` / `js_reexports` | per-file `FileInfoAst` | module specifiers, verbatim. Feed the import graph. |
| import graph | `js_import_graph::ImportGraph` | reversed importer/re-exporter maps; rebuilt per query. |

## 7. tsserver project management

- One **external project** (`openExternalProject`) with a `paths` map translating
  `@module/*` aliases — this is what resolves Odoo imports without any `jsconfig.json` (a
  root jsconfig would supersede it and balloon the program; onboarding recommends removing
  it).
- `typeRoots` points at Odoo's own `@types` dirs (stops ambient `node_modules/@types`
  leakage); ATA is disabled; `@odoo/o-spreadsheet` is deliberately not aliased (2.9 MB
  minified bundle, no `.d.ts`).
- **Content before project rebuild**: a doc's `open` (with content) must be sent before the
  `openExternalProject` that makes it a root. tsserver processes requests FIFO and the
  project command runs a synchronous rebuild we block on — sending content first guarantees
  the very first request against a new doc sees real results (the pre-content ordering was
  the historical "first request returns empty" bug).

## 8. Known gaps / deferred

- `t-inherit` extension fragments (expressions inside `<xpath>`/`<attribute>` bodies) are
  not collected; deferred (see `docs/owl-virtual-doc-rework-plan.md` §3.C).
- Orphan `t-call`ed templates (no owning component) — the only genuine N:M case; deferred.
- Import-graph gaps: factory functions re-exposing a type, aliased imports
  (`import { A as B }`), dynamic `import()`.
- Closing tag names (`</ChangeLine>`) are not tokenized/navigable.
- `t-foreach` over plain objects/Maps mistypes the key vars (§2).
- `ident_tokens` is a lightweight scanner, not a full JS lexer: comments and regex literals
  are not recognized — but neither are they by OWL, so the divergence is nil. Object keys
  named `or`, and the naive `${…}` / `{{…}}` scans, are *deliberate* fidelity, not gaps (§2).
- Three expression shapes cannot be represented byte-aligned (a JS keyword used as a name,
  `this` as an object shorthand) or are broken in Odoo itself (interpolated `t-ref`) — see the
  corpus test in §2. Their expressions lose all features.
