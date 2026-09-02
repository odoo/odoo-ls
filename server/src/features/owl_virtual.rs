//! OWL-template virtual documents.
//!
//! LSP features (hover, definition, completion, references, semantic tokens) for the
//! JavaScript expressions embedded in XML OWL templates. Each component gets a minimal
//! virtual `.js` — an `import` of the class plus one `/** @this {Class} */` function per
//! expression — opened in tsserver; requests are forwarded at mapped positions and results
//! mapped back onto the XML. Design, rationale and the references machinery are documented
//! in `server/docs/owl-virtual-docs.md`.
//!
//! Positions: tsserver speaks UTF-16 (converted at the boundary); everything internal is
//! bytes. The compiled expression is byte-aligned with the XML attribute value, so mapping
//! between the two is a constant offset per expression.

use std::cell::RefCell;
use std::rc::Rc;

use lsp_types::{
    CompletionList, CompletionTriggerKind, GotoDefinitionResponse, Hover, HoverContents, Location, MarkupContent,
    MarkupKind, Position, Range, SemanticTokens,
};
use ruff_source_file::{LineIndex, PositionEncoding};

use crate::core::file_mgr::{FileInfo, FileMgr, offset_to_position_with_line_index, position_to_offset_with_line_index};
use crate::core::js_arch_builder::JsExportKind;
use crate::features::owl_component_utils::{self, template_reference_resolves};
use crate::core::tsserver_bridge::{ts_to_lsp_location, TsLocation, TsServerBridge};
use crate::features::owl_expr::{compile_owl_expr, interp_chunk_ranges, this_token_at};
use crate::features::owl_xml_utils::{TEMPLATE_NAME_ATTRS, component_tag_name_range, is_owl_expression_attr, is_owl_interpolation_attr, is_prop_expr_attr, tag_is_component};
use crate::features::semantic_tokens::{SemanticTokensFeature, TokMod, TokType, U16ToByte};
use crate::threads::SessionInfo;
use crate::utils::HashMap;

/// A template-local in scope for an expression (`t-set` or `t-foreach`/`t-as`), emitted
/// into the function preamble as a `let` so tsserver types bare identifiers. `decl_offset`
/// is the XML byte of the declared name, for go-to-definition on a local.
#[derive(Clone)]
enum TemplateLocal {
    /// `t-set="name" t-value="EXPR"` → `let name = (EXPR);`.
    Set { name: String, value: String, decl_offset: usize },
    /// `t-set="name">body</t>` (body form, no `t-value`) → an `any` (the body is markup).
    SetBody { name: String, decl_offset: usize },
    /// `t-foreach="COLL" t-as="name"` → the loop bindings (`name`, `name_value`,
    /// `name_index`, `name_first`, `name_last`).
    Foreach { name: String, collection: String, decl_offset: usize },
}

impl TemplateLocal {
    /// The primary identifier the directive binds (the `t-as` / `t-set` name).
    fn name(&self) -> &str {
        match self {
            TemplateLocal::Set { name, .. }
            | TemplateLocal::SetBody { name, .. }
            | TemplateLocal::Foreach { name, .. } => name,
        }
    }

    /// XML byte offset of the declared name (inside the `t-set` / `t-as` attribute value).
    fn decl_offset(&self) -> usize {
        match self {
            TemplateLocal::Set { decl_offset, .. }
            | TemplateLocal::SetBody { decl_offset, .. }
            | TemplateLocal::Foreach { decl_offset, .. } => *decl_offset,
        }
    }
}

/// A preamble `let <ident>` declaration, relative to the start of a preamble string. Turned
/// into an absolute `LocalDecl` once the block's virtual position is known.
struct PreDecl {
    /// Byte offset of the identifier within the preamble string.
    rel_offset: usize,
    /// Byte length of the identifier (may be a derived name, e.g. `rec_index`).
    v_name_len: usize,
    /// XML byte offset of the source declaration name (the `t-set` / `t-as` value).
    xml_offset: usize,
    /// Byte length of the source declaration name.
    xml_name_len: usize,
}

/// A preamble `let <name>` declaration located in the assembled virtual file, mapping the
/// identifier's virtual byte range back to the XML declaration that produced it.
struct LocalDecl {
    v_byte_start: usize,
    v_byte_end: usize,
    xml_byte_start: usize,
    xml_name_len: usize,
}

/// An OWL directive expression collected from an XML template.
struct CollectedExpr {
    template_name: String,
    /// Raw source text of the expression — a verbatim slice of the XML (entities not
    /// decoded), so its byte offsets line up 1:1 with the XML file.
    text: String,
    /// Byte offset of the expression's first char in the XML source.
    xml_byte_start: usize,
    /// Template-locals in scope for this expression, in declaration order (outermost first).
    locals: Vec<TemplateLocal>,
    /// `true` when `text` is a *static component tag name* (`<ChangeLine/>`), spliced through
    /// a `const Tag = Class.components["Tag"];` local. Carries no preamble (a tag resolves
    /// against the class, never shadowed by a template-local).
    is_component_tag: bool,
}

/// A spliced expression placed in the virtual `.js`, carrying the data needed to map a
/// position between the XML source and the virtual file in both directions.
struct SplicedExpr {
    /// Byte offset of the compiled expression's start inside the virtual file.
    v_byte_start: usize,
    /// The compiled expression text. Byte-aligned with the XML raw text (word-op
    /// replacement is length-preserving), so byte offsets within it are valid XML offsets.
    compiled: String,
    /// Byte offset of the expression's first char in the XML source.
    xml_byte_start: usize,
    /// `true` for a static component-tag expression. `compiled` is the tag name verbatim (the
    /// `const Tag` the splice declares), byte-aligned with the XML like any other expression;
    /// only go-to-definition differs, asking tsserver for the *type* of that local.
    is_tag: bool,
}

impl SplicedExpr {
    /// Byte offset just past the compiled expression in the virtual file.
    fn v_byte_end(&self) -> usize {
        self.v_byte_start + self.compiled.len()
    }

    /// Whether an XML byte cursor falls on this expression. End-inclusive so a cursor right
    /// after the last typed character (the common completion case) still counts; distinct
    /// expressions are never byte-adjacent, so two can't both match.
    fn contains_cursor(&self, xml_byte: usize) -> bool {
        self.xml_byte_start <= xml_byte && xml_byte <= self.xml_byte_start + self.compiled.len()
    }

    /// Whether a virtual byte falls inside the compiled expression text (as opposed to the
    /// surrounding synthetic function wrapper).
    fn contains_v_byte(&self, v_byte: usize) -> bool {
        self.v_byte_start <= v_byte && v_byte < self.v_byte_end()
    }
}

/// A shim for a non-exported component's file: the real source **verbatim** plus a trailing
/// `export { … };`, so a doc can `import { Class }` from it. Its prefix is byte-identical
/// to the real file, so a hit in it identity-maps back.
#[derive(Clone)]
pub(crate) struct ShimDoc {
    /// Path of the shim (`<stem>.__ols_shim__.js`), sibling of the real file.
    pub(crate) path: String,
    /// The shim content (`real` + `\nexport { … };`).
    content: String,
}

/// A built virtual document for one OWL **component**: import line + one `@this` function
/// per template expression, plus the data to map results back. Owned (independent of
/// `session`), so it can be held across `&mut session` bridge calls.
pub(crate) struct OwlVirtualDoc {
    /// Path of the virtual `.js` (next to the real file, stem `<file>.<Class>`).
    pub(crate) virtual_path: String,
    /// The virtual content sent to tsserver (import line + synthetic functions).
    content: String,
    /// Path of the real component `.js`.
    pub(crate) real_path: String,
    /// The per-file shim when the component is non-exported (the doc then imports the class
    /// from it), else `None`. `Some` also flags that a references query needs Query B — the
    /// doc's `this.member` is then a symbol distinct from the real member.
    pub(crate) shim: Option<ShimDoc>,
    /// Verbatim real component source — used to place real-file targets (the `this`
    /// definition, shim-hit identity mapping); *not* part of `content`.
    real_content: String,
    /// The spliced expressions, in ascending virtual order.
    exprs: Vec<SplicedExpr>,
    /// Preamble `let <name>` declarations, mapping a template-local's virtual identifier
    /// range back to its `t-set` / `t-as` declaration in the XML (for go-to-definition).
    local_decls: Vec<LocalDecl>,
    /// Line index of `content`, for the UTF-16 position conversions at the tsserver boundary.
    index: LineIndex,
    /// Line index of `real_content`, for encoding real-`.js` result positions.
    real_index: LineIndex,
}

