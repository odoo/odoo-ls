use std::sync::{Arc, Mutex};

use crate::constants::SymType;
use crate::core::entry_point::EntryPointType;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::SymbolTable;
use crate::core::symbols::symbol_keys::SymbolKey;

/// Read-only, owned view of an `EntryPoint`, for display in the inspector.
#[derive(Debug, Clone)]
pub struct EntryPointRow {
    pub path: String,
    pub typ: EntryPointType,
    pub root_key: SymbolKey,
}

/// Read-only, owned view of one level of the symbol tree.
#[derive(Debug, Clone)]
pub struct SymbolNode {
    pub key: SymbolKey,
    pub name: String,
    pub kind: SymType,
    pub path: Option<String>,
    pub has_children: bool,
}

/// Read-only, owned view of a `FileInfo`.
#[derive(Debug, Clone)]
pub struct FileRow {
    pub uri: String,
    pub version: Option<i32>,
    pub valid: bool,
    pub opened: bool,
}

/// Kinds whose `SymbolTable::all_symbols` can return children (see `SymbolTable::all_symbols`).
/// Used only to decide whether to show an expand affordance; the real check happens on click.
fn is_container_kind(kind: SymType) -> bool {
    matches!(
        kind,
        SymType::ROOT
            | SymType::DISK_DIR
            | SymType::NAMESPACE
            | SymType::PACKAGE(_)
            | SymType::FILE
            | SymType::CLASS
            | SymType::FUNCTION
    )
}

/// Snapshot every known entry point. Locks `odoo` briefly and copies out plain data.
pub fn entry_points(odoo: &Arc<Mutex<SyncOdoo>>) -> Vec<EntryPointRow> {
    let odoo = odoo.lock().unwrap();
    let mgr = odoo.entry_point_mgr.borrow();
    let mut rows: Vec<EntryPointRow> = mgr
        .iter_all()
        .map(|ep| {
            let ep = ep.borrow();
            EntryPointRow {
                path: ep.path.clone(),
                typ: ep.typ.clone(),
                root_key: ep.root.into(),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    rows
}

/// Snapshot the direct children of `parent` in the symbol tree. One call = one level;
/// call again with a child's key to lazily expand further.
pub fn symbol_children(odoo: &Arc<Mutex<SyncOdoo>>, parent: SymbolKey) -> Vec<SymbolNode> {
    let odoo = odoo.lock().unwrap();
    let st = &odoo.symbol_table;
    let mut rows: Vec<SymbolNode> = st
        .all_symbols(parent)
        .into_iter()
        .map(|key| {
            let kind = key.typ();
            let path = st.paths(key).into_iter().next();
            SymbolNode {
                key,
                name: st.repr(key).to_string(),
                kind,
                path,
                has_children: is_container_kind(kind),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

/// A symbol annotated with the total number of symbols in its subtree, for the size-map tab.
/// A symbol without children always has `size == 1`; a container's size is the sum of its
/// children's sizes. `children` is populated up to the requested depth; beyond that, `size`
/// still reflects the full subtree but `truncated` is set instead of recursing further.
#[derive(Debug, Clone)]
pub struct TreemapNode {
    pub key: SymbolKey,
    pub name: String,
    pub kind: SymType,
    pub size: u64,
    pub children: Vec<TreemapNode>,
    pub truncated: bool,
}

/// Total number of symbols (leaves) in the subtree rooted at `key`, unbounded depth.
fn subtree_size(st: &SymbolTable, key: SymbolKey) -> u64 {
    let children = st.all_symbols(key);
    if children.is_empty() {
        1
    } else {
        children.into_iter().map(|c| subtree_size(st, c)).sum()
    }
}

fn build_treemap(st: &SymbolTable, key: SymbolKey, depth_remaining: usize) -> TreemapNode {
    let name = st.repr(key).to_string();
    let kind = key.typ();
    let direct = st.all_symbols(key);

    if direct.is_empty() {
        return TreemapNode { key, name, kind, size: 1, children: vec![], truncated: false };
    }

    if depth_remaining == 0 {
        let size = direct.into_iter().map(|c| subtree_size(st, c)).sum();
        return TreemapNode { key, name, kind, size, children: vec![], truncated: true };
    }

    let mut children: Vec<TreemapNode> = direct
        .into_iter()
        .map(|c| build_treemap(st, c, depth_remaining - 1))
        .collect();
    children.sort_by(|a, b| b.size.cmp(&a.size));
    let size = children.iter().map(|c| c.size).sum();
    TreemapNode { key, name, kind, size, children, truncated: false }
}

/// Builds a size-annotated tree rooted at `key`, `depth` levels of children deep.
/// This walks the whole subtree under `key` in one lock (needed for accurate sizes) —
/// call it off the GUI thread, since it can take a while for a large entry point.
pub fn treemap(odoo: &Arc<Mutex<SyncOdoo>>, key: SymbolKey, depth: usize) -> TreemapNode {
    let odoo = odoo.lock().unwrap();
    build_treemap(&odoo.symbol_table, key, depth)
}

/// Snapshot every known file (opened or otherwise tracked), sorted by uri.
pub fn files(odoo: &Arc<Mutex<SyncOdoo>>) -> Vec<FileRow> {
    let odoo = odoo.lock().unwrap();
    let file_mgr = odoo.get_file_mgr();
    let file_mgr = file_mgr.borrow();
    let mut rows: Vec<FileRow> = file_mgr
        .files
        .values()
        .map(|file_info| {
            let file_info = file_info.borrow();
            FileRow {
                uri: file_info.uri.clone(),
                version: file_info.version,
                valid: file_info.valid,
                opened: file_info.opened,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.uri.cmp(&b.uri));
    rows
}
