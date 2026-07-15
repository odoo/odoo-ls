//! Find-references for JS/OWL symbols, plus the tsserver "reference roots" expansion that
//! keeps those queries complete. Split out of [`crate::features::owl_virtual`], which owns
//! the virtual-document machinery these functions drive. See
//! `server/docs/owl-virtual-docs.md` §5.

use std::cell::RefCell;
use std::rc::Rc;

use lsp_types::Location;
use ruff_source_file::PositionEncoding;

use crate::core::file_mgr::FileInfo;
use crate::core::js_import_graph;
use crate::core::symbols::symbol_keys::{SymbolKey, XmlTemplateKey};
use crate::core::tsserver_bridge::{ts_to_lsp_location, TsLocation};
use crate::features::owl_virtual::{
    build_virtual_docs, commit_staged_roots, is_owl_artifact_path, is_owl_doc_path,
    is_owl_shim_path, locate_doc_at_cursor, map_virtual_ref, shim_to_real, stage_doc_and_shim,
    MappedRef, OwlVirtualDoc,
};
use crate::threads::SessionInfo;
use crate::utils::{HashMap, HashSet};

/// Append `loc` unless an equal `Location` is already present (the two query halves can
/// overlap on same-file uses; reference sets are small, linear de-dup is enough).
fn push_unique(locations: &mut Vec<Location>, loc: Location) {
    if !locations.iter().any(|l| *l == loc) {
        locations.push(loc);
    }
}

/// A component's virtual doc paired with the XML file it was built from (needed to remap a
/// references hit back onto that template).
struct InheritedDoc {
    doc: OwlVirtualDoc,
    xml_fi: Rc<RefCell<FileInfo>>,
}

/// `class_name -> super_class_name` over every known component whose superclass is a plain
/// identifier — the edge set of the component inheritance graph.
fn build_super_of(session: &SessionInfo) -> HashMap<String, String> {
    session
        .sync_odoo
        .component_descriptors
        .values()
        .filter_map(|d| d.super_class_name.clone().map(|s| (d.class_name.clone(), s)))
        .collect()
}

/// Transitive subclasses of `roots` given a `class -> superclass` edge map. Excludes the roots
/// themselves and is robust against cycles (each class is added at most once).
fn collect_subclasses(super_of: &HashMap<String, String>, roots: &[String]) -> Vec<String> {
    let root_set: HashSet<&str> = roots.iter().map(String::as_str).collect();
    let mut result: Vec<String> = vec![];
    let mut frontier: HashSet<String> = roots.iter().cloned().collect();
    while !frontier.is_empty() {
        let mut next: HashSet<String> = HashSet::default();
        for (child, parent) in super_of {
            if frontier.contains(parent)
                && !root_set.contains(child.as_str())
                && !result.iter().any(|r| r == child)
                && !next.contains(child)
            {
                next.insert(child.clone());
            }
        }
        result.extend(next.iter().cloned());
        frontier = next;
    }
    result
}

/// Ceiling on the transient roots *retained* between queries (expansion roots + open docs).
/// The budget never truncates an answer — a query always expands fully; once the retained
/// set outgrows this, the *next* query drops it first and re-expands (lazy, so the rebuild
/// never lands inside the request the user is waiting on).
const TRANSIENT_ROOT_BUDGET: usize = 1500;

/// Drop the accumulated transient roots once they outgrow [`TRANSIENT_ROOT_BUDGET`]. Must
/// run **before** anything is staged: eviction closes every open virtual doc, including
/// ones the current query is about to use.
fn evict_transient_roots_if_over_budget(session: &mut SessionInfo) {
    if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
        if bridge.transient_root_count() > TRANSIENT_ROOT_BUDGET {
            bridge.evict_transient_roots();
        }
    }
}

/// Stage the files that could reference a symbol declared in any of `anchor_files`, so the
/// following `references` query can see them (tsserver's program only ever grows *forward*
/// from its roots). See [`js_import_graph::reference_roots`].
fn stage_reference_roots(session: &mut SessionInfo, anchor_files: &[String]) {
    if session.sync_odoo.tsserver_bridge.is_none() {
        return; // no tsserver (CLI, tests): building the graph would be wasted work
    }
    let roots = js_import_graph::reference_roots(session, anchor_files);
    if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
        bridge.stage_transient_roots(&roots);
    }
}

