use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use iced::widget::{Column, Row, Space, button, column, container, row, scrollable, text, text_input};
use iced::{Border, Color, Element, Fill, Length, Task};

use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol_keys::SymbolKey;

use super::snapshot::{self, EntryPointRow, FileRow, TreemapNode};

/// How many levels of children are computed (and rendered as nested boxes) per treemap
/// drill-in. Deeper descendants still count towards a node's size, just aren't broken out.
const TREEMAP_DEPTH: usize = 2;

pub fn run(odoo: Arc<Mutex<SyncOdoo>>) -> iced::Result {
    iced::application(move || State::new(odoo.clone()), update, view)
        .title("Odoo LS Inspector")
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    EntryPoints,
    Symbols,
    Files,
    SizeMap,
}

struct State {
    odoo: Arc<Mutex<SyncOdoo>>,
    tab: Tab,
    entry_points: Vec<EntryPointRow>,
    files: Vec<FileRow>,
    /// Symbol tree nodes currently expanded. Children are fetched live from `odoo`
    /// on every `view()` call, so this is the only tree state we keep around.
    expanded: HashSet<SymbolKey>,
    symbol_filter: String,
    file_filter: String,
    /// Breadcrumb path for the size map, from the chosen entry point down to the
    /// node currently drilled into. Empty means "no entry point picked yet".
    treemap_stack: Vec<(SymbolKey, String)>,
    /// Size-annotated tree for `treemap_stack`'s last entry, once computed.
    treemap_root: Option<TreemapNode>,
    treemap_loading: bool,
}

impl State {
    fn new(odoo: Arc<Mutex<SyncOdoo>>) -> Self {
        let entry_points = snapshot::entry_points(&odoo);
        let files = snapshot::files(&odoo);
        Self {
            odoo,
            tab: Tab::EntryPoints,
            entry_points,
            files,
            expanded: HashSet::new(),
            symbol_filter: String::new(),
            file_filter: String::new(),
            treemap_stack: Vec::new(),
            treemap_root: None,
            treemap_loading: false,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    SwitchTab(Tab),
    RefreshEntryPoints,
    RefreshFiles,
    ToggleExpand(SymbolKey),
    JumpToSymbol(SymbolKey),
    SymbolFilterChanged(String),
    FileFilterChanged(String),
    TreemapReset,
    TreemapSelectRoot(SymbolKey, String),
    TreemapDrillInto(SymbolKey, String),
    TreemapGoUp(usize),
    TreemapComputed(SymbolKey, TreemapNode),
    TreemapFailed(SymbolKey),
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::SwitchTab(tab) => {
            state.tab = tab;
            Task::none()
        }
        Message::RefreshEntryPoints => {
            state.entry_points = snapshot::entry_points(&state.odoo);
            Task::none()
        }
        Message::RefreshFiles => {
            state.files = snapshot::files(&state.odoo);
            Task::none()
        }
        Message::ToggleExpand(key) => {
            if !state.expanded.remove(&key) {
                state.expanded.insert(key);
            }
            Task::none()
        }
        Message::JumpToSymbol(key) => {
            state.tab = Tab::Symbols;
            state.expanded.insert(key);
            Task::none()
        }
        Message::SymbolFilterChanged(value) => {
            state.symbol_filter = value;
            Task::none()
        }
        Message::FileFilterChanged(value) => {
            state.file_filter = value;
            Task::none()
        }
        Message::TreemapReset => {
            state.treemap_stack.clear();
            state.treemap_root = None;
            state.treemap_loading = false;
            Task::none()
        }
        Message::TreemapSelectRoot(key, label) => {
            state.treemap_stack = vec![(key, label)];
            state.treemap_root = None;
            state.treemap_loading = true;
            compute_treemap_task(state.odoo.clone(), key)
        }
        Message::TreemapDrillInto(key, label) => {
            state.treemap_stack.push((key, label));
            state.treemap_root = None;
            state.treemap_loading = true;
            compute_treemap_task(state.odoo.clone(), key)
        }
        Message::TreemapGoUp(index) => {
            state.treemap_stack.truncate(index + 1);
            state.treemap_root = None;
            if let Some(key) = state.treemap_stack.last().map(|(key, _)| *key) {
                state.treemap_loading = true;
                compute_treemap_task(state.odoo.clone(), key)
            } else {
                state.treemap_loading = false;
                Task::none()
            }
        }
        Message::TreemapComputed(key, node) => {
            if state.treemap_stack.last().map(|(k, _)| *k) == Some(key) {
                state.treemap_root = Some(node);
                state.treemap_loading = false;
            }
            Task::none()
        }
        Message::TreemapFailed(key) => {
            if state.treemap_stack.last().map(|(k, _)| *k) == Some(key) {
                state.treemap_loading = false;
            }
            Task::none()
        }
    }
}

/// Runs the (potentially slow) full-subtree walk on its own OS thread, so it never blocks
/// iced's own executor or the GUI redraw loop while it holds the `SyncOdoo` lock.
fn compute_treemap_task(odoo: Arc<Mutex<SyncOdoo>>, key: SymbolKey) -> Task<Message> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let node = snapshot::treemap(&odoo, key, TREEMAP_DEPTH);
        let _ = tx.send(node);
    });
    Task::perform(rx, move |result| match result {
        Ok(node) => Message::TreemapComputed(key, node),
        Err(_) => Message::TreemapFailed(key),
    })
}