impl OwlVirtualDoc {
    /// Convert a tsserver `(line, character)` — always UTF-16 — to a byte offset in the
    /// virtual content.
    fn ts_pos_to_v_byte(&self, line: u32, character: u32) -> usize {
        position_to_offset_with_line_index(&self.index, &self.content, line, character, PositionEncoding::Utf16)
    }

    /// Convert a virtual byte offset to the `(line, character)` — UTF-16 — tsserver expects.
    fn v_byte_to_ts_pos(&self, byte: usize) -> (u32, u32) {
        let pos = offset_to_position_with_line_index(&self.index, &self.content, byte, PositionEncoding::Utf16);
        (pos.line, pos.character)
    }

    /// Convert a byte offset in the real `.js` to an LSP position in `encoding`.
    fn real_byte_to_position(&self, byte: usize, encoding: PositionEncoding) -> Position {
        offset_to_position_with_line_index(&self.real_index, &self.real_content, byte, encoding)
    }

    /// The spliced expression an XML byte cursor sits on, if any.
    fn expr_at_cursor(&self, xml_byte: usize) -> Option<&SplicedExpr> {
        self.exprs.iter().find(|e| e.contains_cursor(xml_byte))
    }

    /// The `(line, character)` — 0-based, UTF-16 — to query tsserver at for an XML byte cursor
    /// sitting on one of our expressions.
    pub(crate) fn cursor_ts_pos(&self, xml_byte: usize) -> Option<(u32, u32)> {
        let expr = self.expr_at_cursor(xml_byte)?;
        Some(self.v_byte_to_ts_pos(expr.v_byte_start + (xml_byte - expr.xml_byte_start)))
    }
}

/// Filename suffix of an OWL virtual **doc** (`<stem>.<Class>.__ols_owl__.js`).
const VIRTUAL_JS_SUFFIX: &str = ".__ols_owl__.js";

/// Filename suffix of an OWL **shim** (`<stem>.__ols_shim__.js`).
const SHIM_JS_SUFFIX: &str = ".__ols_shim__.js";

/// Whether a path is an OWL virtual doc (the minimal splice).
pub(crate) fn is_owl_doc_path(path: &str) -> bool {
    path.ends_with(VIRTUAL_JS_SUFFIX)
}

/// Whether a path is an OWL shim (a real-file copy that re-exports non-exported components).
pub(crate) fn is_owl_shim_path(path: &str) -> bool {
    path.ends_with(SHIM_JS_SUFFIX)
}

/// Whether a path is any of our OWL virtual artefacts (doc or shim). Such paths are
/// internal and must never surface to the client verbatim: remapped or dropped.
pub(crate) fn is_owl_artifact_path(path: &str) -> bool {
    is_owl_doc_path(path) || is_owl_shim_path(path)
}

/// The real `.js` a shim was copied from (`<stem>.__ols_shim__.js` → `<stem>.js`).
pub(crate) fn shim_to_real(shim_path: &str) -> String {
    let stem = shim_path.strip_suffix(SHIM_JS_SUFFIX).unwrap_or(shim_path);
    format!("{stem}.js")
}

/// Semantic tokens for an XML OWL-template file: template-name tokens (in-house, works
/// without tsserver) merged with JS-expression tokens (delegated to tsserver via the
/// virtual docs). `None` when neither source produced a token.
pub fn semantic_tokens_xml(session: &mut SessionInfo, file_info: &Rc<RefCell<FileInfo>>) -> Option<SemanticTokens> {
    let encoding = session.sync_odoo.encoding;
    let mut raw_tokens: Vec<(Range, u32, u32)> = vec![];

    // 1. Template-name tokens. Independent of tsserver: we only need to parse the XML.
    let xml = {
        let fi = file_info.borrow();
        let fia = fi.file_info_ast.borrow();
        fia.text_document.as_ref().map(|td| td.contents().to_string())
    };
    if let Some(xml) = xml.as_deref() && let Ok(document) = roxmltree::Document::parse(xml)
    {
        let mut names = vec![];
        collect_template_names(document.root_element(), &mut names);
        for (range, is_declaration) in names {
            // References are highlighted only when they resolve — exactly when Definition
            // would navigate from them; a broken reference stays grammar-coloured.
            if !is_declaration && !template_reference_resolves(session, &xml[range.clone()]) {
                continue;
            }
            let lsp_range = file_info.borrow().std_range_to_range(&range, encoding);
            let modifiers = if is_declaration { TokMod::Declaration.bit() } else { 0 };
            raw_tokens.push((lsp_range, TokType::Type as u32, modifiers));
        }
    }

    // 2. OWL-template JS expression tokens, delegated to tsserver. Without a bridge we keep
    //    only the template-name tokens collected above.
    if session.sync_odoo.tsserver_bridge.is_some() {
        let docs = build_virtual_docs(session, file_info);
        // Stage every doc (an XML file may back several components), one project rebuild.
        for doc in &docs {
            let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() else { break };
            stage_doc_and_shim(bridge, doc);
        }
        commit_staged_roots(session);
        for doc in &docs {
            let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() else { break };
            let spans = bridge.get_semantic_tokens(&doc.virtual_path);
            remap_spans_to_xml(doc, spans, file_info, encoding, &mut raw_tokens);
        }
    }

    if raw_tokens.is_empty() {
        return None;
    }
    Some(SemanticTokensFeature::encode(raw_tokens))
}

/// Collect every template-name attribute value as `(xml_byte_range, is_declaration)` —
/// `is_declaration` is true for `t-name`, false for the `t-call` / `t-inherit` references.
fn collect_template_names(node: roxmltree::Node, out: &mut Vec<(std::ops::Range<usize>, bool)>) {
    if node.is_element() {
        for attr in node.attributes() {
            if !TEMPLATE_NAME_ATTRS.contains(&attr.name()) {
                continue;
            }
            // `range_value()` excludes the surrounding quotes, so it maps 1:1 onto the name.
            let r = attr.range_value();
            if r.start < r.end {
                out.push((r.start..r.end, attr.name() == "t-name"));
            }
        }
    }
    for child in node.children() {
        collect_template_names(child, out);
    }
}

/// Locate the XML cursor on a spliced expression: the containing virtual doc plus the
/// cursor's XML byte offset. Sends nothing to tsserver. `None` when there is no bridge or
/// the cursor is on no expression — the caller falls back.
pub(crate) fn locate_doc_at_cursor(
    session: &mut SessionInfo,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
) -> Option<(OwlVirtualDoc, usize)> {
    session.sync_odoo.tsserver_bridge.as_ref()?;
    let encoding = session.sync_odoo.encoding;
    let xml_byte = file_info.borrow().position_to_offset(line, character, encoding);

    let mut docs = build_virtual_docs(session, file_info);
    let idx = docs.iter().position(|doc| doc.expr_at_cursor(xml_byte).is_some())?;
    Some((docs.swap_remove(idx), xml_byte))
}

/// Stage a doc and, if shim-backed, its shim (so the doc's `import` resolves), without
/// committing — callers commit once per batch. Staging is idempotent (deduped in the bridge).
pub(crate) fn stage_doc_and_shim(bridge: &mut TsServerBridge, doc: &OwlVirtualDoc) {
    if let Some(shim) = &doc.shim {
        bridge.stage_virtual_doc(&shim.path, &shim.content);
    }
    bridge.stage_virtual_doc(&doc.virtual_path, &doc.content);
}

/// Commit everything staged since the last commit — one `openExternalProject` rebuild.
pub(crate) fn commit_staged_roots(session: &mut SessionInfo) {
    if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
        bridge.commit_transient_roots();
    }
}

/// Open a doc (and its shim, if any) for a single-shot cursor feature, in one project
/// rebuild. Returns `None` when there is no bridge.
fn open_doc_with_shim(session: &mut SessionInfo, doc: &OwlVirtualDoc) -> Option<()> {
    let bridge = session.sync_odoo.tsserver_bridge.as_mut()?;
    stage_doc_and_shim(bridge, doc);
    bridge.commit_transient_roots();
    Some(())
}