/// Anchor set for reference-root and subclass-doc expansion: the `origin` (cursor's) file —
/// a floor that is never lost, even when the declaring file is outside the import graph —
/// unioned with the declaring file(s), deduped.
fn anchor_files_for(origin: &str, declaring: &[String]) -> Vec<String> {
    let mut out = vec![origin.to_string()];
    for f in declaring {
        if !out.contains(f) {
            out.push(f.clone());
        }
    }
    out
}

/// Keep only real `.js` declaration files, deduped: a `.d.ts` or virtual doc is outside the
/// import graph (the origin floor covers it), and a shim path maps to its real file — the
/// import graph is keyed on real files.
fn declaring_js_files(files: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for f in files {
        let f = if is_owl_shim_path(&f) { shim_to_real(&f) } else { f };
        if f.ends_with(".js") && !is_owl_doc_path(&f) && !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

/// Build and stage (not commit) the virtual docs of every transitive subclass of a
/// component defined in `anchor_files`, so a references query on the real member also sees
/// inherited template uses. Anchoring at the *declaring* file reaches the cousin case
/// (member in ancestor `A`, cursor in `B`, use in sibling subclass `C`'s template).
fn stage_subclass_docs(session: &mut SessionInfo, anchor_files: &[String]) -> Vec<InheritedDoc> {
    let anchor_classes: Vec<String> = session
        .sync_odoo
        .component_descriptors
        .values()
        .filter(|d| anchor_files.iter().any(|f| *f == d.file_path))
        .map(|d| d.class_name.clone())
        .collect();
    if anchor_classes.is_empty() {
        return vec![];
    }
    let super_of = build_super_of(session);
    let subclasses = collect_subclasses(&super_of, &anchor_classes);

    // Distinct `.js` files defining those subclasses (one file may define several).
    let mut sub_paths: Vec<String> = vec![];
    for class in &subclasses {
        if let Some(desc) = session.sync_odoo.component_descriptors.get(class) {
            if !sub_paths.contains(&desc.file_path) {
                sub_paths.push(desc.file_path.clone());
            }
        }
    }

    let mut docs: Vec<InheritedDoc> = vec![];
    for sub_path in sub_paths {
        let Some(sub_fi) = session.sync_odoo.get_file_mgr().borrow().get_file_info(&sub_path) else {
            continue;
        };
        for inherited in virtual_docs_for_js(session, &sub_fi) {
            if docs.iter().any(|d| d.doc.virtual_path == inherited.doc.virtual_path) {
                continue;
            }
            if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
                stage_doc_and_shim(bridge, &inherited.doc);
            }
            docs.push(inherited);
        }
    }
    docs
}

/// The virtual docs whose real file is `js_fi`, each paired with the XML `FileInfo` needed
/// to remap hits back onto its template.
fn virtual_docs_for_js(session: &mut SessionInfo, js_fi: &Rc<RefCell<FileInfo>>) -> Vec<InheritedDoc> {
    let js_path = js_fi.borrow().uri.clone();
    let mut out = vec![];
    for xml_path in xml_files_backing_js(session, js_fi) {
        let Some(xml_fi) = session.sync_odoo.get_file_mgr().borrow().get_file_info(&xml_path) else {
            continue;
        };
        for doc in build_virtual_docs(session, &xml_fi) {
            if doc.real_path == js_path {
                out.push(InheritedDoc { doc, xml_fi: xml_fi.clone() });
            }
        }
    }
    out
}

/// Map one hit from a real-member references query (Query A): real results pass through;
/// a hit at a staged doc's path is a template use, remapped onto its XML (non-template doc
/// hits are duplicates of files already in the program, dropped); an unknown virtual path
/// is dropped (leak guard).
fn remap_query_a_hit(
    inherited: &[InheritedDoc],
    encoding: PositionEncoding,
    loc: &TsLocation,
) -> Option<Location> {
    let file = loc.0.as_str();
    if !is_owl_artifact_path(file) {
        return Some(ts_to_lsp_location(loc));
    }
    let id = inherited.iter().find(|id| id.doc.virtual_path == file)?;
    match map_virtual_ref(&id.doc, &id.xml_fi, encoding, loc)? {
        MappedRef::Xml(loc) => Some(loc),
        MappedRef::RealJs(_) => None,
    }
}

/// Find-references with the cursor in a JS file (`references.rs` routes every JS query
/// here). One `references` on the real symbol — over reference roots expanded at its
/// declaring file — answers the JS side *and* the template uses (which bind to the same
/// real member through the staged docs and are remapped onto their XML). Query B recovers
/// the template uses of shim-backed (non-exported) components, whose doc member is a
/// distinct symbol. `None` without a tsserver bridge or when nothing references the symbol.
/// See `server/docs/owl-virtual-docs.md` §5.
pub fn references_js_owl(
    session: &mut SessionInfo,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
) -> Option<Vec<Location>> {
    if session.sync_odoo.tsserver_bridge.is_none() {
        return None;
    }
    let encoding = session.sync_odoo.encoding;
    let js_path = file_info.borrow().uri.clone();

    // Anchor expansion at the *declaring* file (the real file is already open — one cheap
    // round trip); anchoring at the cursor's file would miss an imported symbol's callers.
    let declaring = {
        let bridge = session.sync_odoo.tsserver_bridge.as_mut()?;
        declaring_js_files(bridge.get_definition(&js_path, line, character).into_iter().map(|(f, ..)| f))
    };
    let anchor_files = anchor_files_for(&js_path, &declaring);

    // Evict before staging: eviction closes open virtual docs, including ones staged below.
    evict_transient_roots_if_over_budget(session);
    let inherited = stage_subclass_docs(session, &anchor_files);
    let own_docs = virtual_docs_for_js(session, file_info);
    for InheritedDoc { doc, .. } in own_docs.iter() {
        if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
            stage_doc_and_shim(bridge, doc);
        }
    }
    stage_reference_roots(session, &anchor_files);
    commit_staged_roots(session);

    // Query A's remap set: the component's own doc(s) plus every subclass doc.
    let mut remap_docs = inherited;
    remap_docs.extend(own_docs);

    let mut locations: Vec<Location> = vec![];
    let raw_a = {
        let bridge = session.sync_odoo.tsserver_bridge.as_mut()?;
        bridge.get_references(&js_path, line, character)
    };
    for hit in &raw_a {
        if let Some(loc) = remap_query_a_hit(&remap_docs, encoding, hit) {
            push_unique(&mut locations, loc);
        }
    }

    // Query B — shim-backed own components only: query the shared shim at the cursor (its
    // prefix is byte-identical to the real file, so the position addresses the same
    // declaration) and remap each hit through whichever own doc or shim owns it.
    let own_shim = remap_docs
        .iter()
        .filter(|d| d.doc.real_path == js_path)
        .find_map(|d| d.doc.shim.as_ref().map(|s| s.path.clone()));
    if let Some(shim_path) = own_shim {
        let raw_b = {
            let bridge = session.sync_odoo.tsserver_bridge.as_mut()?;
            bridge.get_references(&shim_path, line, character)
        };
        for hit in &raw_b {
            for d in remap_docs.iter().filter(|d| d.doc.real_path == js_path) {
                if let Some(MappedRef::Xml(loc) | MappedRef::RealJs(loc)) =
                    map_virtual_ref(&d.doc, &d.xml_fi, encoding, hit)
                {
                    push_unique(&mut locations, loc);
                    break;
                }
            }
        }
    }

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

/// Find-references for a `this.member` under the cursor in an XML OWL template — the
/// symmetric counterpart of [`references_js_owl`]. The member's real declaration is first
/// resolved via `definition` against the cursor's doc, then Query A / Query B run as in the
/// JS-origin case. `None` when there is no bridge, no expression at the cursor, or no
/// references. See `server/docs/owl-virtual-docs.md` §5.
pub fn references_xml_owl_member(
    session: &mut SessionInfo,
    file_info: &Rc<RefCell<FileInfo>>,
    line: u32,
    character: u32,
) -> Option<Vec<Location>> {
    let encoding = session.sync_odoo.encoding;
    // Evict before staging: eviction closes open virtual docs, including the cursor's own.
    evict_transient_roots_if_over_budget(session);
    let (doc, xml_byte) = locate_doc_at_cursor(session, file_info, line, character)?;
    let (v_line, v_char) = doc.cursor_ts_pos(xml_byte)?;
    let origin_real_path = doc.real_path.clone();

    // Commit #1 — the cursor's doc alone, so the `definition` below can resolve the member.
    // The result drives root/subclass anchoring, hence the second commit further down.
    if let Some(bridge) = session.sync_odoo.tsserver_bridge.as_mut() {
        stage_doc_and_shim(bridge, &doc);
        bridge.commit_transient_roots();
    }

    // Resolve the member's real declaration; the result is both the Query A anchors and —
    // filtered to real `.js` — the declaring files for expansion.
    let raw_def = {
        let bridge = session.sync_odoo.tsserver_bridge.as_mut()?;
        bridge.get_definition(&doc.virtual_path, v_line, v_char)
    };
    let mut anchors: Vec<(String, u32, u32)> = vec![];
    for (file, sl, sc, _el, _ec) in raw_def {
        // A result landing back in the doc (a local's `let`, the wrapper) is never a member
        // anchor; a shim result maps to the real file so Query A binds the real member (the
        // shim's byte-identical prefix keeps the position valid).
        if file == doc.virtual_path {
            continue;
        }
        let file = if is_owl_shim_path(&file) { shim_to_real(&file) } else { file };
        anchors.push((file, sl, sc));
    }

    // Commit #2 — subclass docs + reference roots, anchored at declaring ∪ origin.
    let declaring = declaring_js_files(anchors.iter().map(|(f, _, _)| f.clone()));
    let anchor_files = anchor_files_for(&origin_real_path, &declaring);
    let inherited = stage_subclass_docs(session, &anchor_files);
    stage_reference_roots(session, &anchor_files);
    commit_staged_roots(session);

    let mut collected: Vec<Location> = vec![];

    // Query A's remap set: subclass docs + the cursor's own doc.
    let mut remap_docs = inherited;
    remap_docs.push(InheritedDoc { doc, xml_fi: file_info.clone() });

    // Query A — cross-file JS callers + own/subclass-template references, over each anchor.
    for (path, al, ac) in &anchors {
        let raw_a = {
            let bridge = session.sync_odoo.tsserver_bridge.as_mut()?;
            bridge.get_references(path, *al, *ac)
        };
        for hit in &raw_a {
            if let Some(loc) = remap_query_a_hit(&remap_docs, encoding, hit) {
                push_unique(&mut collected, loc);
            }
        }
    }

    // Query B — the cursor's own doc, only when shim-backed (its `this.member` is then a
    // symbol distinct from the real member, invisible to Query A).
    let own = remap_docs.last().expect("just pushed the cursor's own doc");
    if own.doc.shim.is_some() {
        let raw_b = {
            let bridge = session.sync_odoo.tsserver_bridge.as_mut()?;
            bridge.get_references(&own.doc.virtual_path, v_line, v_char)
        };
        for hit in &raw_b {
            match map_virtual_ref(&own.doc, &own.xml_fi, encoding, hit) {
                Some(MappedRef::Xml(loc)) | Some(MappedRef::RealJs(loc)) => push_unique(&mut collected, loc),
                None => {}
            }
        }
    }

    if collected.is_empty() {
        None
    } else {
        Some(collected)
    }
}

/// Distinct paths of XML files declaring (via `<t t-name>`) a template used by a component
/// of this `.js` file: `static template` refs → `js_templates` → each symbol's parent file.
fn xml_files_backing_js(session: &mut SessionInfo, file_info: &Rc<RefCell<FileInfo>>) -> Vec<String> {
    let template_names: Vec<String> = file_info
        .borrow()
        .file_info_ast
        .borrow()
        .ast.as_js_ast()
        .js_template_refs
        .iter()
        .map(|template_ref| template_ref.t_name.clone())
        .collect();

    let mut paths: Vec<String> = vec![];
    for name in template_names {
        // Collect the valid keys first, releasing the table borrows before indexing again.
        let Some(keys) = session
            .sync_odoo
            .js_templates
            .get(&name)
            .map(|templates| templates.iter_valid(&session.sync_odoo.symbol_table).collect::<Vec<XmlTemplateKey>>())
        else {
            continue;
        };
        for key in keys {
            let SymbolKey::XmlFile(xml_file) = session.st()[key].parent() else {
                continue;
            };
            let path = session.st()[xml_file].path.clone();
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range};
    use crate::core::file_mgr::FileMgr;

    #[test]
    fn declaring_js_files_keeps_only_real_js_declarations() {
        let files = declaring_js_files([
            "/a/base.js".to_string(),               // real .js — kept
            "/a/base.js".to_string(),               // duplicate — dropped
            "/lib/owl/owl.d.ts".to_string(),        // .d.ts (e.g. Owl Component) — outside the graph
            "/a/types.ts".to_string(),              // .ts — origin floor covers it
            "/a/foo.Foo.__ols_owl__.js".to_string(), // a virtual doc — never a declaring file
            "/a/child.__ols_shim__.js".to_string(), // a shim → its real file is the declaring one
        ]);
        assert_eq!(files, vec!["/a/base.js".to_string(), "/a/child.js".to_string()]);
    }

    #[test]
    fn anchor_files_for_unions_origin_first_and_dedups() {
        // Origin is always present and listed first (the floor), declaring files follow, deduped.
        assert_eq!(
            anchor_files_for("/a/cursor.js", &["/a/base.js".to_string(), "/a/cursor.js".to_string()]),
            vec!["/a/cursor.js".to_string(), "/a/base.js".to_string()],
        );
        // Same-file declaration (the common case) collapses to just the origin — a no-op.
        assert_eq!(
            anchor_files_for("/a/cursor.js", &["/a/cursor.js".to_string()]),
            vec!["/a/cursor.js".to_string()],
        );
        // No declaration resolved falls back to the origin roots (current behaviour).
        assert_eq!(anchor_files_for("/a/cursor.js", &[]), vec!["/a/cursor.js".to_string()]);
    }

    #[test]
    fn push_unique_drops_exact_duplicates_only() {
        let loc = |uri: &str, l: u32| Location {
            uri: FileMgr::pathname2uri(uri),
            range: Range {
                start: Position { line: l, character: 0 },
                end: Position { line: l, character: 4 },
            },
        };
        let mut v = vec![];
        push_unique(&mut v, loc("/a.js", 1));
        push_unique(&mut v, loc("/a.js", 1)); // exact duplicate — dropped (Query A/B overlap)
        push_unique(&mut v, loc("/a.js", 2)); // different range — kept
        push_unique(&mut v, loc("/b.xml", 1)); // different uri — kept
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn collect_subclasses_walks_transitively_and_excludes_roots() {
        // A ← B ← C, A ← D, plus an unrelated E ← F.
        let mut super_of: HashMap<String, String> = HashMap::default();
        super_of.insert("B".into(), "A".into());
        super_of.insert("C".into(), "B".into());
        super_of.insert("D".into(), "A".into());
        super_of.insert("F".into(), "E".into());
        let mut subs = collect_subclasses(&super_of, &["A".to_string()]);
        subs.sort();
        assert_eq!(subs, vec!["B".to_string(), "C".to_string(), "D".to_string()]);
        assert!(!subs.contains(&"A".to_string())); // root excluded
        assert!(!subs.contains(&"F".to_string())); // unrelated branch excluded

        // A cycle must terminate (each class added at most once).
        let mut cyc: HashMap<String, String> = HashMap::default();
        cyc.insert("X".into(), "Y".into());
        cyc.insert("Y".into(), "X".into());
        assert_eq!(collect_subclasses(&cyc, &["X".to_string()]), vec!["Y".to_string()]);
    }
}