fn view(state: &State) -> Element<'_, Message> {
    let tabs = row![
        tab_button("Entry Points", Tab::EntryPoints, state.tab),
        tab_button("Symbols", Tab::Symbols, state.tab),
        tab_button("Files", Tab::Files, state.tab),
        tab_button("Size Map", Tab::SizeMap, state.tab),
    ]
    .spacing(8);

    let body: Element<'_, Message> = match state.tab {
        Tab::EntryPoints => view_entry_points(state),
        Tab::Symbols => view_symbols(state),
        Tab::Files => view_files(state),
        Tab::SizeMap => view_treemap(state),
    };

    column![tabs, body].spacing(12).padding(12).into()
}

fn tab_button(label: &'static str, tab: Tab, current: Tab) -> Element<'static, Message> {
    let style = if tab == current { button::primary } else { button::secondary };
    button(text(label)).style(style).on_press(Message::SwitchTab(tab)).into()
}

fn view_entry_points(state: &State) -> Element<'_, Message> {
    let refresh = button("Refresh").on_press(Message::RefreshEntryPoints);

    let rows: Vec<Element<'_, Message>> = state
        .entry_points
        .iter()
        .map(|ep| {
            let label = format!("{:?}   {}", ep.typ, ep.path);
            let root_key = ep.root_key;
            button(text(label))
                .style(button::text)
                .on_press(Message::JumpToSymbol(root_key))
                .into()
        })
        .collect();

    if rows.is_empty() {
        column![refresh, text("No entry points yet (workspace may still be indexing).")]
            .spacing(8)
            .into()
    } else {
        column![refresh, scrollable(column(rows).spacing(4)).height(Fill)]
            .spacing(8)
            .into()
    }
}

fn view_symbols(state: &State) -> Element<'_, Message> {
    let filter = text_input("Filter by name...", &state.symbol_filter)
        .on_input(Message::SymbolFilterChanged)
        .width(Fill);
    let refresh = button("Refresh entry points").on_press(Message::RefreshEntryPoints);

    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    for ep in &state.entry_points {
        let label = format!("{:?}   {}", ep.typ, ep.path);
        rows.push(symbol_row(label, ep.root_key, true, 0, state));
        if state.expanded.contains(&ep.root_key) {
            push_symbol_children(state, ep.root_key, 1, &mut rows);
        }
    }

    let tree: Element<'_, Message> = if rows.is_empty() {
        text("No entry points yet (workspace may still be indexing).").into()
    } else {
        scrollable(column(rows).spacing(2)).height(Fill).into()
    };

    column![row![filter, refresh].spacing(8), tree].spacing(8).into()
}