/// Shared prologue of the cursor-anchored features: locate the cursor and open the
/// containing doc, ready to query at the returned `(line, character)` (0-based, UTF-16).
fn open_doc_at_cursor(
    session: &mut SessionInfo,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
) -> Option<(OwlVirtualDoc, u32, u32)> {
    let (doc, xml_byte) = locate_doc_at_cursor(session, file_info, line, character)?;
    let (v_line, v_char) = doc.cursor_ts_pos(xml_byte)?;

    open_doc_with_shim(session, &doc)?;
    Some((doc, v_line, v_char))
}

/// Hover for an OWL-template JS expression, delegated to tsserver's `quickinfo`.
/// `None` (caller falls back) when there is no tsserver, the cursor is not on an
/// expression, or tsserver has nothing to say.
pub fn hover_xml_owl(
    session: &mut SessionInfo,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
) -> Option<Hover> {
    let (doc, v_line, v_char) = open_doc_at_cursor(session, file_info, line, character)?;
    let text = session
        .sync_odoo
        .tsserver_bridge
        .as_mut()?
        .get_hover(&doc.virtual_path, v_line, v_char)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: text,
        }),
        range: None,
    })
}

/// Completion for an OWL-template JS expression, delegated to tsserver's `completions`.
/// The doc is built from the current (possibly mid-edit) XML, so tsserver sees the partial
/// expression. `None` when there is no tsserver, no expression at the cursor, or no items.
pub fn completion_xml_owl(
    session: &mut SessionInfo,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
    trigger_kind: CompletionTriggerKind,
) -> Option<CompletionList> {
    let (doc, v_line, v_char) = open_doc_at_cursor(session, file_info, line, character)?;
    let mut list = session
        .sync_odoo
        .tsserver_bridge
        .as_mut()?
        // No auto-imports: a template expression cannot carry an import, and the edit that
        // would write one addresses the virtual doc, so every such entry is unusable here.
        .completion_list_for_content(&doc.virtual_path, v_line, v_char, trigger_kind, false);

    // Drop edit-bearing entries (their positions address the virtual `.js` the client never
    // saw). Resolve data survives: it points at the still-open virtual doc, and
    // `handle_completion_resolve` keeps only the signature/docs for such paths.
    list.items.retain(|item| item.text_edit.is_none());

    if list.items.is_empty() {
        return None;
    }
    Some(list)
}

/// Go-to-definition for an OWL-template JS expression, delegated to tsserver and triaged by
/// [`map_virtual_ref`]. `None` when there is no tsserver, no expression at the cursor, or no
/// target.
pub fn definition_xml_owl(
    session: &mut SessionInfo,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
) -> Option<GotoDefinitionResponse> {
    let encoding = session.sync_odoo.encoding;
    let (doc, xml_byte) = locate_doc_at_cursor(session, file_info, line, character)?;

    // Two cursors want the *type* of the expression, which in both cases is the component
    // class: a bare `this` (the `@this` tag types it but declares nothing, so `definition`
    // has no answer at all) and a component tag (whose `definition` is the synthetic
    // `const Tag` local the splice declares, not the class it holds).
    let expr = doc.expr_at_cursor(xml_byte)?;
    let wants_type = expr.is_tag || this_token_at(&expr.compiled, xml_byte - expr.xml_byte_start);

    let (v_line, v_char) = doc.cursor_ts_pos(xml_byte)?;
    open_doc_with_shim(session, &doc)?;
    let bridge = session.sync_odoo.tsserver_bridge.as_mut()?;
    let raw = if wants_type {
        bridge.get_type_definition(&doc.virtual_path, v_line, v_char)
    } else {
        bridge.get_definition(&doc.virtual_path, v_line, v_char)
    };

    let locations: Vec<Location> = raw
        .iter()
        .filter_map(|hit| {
            match map_virtual_ref(&doc, file_info, encoding, hit)? {
                MappedRef::Xml(loc) | MappedRef::RealJs(loc) => Some(loc),
            }
        })
        .collect();

    if locations.is_empty() {
        return None;
    }
    Some(GotoDefinitionResponse::Array(locations))
}

/// A tsserver result mapped to where it really lives: the host XML (spliced expression or
/// template-local declaration) or real component code (cross-module target, shim hit).
pub(crate) enum MappedRef {
    Xml(Location),
    RealJs(Location),
}

/// The one triage shared by definition and references: map a tsserver result from a query
/// against `doc` to its real location, or `None` if it is virtual-only noise. This doc's
/// shim identity-maps to the real file; other real files pass through; other virtual
/// artefacts are dropped (leak guard); a spliced expression maps onto the XML; a preamble
/// `let` maps to its `t-set` / `t-as` declaration; anything else in the doc is dropped.
pub(crate) fn map_virtual_ref(
    doc: &OwlVirtualDoc,
    xml_fi: &Rc<RefCell<FileInfo>>,
    encoding: PositionEncoding,
    loc: &TsLocation,
) -> Option<MappedRef> {
    let (file, sl, sc, el, ec) = (loc.0.as_str(), loc.1, loc.2, loc.3, loc.4);
    if file != doc.virtual_path {
        // Shim = real file verbatim + `export` suffix: a byte in the prefix maps to the
        // real file by identity; a hit in the suffix is a re-export, dropped.
        if let Some(shim) = &doc.shim && file == shim.path {
            let start = position_to_offset_with_line_index(&doc.real_index, &doc.real_content, sl, sc, PositionEncoding::Utf16);
            if start >= doc.real_content.len() {
                return None;
            }
            let end = position_to_offset_with_line_index(&doc.real_index, &doc.real_content, el, ec, PositionEncoding::Utf16);
            let range = Range {
                start: doc.real_byte_to_position(start, encoding),
                end: doc.real_byte_to_position(end.min(doc.real_content.len()), encoding),
            };
            return Some(MappedRef::RealJs(Location { uri: FileMgr::pathname2uri(&doc.real_path), range }));
        }
        if is_owl_artifact_path(file) {
            return None;
        }
        return Some(MappedRef::RealJs(ts_to_lsp_location(loc)));
    }
    let v_start = doc.ts_pos_to_v_byte(sl, sc);
    let v_end = doc.ts_pos_to_v_byte(el, ec);
    if let Some(expr) = doc.exprs.iter().find(|e| e.contains_v_byte(v_start)) {
        // Inside a template expression ⇒ map back onto the XML (byte-aligned with `compiled`).
        let xml_start = expr.xml_byte_start + (v_start - expr.v_byte_start);
        let xml_end = xml_start + v_end.saturating_sub(v_start);
        let range = xml_fi.borrow().std_range_to_range(&(xml_start..xml_end), encoding);
        return Some(MappedRef::Xml(Location {
            uri: FileMgr::pathname2uri(&xml_fi.borrow().uri),
            range,
        }));
    }
    if let Some(decl) = doc
        .local_decls
        .iter()
        .find(|d| d.v_byte_start <= v_start && v_start < d.v_byte_end)
    {
        // A preamble `let <name>` ⇒ a template-local; report its `t-set` / `t-as` in the XML.
        let xml_range = decl.xml_byte_start..decl.xml_byte_start + decl.xml_name_len;
        let range = xml_fi.borrow().std_range_to_range(&xml_range, encoding);
        return Some(MappedRef::Xml(Location {
            uri: FileMgr::pathname2uri(&xml_fi.borrow().uri),
            range,
        }));
    }
    None
}

/// Build one minimal virtual document per **component** whose template lives in this XML
/// file: expressions collected and grouped by declaring class, then import line + one
/// function each. No tsserver interaction — the result is fully owned. A non-exported
/// component's doc imports its class from a per-file shim instead.
pub(crate) fn build_virtual_docs(session: &mut SessionInfo, file_info: &Rc<RefCell<FileInfo>>) -> Vec<OwlVirtualDoc> {
    // 1. XML content.
    let xml = {
        let fi = file_info.borrow();
        let fia = fi.file_info_ast.borrow();
        match fia.text_document.as_ref() {
            Some(td) => td.contents().to_string(),
            None => return vec![],
        }
    };

    // 2. Collect every directive expression, tagged with its enclosing template.
    let Ok(document) = roxmltree::Document::parse(&xml) else {
        return vec![];
    };
    let mut collected = vec![];
    collect_exprs(document.root_element(), &xml, None, &[], &mut collected);
    if collected.is_empty() {
        return vec![];
    }

    // 3. Group expressions by declaring component class — one doc per component.
    let mut by_class: HashMap<String, Vec<CollectedExpr>> = HashMap::default();
    for expr in collected {
        let Some(class_name) = owl_component_utils::component_for_template(session, &expr.template_name) else {
            continue;
        };
        by_class.entry(class_name).or_default().push(expr);
    }

    // 4. Per component: resolve its descriptor, form the import line, and splice the functions.
    let mut docs = vec![];
    for (class_name, exprs) in by_class {
        let Some((file_path, export_kind)) = session
            .sync_odoo
            .component_descriptors
            .get(&class_name)
            .map(|d| (d.file_path.clone(), d.export_kind))
        else {
            continue;
        };
        let Some(real) = read_real_js(session, &file_path) else {
            continue;
        };
        let (import_line, shim) = match export_kind {
            JsExportKind::Named => {
                (format!("import {{ {class_name} }} from \"{}\";", module_specifier(&file_path)), None)
            }
            JsExportKind::Default => {
                (format!("import {class_name} from \"{}\";", module_specifier(&file_path)), None)
            }
            // Non-exported: import the class through a per-file shim that re-exports it.
            JsExportKind::None => {
                let Some(shim) = build_shim_for_file(session, &real, &file_path) else {
                    continue;
                };
                (format!("import {{ {class_name} }} from \"{}\";", shim_module_specifier(&file_path)), Some(shim))
            }
        };
        let (content, exprs, local_decls) = build_virtual(&import_line, &class_name, exprs);
        if exprs.is_empty() {
            continue;
        }
        let index = LineIndex::from_source_text(&content);
        let real_index = LineIndex::from_source_text(&real);
        docs.push(OwlVirtualDoc {
            virtual_path: virtual_js_path(&file_path, &class_name),
            content,
            real_path: file_path,
            shim,
            real_content: real,
            exprs,
            local_decls,
            index,
            real_index,
        });
    }
    docs
}

/// Build the shim for `file_path` (`real` verbatim + `export { A, B };` naming every
/// non-exported component), or `None` if the file has none. The leading `\n` guards a file
/// ending in a `//` comment; an unterminated `/*` cannot occur in a parseable file.
fn build_shim_for_file(session: &SessionInfo, real: &str, file_path: &str) -> Option<ShimDoc> {
    let mut names: Vec<String> = session
        .sync_odoo
        .component_descriptors
        .values()
        .filter(|d| d.file_path == file_path && d.export_kind == JsExportKind::None)
        .map(|d| d.class_name.clone())
        .collect();
    if names.is_empty() {
        return None;
    }
    names.sort();
    names.dedup();
    Some(ShimDoc {
        path: shim_js_path(file_path),
        content: format!("{real}\nexport {{ {} }};\n", names.join(", ")),
    })
}

/// Recursively collect directive expressions, tracking the nearest enclosing `t-name` and
/// the template-locals in scope. Scope rules mirror OWL's: a `t-foreach`/`t-as` var covers
/// the element's other expr attrs and descendants (not the collection expr itself); a
/// `t-set` var covers the *following* siblings and their descendants.
fn collect_exprs<'a>(
    node: roxmltree::Node<'a, 'a>,
    xml: &str,
    parent_template: Option<&'a str>,
    scope: &[TemplateLocal],
    out: &mut Vec<CollectedExpr>,
) {
    if !node.is_element() {
        return;
    }
    let current_template = node.attribute("t-name").or(parent_template);

    // Loop var introduced by this element (in scope for its non-`t-foreach` attrs + children).
    let mut inner_scope = scope.to_vec();
    if let Some(local) = extract_foreach_local(node) {
        inner_scope.push(local);
    }

    if let Some(template_name) = current_template {
        let is_component = tag_is_component(node);

        // A static component tag becomes its own expression (empty locals: it resolves in
        // module scope, never shadowed by a template-local).
        if let Some((offset, len)) = component_tag_name_range(node) {
            out.push(CollectedExpr {
                template_name: template_name.to_string(),
                text: xml[offset..offset + len].to_string(),
                xml_byte_start: offset,
                locals: vec![],
                is_component_tag: true,
            });
        }

        for attr in node.attributes() {
            let name = attr.name();
            let range = attr.range_value();
            if range.end <= range.start {
                continue;
            }

            // An interpolation attribute yields one expression per `{{…}}` / `#{…}` chunk,
            // each seeing this element's scope.
            if is_owl_interpolation_attr(name) {
                let value = &xml[range.start..range.end];
                for (inner_start, inner_end) in interp_chunk_ranges(value) {
                    let text = xml[range.start + inner_start..range.start + inner_end].to_string();
                    if text.trim().is_empty() {
                        continue;
                    }
                    out.push(CollectedExpr {
                        template_name: template_name.to_string(),
                        text,
                        xml_byte_start: range.start + inner_start,
                        locals: inner_scope.clone(),
                        is_component_tag: false,
                    });
                }
                continue;
            }

            if !(is_owl_expression_attr(name) || (is_component && is_prop_expr_attr(name))) {
                continue;
            }
            let text = xml[range.start..range.end].to_string();
            if text.trim().is_empty() {
                continue;
            }
            // Only the `t-foreach` collection is evaluated in the outer scope.
            let expr_scope = if name == "t-foreach" { scope } else { &inner_scope };
            out.push(CollectedExpr {
                template_name: template_name.to_string(),
                text,
                xml_byte_start: range.start,
                locals: expr_scope.to_vec(),
                is_component_tag: false,
            });
        }
    }

    // Children see `inner_scope`, extended by each preceding sibling's `t-set` var.
    let mut sibling_scope = inner_scope;
    for child in node.children() {
        collect_exprs(child, xml, current_template, &sibling_scope, out);
        if child.is_element() && let Some(local) = extract_tset_local(child) 
        {
            sibling_scope.push(local);
        }
    }
}

/// Extract the loop binding declared by `t-foreach="COLL" t-as="name"` on an element.
fn extract_foreach_local(node: roxmltree::Node) -> Option<TemplateLocal> {
    let name = node.attribute("t-as")?;
    let collection = node.attribute("t-foreach")?;
    if name.trim().is_empty() || collection.trim().is_empty() {
        return None;
    }
    let decl_offset = node.attributes().find(|a| a.name() == "t-as")?.range_value().start;
    Some(TemplateLocal::Foreach { name: name.to_string(), collection: collection.to_string(), decl_offset })
}

/// Extract the local declared by `t-set="name"` on an element (value form or body form).
fn extract_tset_local(node: roxmltree::Node) -> Option<TemplateLocal> {
    let name = node.attribute("t-set")?;
    if name.trim().is_empty() {
        return None;
    }
    let decl_offset = node.attributes().find(|a| a.name() == "t-set")?.range_value().start;
    match node.attribute("t-value") {
        Some(value) => Some(TemplateLocal::Set { name: name.to_string(), value: value.to_string(), decl_offset }),
        None => Some(TemplateLocal::SetBody { name: name.to_string(), decl_offset }),
    }
}