fn push_symbol_children<'a>(
    state: &'a State,
    parent: SymbolKey,
    depth: usize,
    out: &mut Vec<Element<'a, Message>>,
) {
    let children = snapshot::symbol_children(&state.odoo, parent);
    let filter = state.symbol_filter.to_lowercase();
    for child in children {
        let matches = filter.is_empty() || child.name.to_lowercase().contains(&filter);
        if matches {
            let label = match &child.path {
                Some(path) => format!("{:?}   {}   ({path})", child.kind, child.name),
                None => format!("{:?}   {}", child.kind, child.name),
            };
            out.push(symbol_row(label, child.key, child.has_children, depth, state));
        }
        // Descend even when this row is filtered out, so a matching descendant isn't hidden.
        if child.has_children && state.expanded.contains(&child.key) {
            push_symbol_children(state, child.key, depth + 1, out);
        }
    }
}

fn symbol_row<'a>(
    label: String,
    key: SymbolKey,
    has_children: bool,
    depth: usize,
    state: &'a State,
) -> Element<'a, Message> {
    let indent = Space::new().width(Length::Fixed(depth as f32 * 16.0));
    let toggle: Element<'_, Message> = if has_children {
        let arrow = if state.expanded.contains(&key) { "v" } else { ">" };
        button(text(arrow)).style(button::text).on_press(Message::ToggleExpand(key)).into()
    } else {
        Space::new().width(Length::Fixed(20.0)).into()
    };
    row![indent, toggle, text(label)].spacing(6).into()
}

fn view_files(state: &State) -> Element<'_, Message> {
    let filter = text_input("Filter by uri...", &state.file_filter)
        .on_input(Message::FileFilterChanged)
        .width(Fill);
    let refresh = button("Refresh").on_press(Message::RefreshFiles);

    let needle = state.file_filter.to_lowercase();
    let rows: Vec<Element<'_, Message>> = state
        .files
        .iter()
        .filter(|f| needle.is_empty() || f.uri.to_lowercase().contains(&needle))
        .map(file_row)
        .collect();

    let list: Element<'_, Message> = if rows.is_empty() {
        text("No files tracked yet.").into()
    } else {
        scrollable(column(rows).spacing(2)).height(Fill).into()
    };

    column![row![filter, refresh].spacing(8), list].spacing(8).into()
}

fn file_row(file: &FileRow) -> Element<'_, Message> {
    let version = file.version.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());
    let label = format!(
        "{}   [version {}] [{}] [{}]",
        file.uri,
        version,
        if file.valid { "valid" } else { "invalid" },
        if file.opened { "opened" } else { "closed" },
    );
    text(label).into()
}

fn view_treemap(state: &State) -> Element<'_, Message> {
    if state.treemap_stack.is_empty() {
        return view_treemap_picker(state);
    }

    let mut crumbs: Vec<Element<'_, Message>> =
        vec![button(text("Entry Points")).style(button::text).on_press(Message::TreemapReset).into()];
    for (index, (_, label)) in state.treemap_stack.iter().enumerate() {
        crumbs.push(text(" / ").into());
        crumbs.push(
            button(text(label.clone())).style(button::text).on_press(Message::TreemapGoUp(index)).into(),
        );
    }
    let breadcrumb = row(crumbs).spacing(2);

    let content: Element<'_, Message> = if state.treemap_loading {
        text("Computing sizes... (a large entry point can briefly pause the language server)").into()
    } else {
        match &state.treemap_root {
            Some(node) if !node.children.is_empty() => render_siblings(&node.children, Orientation::Row),
            Some(_) => text("This symbol has no children.").into(),
            None => text("...").into(),
        }
    };

    column![breadcrumb, container(content).width(Fill).height(Fill)].spacing(8).into()
}

fn view_treemap_picker(state: &State) -> Element<'_, Message> {
    let rows: Vec<Element<'_, Message>> = state
        .entry_points
        .iter()
        .map(|ep| {
            let label = format!("{:?}   {}", ep.typ, ep.path);
            let path = ep.path.clone();
            button(text(label)).on_press(Message::TreemapSelectRoot(ep.root_key, path)).into()
        })
        .collect();

    if rows.is_empty() {
        text("No entry points yet (workspace may still be indexing).").into()
    } else {
        column![
            text("Pick an entry point to size up:"),
            scrollable(column(rows).spacing(4)).height(Fill),
        ]
        .spacing(8)
        .into()
    }
}