/// Build the `let`-declaration preamble for the locals in scope of a spliced expression, plus
/// the position of each declared identifier (for mapping go-to-definition back to the XML).
/// De-duplicates by name so a shadowing inner declaration replaces the outer one (avoids a
/// `let` redeclaration error); the innermost declaration wins.
fn emit_preamble(locals: &[TemplateLocal]) -> (String, Vec<PreDecl>) {
    let mut out = String::new();
    let mut decls = vec![];
    for local in dedup_locals(locals) {
        let xml_offset = local.decl_offset();
        let xml_name_len = local.name().len();
        // Record the `<ident>` that immediately follows the just-pushed `let `.
        let mut push_let = |out: &mut String, ident: &str, rhs: &str| {
            decls.push(PreDecl {
                rel_offset: out.len() + "let ".len(),
                v_name_len: ident.len(),
                xml_offset,
                xml_name_len,
            });
            out.push_str(&format!("let {ident} = {rhs}; "));
        };
        match local {
            TemplateLocal::Set { name, value, .. } => {
                push_let(&mut out, name, &format!("({})", compile_owl_expr(value)));
            }
            TemplateLocal::SetBody { name, .. } => {
                // Body form renders markup; expose it as `any` so member access resolves.
                push_let(&mut out, name, "/** @type {any} */ (undefined)");
            }
            TemplateLocal::Foreach { name, collection, .. } => {
                // `(COLL)[0]` is the element type for the dominant array/iterable case;
                // object/Map *keys* are knowingly mistyped as the element (hover-only impact).
                // Every derived name maps back to the same `t-as` declaration.
                let c = format!("({})[0]", compile_owl_expr(collection));
                push_let(&mut out, name, &c);
                push_let(&mut out, &format!("{name}_value"), &c);
                push_let(&mut out, &format!("{name}_index"), "0");
                push_let(&mut out, &format!("{name}_first"), "false");
                push_let(&mut out, &format!("{name}_last"), "false");
            }
        }
    }
    (out, decls)
}

/// Keep only the last (innermost) declaration of each name, preserving order. Cheap O(n²)
/// over the handful of locals a template node ever has in scope.
fn dedup_locals(locals: &[TemplateLocal]) -> Vec<&TemplateLocal> {
    let mut out: Vec<&TemplateLocal> = vec![];
    for local in locals {
        out.retain(|l| l.name() != local.name());
        out.push(local);
    }
    out
}

/// Assemble the virtual `.js` content — `import_line`, then one `@this` function per
/// expression — returning the spliced expressions and the preamble `let` declarations
/// needed to map results back. All expressions belong to one component (`class_name`).
fn build_virtual(import_line: &str, class_name: &str, exprs: Vec<CollectedExpr>) -> (String, Vec<SplicedExpr>, Vec<LocalDecl>) {
    let mut content = String::with_capacity(import_line.len() + 1 + exprs.len() * 96);
    content.push_str(import_line);
    content.push('\n');
    let mut spliced: Vec<SplicedExpr> = Vec::with_capacity(exprs.len());
    let mut local_decls: Vec<LocalDecl> = vec![];

    for (idx, expr) in exprs.into_iter().enumerate() {
        content.push_str(&format!("\n/** @this {{{class_name}}} */ function __ols_m{idx}() {{ "));
        let (preamble, pre_decls) = emit_preamble(&expr.locals);
        for pd in pre_decls {
            let v_byte_start = content.len() + pd.rel_offset;
            local_decls.push(LocalDecl {
                v_byte_start,
                v_byte_end: v_byte_start + pd.v_name_len,
                xml_byte_start: pd.xml_offset,
                xml_name_len: pd.xml_name_len,
            });
        }
        content.push_str(&preamble);
        let compiled = if expr.is_component_tag {
            // `const Tag = Class.components["Tag"];`, then `return (Tag);` — the local holds the
            // component class, so tsserver types and classifies the anchored `Tag` as that class
            // rather than as a key of `components`. `Class.components["Tag"]` is how Owl itself
            // resolves a tag, so an aliased or spread entry resolves too.
            content.push_str(&format!("const {0} = {class_name}.components[\"{0}\"]; ", expr.text));
            expr.text.clone()
        } else {
            compile_owl_expr(&expr.text)
        };
        content.push_str("return (");
        let v_byte_start = content.len();
        content.push_str(&compiled);
        content.push_str("); }");
        spliced.push(SplicedExpr {
            v_byte_start,
            compiled,
            xml_byte_start: expr.xml_byte_start,
            is_tag: expr.is_component_tag,
        });
    }

    (content, spliced, local_decls)
}

/// Map tsserver's semantic-token spans (flat ascending UTF-16 offsets into the virtual
/// file) back onto the XML; spans outside every spliced expression are dropped.
fn remap_spans_to_xml(
    doc: &OwlVirtualDoc,
    spans: Vec<(u32, u32, u32, u32)>,
    file_info: &Rc<RefCell<FileInfo>>,
    encoding: PositionEncoding,
    out: &mut Vec<(Range, u32, u32)>,
) {
    // One monotone forward pass converts the ascending span offsets to bytes.
    let mut conv = U16ToByte::new(&doc.content);
    for (u16_start, u16_len, token_type, modifiers) in spans {
        let b_start = conv.advance_to(u16_start);
        let b_end = conv.advance_to(u16_start + u16_len);
        let Some(expr) = doc
            .exprs
            .iter()
            .find(|e| e.contains_v_byte(b_start) && b_end <= e.v_byte_end())
        else {
            continue;
        };
        let xml_start = expr.xml_byte_start + (b_start - expr.v_byte_start);
        let xml_end = xml_start + (b_end - b_start);
        let range = file_info.borrow().std_range_to_range(&(xml_start..xml_end), encoding);
        out.push((range, token_type, modifiers));
    }
}

/// Path of the virtual `.js` for one component: next to the real file (so module resolution
/// matches) and keyed on the class name (so components sharing a file get distinct docs).
fn virtual_js_path(real_js_path: &str, class_name: &str) -> String {
    let stem = real_js_path.strip_suffix(".js").unwrap_or(real_js_path);
    format!("{stem}.{class_name}{VIRTUAL_JS_SUFFIX}")
}

/// The relative specifier a sibling virtual doc uses to import the real component module
/// (`./<basename-without-.js>`).
fn module_specifier(real_js_path: &str) -> String {
    let stem = real_js_path.strip_suffix(".js").unwrap_or(real_js_path);
    let base = stem.rsplit(['/', '\\']).next().unwrap_or(stem);
    format!("./{base}")
}

/// Path of the shim for a real component file (`<stem>.__ols_shim__.js`), a sibling of it.
fn shim_js_path(real_js_path: &str) -> String {
    let stem = real_js_path.strip_suffix(".js").unwrap_or(real_js_path);
    format!("{stem}{SHIM_JS_SUFFIX}")
}

/// The relative specifier a doc uses to import from the shim (`./<basename>.__ols_shim__`).
fn shim_module_specifier(real_js_path: &str) -> String {
    format!("{}.__ols_shim__", module_specifier(real_js_path))
}

/// Read the real component `.js`, preferring the in-memory buffer (the content the
/// descriptors — and thus `class_name_byte` — were computed from) over disk.
pub(crate) fn read_real_js(session: &mut SessionInfo, file_path: &str) -> Option<String> {
    let file_info = session.sync_odoo.get_file_mgr().borrow().get_file_info(file_path);
    if let Some(file_info) = file_info {
        let fi = file_info.borrow();
        let fia = fi.file_info_ast.borrow();
        if let Some(td) = fia.text_document.as_ref() {
            return Some(td.contents().to_string());
        }
    }
    std::fs::read_to_string(file_path).ok()
}

/// Convert a byte range in `content` — a file that may not be open in the `FileMgr` (so no
/// `FileInfo` conversions are available) — to an LSP `Range` in `encoding`.
/// TODO: we can maybe use FileMgr::std_range_to_range at this function's the call site and drop this.
pub(crate) fn byte_range_to_lsp_range(
    content: &str,
    range: std::ops::Range<usize>,
    encoding: PositionEncoding,
) -> Range {
    let index = LineIndex::from_source_text(content);
    Range {
        start: offset_to_position_with_line_index(&index, content, range.start, encoding),
        end: offset_to_position_with_line_index(&index, content, range.end, encoding),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u16_to_byte_walks_ascending_offsets_across_supplementary_chars() {
        // "a😀b": 😀 is 4 bytes / 2 UTF-16 units. UTF-16 offsets: a@0, 😀@1, b@3.
        let mut conv = U16ToByte::new("a😀b");
        assert_eq!(conv.advance_to(0), 0);
        assert_eq!(conv.advance_to(1), 1); // start of the emoji
        assert_eq!(conv.advance_to(3), 5); // past the emoji → 'b'
        assert_eq!(conv.advance_to(4), 6); // end of content
        assert_eq!(conv.advance_to(99), 6); // past EOF clamps
    }

    /// A non-tag [`CollectedExpr`] with no locals — the common case for these unit tests.
    fn expr(template: &str, text: &str, xml_byte_start: usize) -> CollectedExpr {
        CollectedExpr {
            template_name: template.to_string(),
            text: text.to_string(),
            xml_byte_start,
            locals: vec![],
            is_component_tag: false,
        }
    }

    #[test]
    fn build_virtual_is_import_plus_this_typed_functions() {
        let import_line = "import { Foo } from \"./foo\";";
        let exprs = vec![expr("mod.Foo", "this.x", 100)];
        let (virtual_content, spliced, _decls) = build_virtual(import_line, "Foo", exprs);

        // The import line is the prefix; no copied real code, just the synthetic function(s).
        assert!(virtual_content.starts_with(import_line));
        assert!(!virtual_content.contains("class"));
        assert!(virtual_content.contains("/** @this {Foo} */ function __ols_m0() { return (this.x); }"));
        assert_eq!(spliced.len(), 1);
        assert_eq!(spliced[0].xml_byte_start, 100);
        // The recorded byte range points exactly at the compiled expression.
        assert!(spliced[0].v_byte_start >= import_line.len());
        assert_eq!(&virtual_content[spliced[0].v_byte_start..spliced[0].v_byte_end()], "this.x");
        assert!(!spliced[0].is_tag);
    }

    #[test]
    fn build_virtual_keeps_offsets_aligned_across_functions() {
        // One component, several expressions: each gets its own `@this`-typed function and its
        // recorded byte range lands on the right compiled text.
        let import_line = "import { A } from \"./a\";";
        let exprs = vec![expr("m.A", "this.a", 10), expr("m.A", "this.b", 20)];
        let (virtual_content, spliced, _decls) = build_virtual(import_line, "A", exprs);
        assert!(virtual_content.starts_with(import_line));
        for exp in &spliced {
            assert_eq!(&virtual_content[exp.v_byte_start..exp.v_byte_end()], exp.compiled);
        }
        assert!(virtual_content.contains("/** @this {A} */ function __ols_m0() { return (this.a); }"));
        assert!(virtual_content.contains("/** @this {A} */ function __ols_m1() { return (this.b); }"));
    }

    #[test]
    fn build_virtual_component_tag_splices_components_local_byte_aligned() {
        // A static component tag `<ChangeLine/>` goes through a local holding
        // `Class.components["ChangeLine"]`, and the anchor is the local's *use* — not its
        // declaration (which would carry a `declaration` semantic-token modifier).
        let import_line = "import { Parent } from \"./parent\";";
        let mut tag = expr("m.Parent", "ChangeLine", 40);
        tag.is_component_tag = true;
        let (content, spliced, _decls) = build_virtual(import_line, "Parent", vec![tag]);

        assert!(content.contains(
            r#"{ const ChangeLine = Parent.components["ChangeLine"]; return (ChangeLine); }"#
        ));
        assert_eq!(spliced.len(), 1);
        assert!(spliced[0].is_tag);
        // `compiled`/`v_byte_start` point at the returned tag name, so the XML byte-alignment
        // (used by cursor/hover/completion/tokens) holds.
        assert_eq!(spliced[0].compiled, "ChangeLine");
        assert_eq!(&content[spliced[0].v_byte_start..spliced[0].v_byte_end()], "ChangeLine");
        assert!(content[..spliced[0].v_byte_start].ends_with("return ("));
        assert_eq!(spliced[0].xml_byte_start, 40);
    }

    #[test]
    fn owl_artifact_predicates_split_doc_and_shim() {
        let doc = virtual_js_path("/a/b/wysiwyg.js", "Wysiwyg");
        let shim = shim_js_path("/a/b/wysiwyg.js");
        assert_eq!(doc, "/a/b/wysiwyg.Wysiwyg.__ols_owl__.js");
        assert_eq!(shim, "/a/b/wysiwyg.__ols_shim__.js");

        assert!(is_owl_doc_path(&doc) && !is_owl_shim_path(&doc));
        assert!(is_owl_shim_path(&shim) && !is_owl_doc_path(&shim));
        assert!(is_owl_artifact_path(&doc) && is_owl_artifact_path(&shim));

        // A real file and unrelated paths are neither.
        for p in ["/a/b/wysiwyg.js", "/a/b/component.xml", "/a/b/__ols_owl__.js.bak"] {
            assert!(!is_owl_artifact_path(p), "{p}");
        }
        // A shim round-trips to its real file; the doc's specifier imports from the shim.
        assert_eq!(shim_to_real(&shim), "/a/b/wysiwyg.js");
        assert_eq!(shim_module_specifier("/a/b/wysiwyg.js"), "./wysiwyg.__ols_shim__");
    }

    #[test]
    fn virtual_js_path_and_specifier_are_siblings_of_the_real_file() {
        // The doc is keyed on the class (two components in one file → two docs, no collision),
        // sits next to the real file, and imports it via a bare relative specifier.
        assert_eq!(
            virtual_js_path("/a/b/account_label_text.js", "AccountLabelTextField"),
            "/a/b/account_label_text.AccountLabelTextField.__ols_owl__.js"
        );
        assert_eq!(module_specifier("/a/b/account_label_text.js"), "./account_label_text");
        // The doc's stem differs from the real file's, so `./account_label_text` never resolves
        // back to the doc itself.
        assert_ne!(
            virtual_js_path("/a/b/account_label_text.js", "Foo"),
            "/a/b/account_label_text.js"
        );
    }

    #[test]
    fn utf16_pos_round_trips_with_byte() {
        let content = "let x = 1;\nlet y = 2;\n  return this.z;";
        let index = LineIndex::from_source_text(content);
        for byte in [0usize, 4, 10, 11, 22, 30, content.len()] {
            let pos = offset_to_position_with_line_index(&index, content, byte, PositionEncoding::Utf16);
            assert_eq!(
                position_to_offset_with_line_index(&index, content, pos.line, pos.character, PositionEncoding::Utf16),
                byte,
                "round-trip at byte {byte}"
            );
        }
        // Line/column of a known token.
        let z = content.find("this.z").unwrap();
        let pos = offset_to_position_with_line_index(&index, content, z, PositionEncoding::Utf16);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 9); // "  return " is 9 UTF-16 units
    }

    #[test]
    fn byte_range_to_lsp_range_respects_encoding() {
        let content = "a😀b\ncd";
        // Line 0, after the emoji (byte 5): UTF-16 col 3 (a=1 😀=2), UTF-8 col 5, UTF-32 col 2.
        let start = |enc| byte_range_to_lsp_range(content, 5..5, enc).start;
        assert_eq!(start(PositionEncoding::Utf16), Position { line: 0, character: 3 });
        assert_eq!(start(PositionEncoding::Utf8), Position { line: 0, character: 5 });
        assert_eq!(start(PositionEncoding::Utf32), Position { line: 0, character: 2 });
        // Second line resets the column.
        let d = content.find('d').unwrap();
        let range = byte_range_to_lsp_range(content, d..d + 1, PositionEncoding::Utf16);
        assert_eq!(range.start, Position { line: 1, character: 1 });
        assert_eq!(range.end, Position { line: 1, character: 2 });
    }

    #[test]
    fn collect_exprs_picks_expression_attrs_with_verbatim_offsets() {
        let xml = r#"<templates><t t-name="mod.A"><div t-if="this.ok" t-att-class="this.cls" t-custom-ref="root"><t t-set="x" t-value="1 + 2"/></div></t></templates>"#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let mut out = vec![];
        collect_exprs(document.root_element(), xml, None, &[], &mut out);

        let texts: Vec<&str> = out.iter().map(|c| c.text.as_str()).collect();
        // t-if, t-att-class, t-value included; t-name, t-set (name), t-custom-ref excluded.
        assert_eq!(texts, vec!["this.ok", "this.cls", "1 + 2"]);
        for c in &out {
            assert_eq!(c.template_name, "mod.A");
            // Offset slices back to exactly the collected text (verbatim, entities aside).
            assert_eq!(&xml[c.xml_byte_start..c.xml_byte_start + c.text.len()], c.text);
            // No t-foreach/t-set is in scope of these exprs (the t-value runs before `x` binds).
            assert!(c.locals.is_empty());
        }
    }

    #[test]
    fn is_owl_expr_attr_classification() {
        for yes in [
            "t-if", "t-out", "t-att-class", "t-on-click", "t-props", "t-component", "t-foreach",
            // The whole-value `compileExpr` group.
            "t-att", "t-ref", "t-tag", "t-call-context", "t-log",
        ] {
            assert!(is_owl_expression_attr(yes), "{yes} should be an expr attr");
        }
        for no in [
            "t-name", "t-set", "t-as", "t-custom-ref", "class", "id", "t-else",
            // `t-attf-*` is interpolation, not a single expression — handled separately, and
            // must not be caught by the bare `t-att` entry or the `t-att-*` prefix.
            "t-attf-class", "t-attf",
        ] {
            assert!(!is_owl_expression_attr(no), "{no} should NOT be an expr attr");
        }
    }

    #[test]
    fn is_owl_interp_attr_classification() {
        for yes in ["t-attf-class", "t-attf-style", "t-attf-colspan", "t-call"] {
            assert!(is_owl_interpolation_attr(yes), "{yes} should be an interpolation attr");
        }
        // Single-expression directives and the static navigation attrs are not interpolations.
        for no in ["t-att", "t-att-class", "t-if", "t-out", "t-name", "t-inherit", "class"] {
            assert!(!is_owl_interpolation_attr(no), "{no} should NOT be an interpolation attr");
        }
    }

    #[test]
    fn collect_exprs_splits_interpolation_attrs() {
        // Mirrors account/grouped_view_widget.xml: a `t-attf-*` inside a `t-foreach`, plus a
        // dynamic `t-call`. Each `{{…}}` / `#{…}` chunk becomes its own expression.
        let xml = r#"<templates><t t-name="mod.A"><t t-foreach="this.cols" t-as="col" t-key="col_index"><th t-attf-class="pre {{col['x']}} mid #{col.y}"/></t><t t-call="{{this.tpl}}"/></t></templates>"#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let mut out = vec![];
        collect_exprs(document.root_element(), xml, None, &[], &mut out);

        let texts: Vec<&str> = out.iter().map(|c| c.text.as_str()).collect();
        // The two t-attf chunks and the dynamic t-call chunk are all collected as expressions.
        assert!(texts.contains(&"col['x']"), "got {texts:?}");
        assert!(texts.contains(&"col.y"), "got {texts:?}");
        assert!(texts.contains(&"this.tpl"), "got {texts:?}");
        // The literal parts (`pre`, `mid`) are not collected.
        assert!(!texts.iter().any(|t| t.contains("pre") || t.contains("mid")));
        for c in &out {
            // Every chunk offset slices back to exactly its text (verbatim XML).
            assert_eq!(&xml[c.xml_byte_start..c.xml_byte_start + c.text.len()], c.text);
        }
        // The two t-attf chunks see the enclosing loop var `col`; the static `t-call` still
        // yields nothing (no interpolation), so it is absent from `texts`.
        let chunk = out.iter().find(|c| c.text == "col['x']").unwrap();
        assert!(chunk.locals.iter().any(|l| l.name() == "col"));
    }

    #[test]
    fn tag_is_component_matches_owl_rule() {
        let doc = roxmltree::Document::parse(
            r#"<t><ChangeLine/><Foo.Bar/><div/><tr/><t t-component="this.comp"/><t/></t>"#,
        )
        .unwrap();
        let by_tag = |tag: &str| doc.descendants().find(|n| n.has_tag_name(tag)).unwrap();
        // Capitalized tag or `t-component` ⇒ component.
        assert!(tag_is_component(by_tag("ChangeLine")));
        assert!(tag_is_component(by_tag("Foo.Bar")));
        // The dynamic-component `<t t-component=...>` (lowercase tag, has the directive).
        let dynamic = doc.descendants().find(|n| n.has_attribute("t-component")).unwrap();
        assert!(tag_is_component(dynamic));
        // Plain HTML / bare `<t>` ⇒ not a component.
        assert!(!tag_is_component(by_tag("div")));
        assert!(!tag_is_component(by_tag("tr")));
    }

    #[test]
    fn is_prop_expr_attr_classification() {
        // Non-directive attrs on a component are expression props, incl. class/style and the
        // expression-carrying suffixes.
        for yes in ["changeLine", "ordering", "class", "style", "callback.bind", "count.signal", "onDelete.alike"] {
            assert!(is_prop_expr_attr(yes), "{yes} should be an expr prop");
        }
        // `.translate` is a plain (translatable) string; directives are handled elsewhere.
        for no in ["label.translate", "t-if", "t-props", "t-on-click", "t-foreach"] {
            assert!(!is_prop_expr_attr(no), "{no} should NOT be an expr prop");
        }
    }

    #[test]
    fn collect_exprs_emits_static_component_tag_names() {
        // A static component tag is spliced as its own expression; plain tags and the dynamic
        // `<t t-component>` tag are not (the latter contributes its `t-component` expression).
        let xml = r#"<templates><t t-name="mod.A">
            <div t-foreach="this.items" t-as="item">
                <ChangeLine value="item"/>
                <t t-component="this.dyn" foo="item"/>
            </div>
        </t></templates>"#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let mut out = vec![];
        collect_exprs(document.root_element(), xml, None, &[], &mut out);

        let texts: Vec<&str> = out.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"ChangeLine")); // static component tag
        assert!(texts.contains(&"this.dyn")); // dynamic component's expression
        assert!(!texts.contains(&"div")); // plain HTML tag
        assert!(!texts.contains(&"t")); // the `<t t-component>` tag itself

        // The tag-name expr slices back to the XML tag and carries no template-locals: a
        // component tag resolves in module scope, never shadowed by a loop var like `item`.
        let tag = out.iter().find(|c| c.text == "ChangeLine").unwrap();
        assert_eq!(&xml[tag.xml_byte_start..tag.xml_byte_start + tag.text.len()], "ChangeLine");
        assert!(tag.locals.is_empty());
    }

    #[test]
    fn collect_exprs_picks_component_props_with_scope() {
        // Mirrors account_resequence.xml: a `<ChangeLine>` inside a `t-foreach`, with props
        // referencing both the loop var (`changeLine`) and an outer `t-set` (`value`). The
        // sibling `<div>` (plain HTML) must NOT have its `class` collected.
        let xml = r#"<templates><t t-name="account.ResequenceRenderer">
            <t t-set="value" t-value="this.getValue()"/>
            <div class="table">
                <t t-foreach="value.changeLines" t-as="changeLine" t-key="changeLine.id">
                    <ChangeLine changeLine="changeLine" ordering="value.ordering"/>
                </t>
            </div>
        </t></templates>"#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let mut out = vec![];
        collect_exprs(document.root_element(), xml, None, &[], &mut out);

        let texts: Vec<&str> = out.iter().map(|c| c.text.as_str()).collect();
        // `class="table"` on the plain <div> is absent; the `<ChangeLine>` tag name is emitted
        // (before its own props), and both component props are present.
        assert_eq!(
            texts,
            vec!["this.getValue()", "value.changeLines", "changeLine.id", "ChangeLine", "changeLine", "value.ordering"]
        );
        // Every collected expr slices back to its verbatim XML text.
        for c in &out {
            assert_eq!(&xml[c.xml_byte_start..c.xml_byte_start + c.text.len()], c.text);
        }
        // The two `<ChangeLine>` props see both the outer `t-set` and the loop var, in order.
        let props: Vec<&CollectedExpr> = out.iter().filter(|c| c.text == "changeLine" || c.text == "value.ordering").collect();
        assert_eq!(props.len(), 2);
        for p in props {
            assert_eq!(local_names(p), vec!["value", "changeLine"]);
        }
        // The `t-foreach` collection itself is still evaluated in the outer scope (sees only
        // `value`, not the loop var it introduces).
        let foreach = out.iter().find(|c| c.text == "value.changeLines").unwrap();
        assert_eq!(local_names(foreach), vec!["value"]);
    }

    #[test]
    fn collect_template_names_picks_name_call_inherit() {
        // `t-name` (declaration), `t-call` and `t-inherit` (references) are collected in
        // document order; the declaration flag is set only for `t-name`. Other attributes
        // (`t-call-context`, `class`, an empty `t-name`) are ignored.
        let xml = r#"<templates>
            <t t-name="mod.A"><t t-call="mod.B" t-call-context="{}"/></t>
            <t t-name="mod.C" t-inherit="mod.A"><div class="x"/></t>
            <t t-name=""/>
        </templates>"#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let mut out = vec![];
        collect_template_names(document.root_element(), &mut out);

        // Each range slices back to exactly the template name (quotes excluded).
        let got: Vec<(&str, bool)> = out.iter().map(|(r, decl)| (&xml[r.clone()], *decl)).collect();
        assert_eq!(
            got,
            vec![("mod.A", true), ("mod.B", false), ("mod.C", true), ("mod.A", false)]
        );
    }

    #[test]
    fn contains_cursor_is_end_inclusive() {
        let expr = SplicedExpr {
            v_byte_start: 0,
            compiled: "this.x".to_string(),
            xml_byte_start: 10,
            is_tag: false,
        };
        assert!(!expr.contains_cursor(9)); // before
        assert!(expr.contains_cursor(10)); // start
        assert!(expr.contains_cursor(13)); // middle
        assert!(expr.contains_cursor(16)); // end (10 + len 6) — completion cursor
        assert!(!expr.contains_cursor(17)); // past end
    }

    /// Names of the locals in scope for a collected expression, in order.
    fn local_names(expr: &CollectedExpr) -> Vec<&str> {
        expr.locals.iter().map(|l| l.name()).collect()
    }

    #[test]
    fn collect_exprs_threads_foreach_and_tset_scope() {
        let xml = r#"<templates><t t-name="mod.A">
            <t t-set="greeting" t-value="'hi'"/>
            <div t-foreach="this.records" t-as="rec" t-key="rec.id">
                <span t-esc="rec.name"/>
                <t t-if="rec.active" t-esc="greeting"/>
            </div>
        </t></templates>"#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let mut out = vec![];
        collect_exprs(document.root_element(), xml, None, &[], &mut out);

        let texts: Vec<&str> = out.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, vec!["'hi'", "this.records", "rec.id", "rec.name", "rec.active", "greeting"]);

        // The `t-value` of the t-set runs before `greeting` binds ⇒ no locals in scope.
        assert!(out[0].locals.is_empty());
        // The `t-foreach` collection is evaluated in the outer scope ⇒ sees `greeting`, not `rec`.
        assert_eq!(local_names(&out[1]), vec!["greeting"]);
        // Every other attr/descendant of the loop element sees both `greeting` and `rec`.
        assert_eq!(local_names(&out[2]), vec!["greeting", "rec"]); // t-key on the loop element
        assert_eq!(local_names(&out[3]), vec!["greeting", "rec"]); // t-esc on a child
        assert_eq!(local_names(&out[5]), vec!["greeting", "rec"]); // t-esc="greeting"
    }

    #[test]
    fn emit_preamble_types_locals() {
        // t-foreach binds the element + the four derived vars.
        let foreach = vec![TemplateLocal::Foreach {
            name: "rec".into(), collection: "this.records".into(), decl_offset: 0,
        }];
        let (preamble, decls) = emit_preamble(&foreach);
        assert_eq!(
            preamble,
            "let rec = (this.records)[0]; let rec_value = (this.records)[0]; \
             let rec_index = 0; let rec_first = false; let rec_last = false; "
        );
        // Each declared identifier is recorded at its offset within the preamble, all mapping
        // back to the same `t-as` name (`rec`, length 3).
        assert_eq!(decls.len(), 5);
        for (decl, ident) in decls.iter().zip(["rec", "rec_value", "rec_index", "rec_first", "rec_last"]) {
            assert_eq!(&preamble[decl.rel_offset..decl.rel_offset + decl.v_name_len], ident);
            assert_eq!(decl.xml_offset, 0);
            assert_eq!(decl.xml_name_len, 3);
        }
        // t-set value form declares the compiled expression; word-ops are rewritten.
        let set = vec![TemplateLocal::Set { name: "x".into(), value: "a or b".into(), decl_offset: 7 }];
        let (preamble, decls) = emit_preamble(&set);
        assert_eq!(preamble, "let x = (a || b); ");
        assert_eq!(&preamble[decls[0].rel_offset..decls[0].rel_offset + decls[0].v_name_len], "x");
        assert_eq!(decls[0].xml_offset, 7);
        // t-set body form is an `any`.
        let body = vec![TemplateLocal::SetBody { name: "y".into(), decl_offset: 0 }];
        assert_eq!(emit_preamble(&body).0, "let y = /** @type {any} */ (undefined); ");
    }

    #[test]
    fn emit_preamble_dedups_shadowed_names() {
        // A nested t-foreach shadowing an outer one must not emit two `let item` (redeclare).
        let locals = vec![
            TemplateLocal::Foreach { name: "item".into(), collection: "outer".into(), decl_offset: 0 },
            TemplateLocal::Foreach { name: "item".into(), collection: "item.children".into(), decl_offset: 0 },
        ];
        let preamble = emit_preamble(&locals).0;
        assert_eq!(preamble.matches("let item = ").count(), 1);
        // The innermost declaration wins.
        assert!(preamble.contains("let item = (item.children)[0];"));
        assert!(!preamble.contains("(outer)[0]"));
    }

    #[test]
    fn build_virtual_offsets_survive_preamble() {
        // With a non-empty preamble, the recorded expression offset must still land exactly on
        // the compiled expression (past the `let` declarations).
        let import_line = "import { Foo } from \"./foo\";";
        let exprs = vec![CollectedExpr {
            template_name: "m.Foo".into(),
            text: "rec.name".into(),
            xml_byte_start: 50,
            locals: vec![TemplateLocal::Foreach {
                name: "rec".into(), collection: "this.records".into(), decl_offset: 0,
            }],
            is_component_tag: false,
        }];
        let (content, spliced, _decls) = build_virtual(import_line, "Foo", exprs);

        assert!(content.contains("let rec = (this.records)[0];"));
        assert_eq!(&content[spliced[0].v_byte_start..spliced[0].v_byte_end()], "rec.name");
    }

    #[test]
    fn build_virtual_records_local_decls_pointing_at_virtual_names() {
        // The preamble `let <name>` declarations must be located in the assembled virtual file
        // so a go-to-definition on a local can be routed to its XML declaration.
        let import_line = "import { Foo } from \"./foo\";";
        let exprs = vec![CollectedExpr {
            template_name: "m.Foo".into(),
            text: "greeting".into(),
            xml_byte_start: 200,
            // `greeting` declared at XML offset 42 (the `t-set` value).
            locals: vec![TemplateLocal::Set {
                name: "greeting".into(), value: "'hi'".into(), decl_offset: 42,
            }],
            is_component_tag: false,
        }];
        let (content, _spliced, decls) = build_virtual(import_line, "Foo", exprs);

        assert_eq!(decls.len(), 1);
        let decl = &decls[0];
        // The recorded virtual range slices exactly the `greeting` identifier in `let greeting`.
        assert_eq!(&content[decl.v_byte_start..decl.v_byte_end], "greeting");
        // ...and points back at the XML declaration.
        assert_eq!(decl.xml_byte_start, 42);
        assert_eq!(decl.xml_name_len, "greeting".len());
    }

    #[test]
    fn extract_locals_capture_declaration_offsets() {
        // `t-set` / `t-as` declaration offsets must slice back to the declared name in the XML.
        let xml = r#"<templates><t t-name="m.A"><t t-set="greeting" t-value="'hi'"/><div t-foreach="this.recs" t-as="rec"/></t></templates>"#;
        let document = roxmltree::Document::parse(xml).unwrap();
        let mut out = vec![];
        collect_exprs(document.root_element(), xml, None, &[], &mut out);
        // The `t-as="rec"` expr (the div's t-foreach) exposes `rec` to descendants; grab a
        // local from the tree directly to check its recorded offset.
        let div = document.descendants().find(|n| n.has_attribute("t-foreach")).unwrap();
        let foreach = extract_foreach_local(div).unwrap();
        let off = foreach.decl_offset();
        assert_eq!(&xml[off..off + foreach.name().len()], "rec");

        let tset_node = document.descendants().find(|n| n.has_attribute("t-set")).unwrap();
        let tset = extract_tset_local(tset_node).unwrap();
        let off = tset.decl_offset();
        assert_eq!(&xml[off..off + tset.name().len()], "greeting");
    }
}