#[derive(Clone, Copy)]
enum Orientation {
    Row,
    Column,
}

impl Orientation {
    fn flip(self) -> Self {
        match self {
            Orientation::Row => Orientation::Column,
            Orientation::Column => Orientation::Row,
        }
    }
}

/// Lays out `nodes` (siblings sharing one parent) proportionally to their `size`, alternating
/// row/column orientation per depth so nested boxes read as a classic 2D treemap.
fn render_siblings(nodes: &[TreemapNode], orientation: Orientation) -> Element<'_, Message> {
    let total: u64 = nodes.iter().map(|n| n.size.max(1)).sum::<u64>().max(1);
    let boxes = nodes.iter().map(|node| {
        let weight = ((node.size.max(1) as f64 / total as f64) * 1000.0).round().clamp(1.0, u16::MAX as f64) as u16;
        (render_node(node, orientation.flip()), weight)
    });

    match orientation {
        Orientation::Row => {
            let mut r = Row::new();
            for (element, weight) in boxes {
                r = r.push(container(element).width(Length::FillPortion(weight)).height(Fill));
            }
            r.height(Fill).into()
        }
        Orientation::Column => {
            let mut c = Column::new();
            for (element, weight) in boxes {
                c = c.push(container(element).height(Length::FillPortion(weight)).width(Fill));
            }
            c.width(Fill).into()
        }
    }
}

fn render_node(node: &TreemapNode, children_orientation: Orientation) -> Element<'_, Message> {
    let label = if node.truncated {
        format!("{:?} {} ({}+)", node.kind, node.name, node.size)
    } else {
        format!("{:?} {} ({})", node.kind, node.name, node.size)
    };
    let header = button(text(label).size(12))
        .style(kind_button_style(node.kind))
        .on_press(Message::TreemapDrillInto(node.key, node.name.clone()))
        .width(Fill);

    let body: Element<'_, Message> = if node.children.is_empty() {
        header.into()
    } else {
        column![header, render_siblings(&node.children, children_orientation)].spacing(2).into()
    };

    container(body)
        .width(Fill)
        .height(Fill)
        .padding(2)
        .clip(true)
        .style(move |_theme| {
            container::Style::default()
                .background(color_for_kind(node.kind))
                .border(Border::default().color(Color::BLACK).width(1.0))
        })
        .into()
}

fn kind_button_style(kind: crate::constants::SymType) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, _status| button::Style::default().with_background(color_for_kind(kind))
}

fn color_for_kind(kind: crate::constants::SymType) -> Color {
    use crate::constants::{PackageType, SymType};
    match kind {
        SymType::ROOT | SymType::DISK_DIR | SymType::NAMESPACE => Color::from_rgb8(0xB0, 0xC4, 0xDE),
        SymType::PACKAGE(PackageType::MODULE) => Color::from_rgb8(0x9F, 0xD3, 0xC7),
        SymType::PACKAGE(PackageType::PYTHON_PACKAGE) => Color::from_rgb8(0x8F, 0xC3, 0xB7),
        SymType::FILE | SymType::COMPILED => Color::from_rgb8(0xA0, 0xD0, 0xF0),
        SymType::CLASS => Color::from_rgb8(0xF4, 0xB4, 0x83),
        SymType::FUNCTION => Color::from_rgb8(0xA8, 0xD8, 0xA0),
        SymType::VARIABLE => Color::from_rgb8(0xD3, 0xD3, 0xD3),
        SymType::XML_FILE
        | SymType::XML_RECORD
        | SymType::XML_FIELD
        | SymType::XML_MENUITEM
        | SymType::XML_TEMPLATE
        | SymType::XML_ASSET
        | SymType::XML_DELETE => Color::from_rgb8(0xC9, 0xA6, 0xE0),
        SymType::CSV_FILE => Color::from_rgb8(0xD2, 0xB4, 0x8C),
        SymType::JS_FILE => Color::from_rgb8(0xF0, 0xE6, 0x8C),
    }
}
