#![cfg(feature = "python-cli")]
//! Native Python module (`odoo_ls`) exposed to the embedded RustPython interpreter started by
//! `--interactive` mode (see `cli_interactive.rs`). Gives scripts/the console read access to the
//! parsed symbol table: `odoo_ls.get_symbol(path)`, `Symbol.children()`, `Symbol.parent()`, etc.

use std::sync::{Mutex, OnceLock};

use crossbeam_channel::{Receiver, Sender};
use lsp_server::Message;
use lsp_types::notification::{LogMessage, Notification, PublishDiagnostics};
use lsp_types::{LogMessageParams, PublishDiagnosticsParams};

use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::symbol_keys::SymbolKey;
use crate::core::odoo::SyncOdoo;
use crate::threads::SessionInfo;

static ODOO: OnceLock<Mutex<SyncOdoo>> = OnceLock::new();
static CHANNEL: OnceLock<(Sender<Message>, Receiver<Message>)> = OnceLock::new();

/// Installs the long-lived `SyncOdoo` built by `cli_backend::build_and_init_odoo` as the backing
/// store for every `odoo_ls.*` call. Must run once, before the interpreter starts.
pub(crate) fn install(sync_odoo: SyncOdoo, sender: Sender<Message>, receiver: Receiver<Message>) {
    ODOO.set(Mutex::new(sync_odoo))
        .unwrap_or_else(|_| panic!("python_api::install called twice"));
    CHANNEL
        .set((sender, receiver))
        .unwrap_or_else(|_| panic!("python_api::install called twice"));
}

/// Prints any `LogMessage`/`PublishDiagnostics` notification produced since the last drain (e.g.
/// by the initial `SyncOdoo::init` or a later `odoo_ls.reload()`) to stderr. Nothing else ever
/// reads this channel in interactive mode, so without an explicit drain it grows unbounded across
/// a long session.
pub(crate) fn drain_pending_messages() {
    let Some((_, r)) = CHANNEL.get() else { return };
    while let Ok(msg) = r.try_recv() {
        let Message::Notification(n) = msg else { continue };
        match n.method.as_str() {
            LogMessage::METHOD => {
                if let Ok(p) = serde_json::from_value::<LogMessageParams>(n.params) {
                    eprintln!("[odoo_ls] {:?}: {}", p.typ, p.message);
                }
            }
            PublishDiagnostics::METHOD => {
                if let Ok(p) = serde_json::from_value::<PublishDiagnosticsParams>(n.params)
                    && !p.diagnostics.is_empty() {
                        eprintln!("[odoo_ls] {} diagnostic(s) in {:?}", p.diagnostics.len(), p.uri);
                    }
            }
            _ => {}
        }
    }
}

fn with_odoo<R>(f: impl FnOnce(&mut SessionInfo) -> R) -> R {
    let mutex = ODOO.get().expect("odoo_ls: python API not installed");
    let mut guard = mutex.lock().unwrap();
    let (s, r) = CHANNEL.get().expect("odoo_ls: python API not installed");
    let mut session = SessionInfo::new_from_custom_channel(s.clone(), r.clone(), None, &mut guard);
    f(&mut session)
}

/// Resolve one path segment from `target`: first try a filesystem-based child (file/subpackage),
/// then fall back to a content-based one (class/function/variable) visible at the end of `target`.
fn resolve_child(st: &SymbolTable, target: SymbolKey, name: &str) -> Option<SymbolKey> {
    if let Some(child) = st.get_module_symbol(target, name) {
        return Some(child);
    }
    st.get_content_symbol(target, name, u32::MAX).symbols.first().copied()
}

#[rustpython_vm::pymodule]
pub mod odoo_ls {
    use rustpython_vm::builtins::PyListRef;
    use rustpython_vm::{PyPayload, PyResult, VirtualMachine, pyclass};

    use crate::constants::OYarn;
    use crate::core::odoo::SyncOdoo;
    use crate::core::symbols::symbol_keys::{SymbolKey, Wk};

    use super::{resolve_child, with_odoo};

    #[pyattr]
    #[pyclass(module = "odoo_ls", name = "Symbol")]
    #[derive(PyPayload)]
    pub struct Symbol {
        key: Wk<SymbolKey>,
    }

    impl std::fmt::Debug for Symbol {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Symbol")
        }
    }

    impl Symbol {
        pub(super) fn wrap(key: SymbolKey) -> Self {
            Symbol { key: key.into() }
        }

        fn resolve(&self, vm: &VirtualMachine) -> PyResult<SymbolKey> {
            with_odoo(|session| {
                self.key.upgrade(&session.sync_odoo.symbol_table).ok_or_else(|| {
                    vm.new_value_error("stale symbol; call odoo_ls.reload()".to_owned())
                })
            })
        }
    }

    #[pyclass]
    impl Symbol {
        #[pygetset]
        fn name(&self, vm: &VirtualMachine) -> PyResult<String> {
            let key = self.resolve(vm)?;
            Ok(with_odoo(|session| session.sync_odoo.symbol_table.name(key).to_string()))
        }

        #[pygetset(name = "type")]
        fn type_(&self, vm: &VirtualMachine) -> PyResult<String> {
            let key = self.resolve(vm)?;
            Ok(format!("{:?}", key.typ()))
        }

        #[pygetset]
        fn path(&self, vm: &VirtualMachine) -> PyResult<String> {
            let key = self.resolve(vm)?;
            Ok(with_odoo(|session| {
                session
                    .sync_odoo
                    .symbol_table
                    .get_local_tree(key)
                    .flatten()
                    .iter()
                    .map(|part| part.to_string())
                    .collect::<Vec<_>>()
                    .join(".")
            }))
        }

        #[pygetset]
        fn file_path(&self, vm: &VirtualMachine) -> PyResult<Option<String>> {
            let key = self.resolve(vm)?;
            Ok(with_odoo(|session| {
                key.as_source_file_key()
                    .map(|source_key| session.sync_odoo.symbol_table.file_path(source_key).to_string())
            }))
        }

        #[pymethod]
        fn parent(&self, vm: &VirtualMachine) -> PyResult<Option<Symbol>> {
            let key = self.resolve(vm)?;
            Ok(with_odoo(|session| session.sync_odoo.symbol_table.parent(key)).map(Symbol::wrap))
        }

        #[pymethod]
        fn children(&self, vm: &VirtualMachine) -> PyResult<PyListRef> {
            let key = self.resolve(vm)?;
            let children = with_odoo(|session| session.sync_odoo.symbol_table.children(key));
            Ok(vm.ctx.new_list(
                children.into_iter().map(|k| vm.new_pyobj(Symbol::wrap(k))).collect(),
            ))
        }

        #[pymethod]
        fn get(&self, name: String, vm: &VirtualMachine) -> PyResult<Option<Symbol>> {
            let key = self.resolve(vm)?;
            Ok(
                with_odoo(|session| resolve_child(&session.sync_odoo.symbol_table, key, &name))
                    .map(Symbol::wrap),
            )
        }
    }

    #[pyfunction]
    fn get_symbol(path: String, _vm: &VirtualMachine) -> PyResult<Option<Symbol>> {
        Ok(with_odoo(|session| {
            let mut parts = path.split('.');
            let first = parts.next()?;
            let module_key = session
                .sync_odoo
                .modules
                .get(&OYarn::from(first.to_string()))
                .and_then(|weak| weak.upgrade(&session.sync_odoo.symbol_table))?;
            let mut current: SymbolKey = module_key.into();
            for part in parts {
                current = resolve_child(&session.sync_odoo.symbol_table, current, part)?;
            }
            Some(Symbol::wrap(current))
        }))
    }

    #[pyfunction]
    fn modules(vm: &VirtualMachine) -> PyListRef {
        let names =
            with_odoo(|session| session.sync_odoo.modules.keys().map(|name| name.to_string()).collect::<Vec<_>>());
        vm.ctx.new_list(names.into_iter().map(|n| vm.new_pyobj(n)).collect())
    }

    #[pyfunction]
    fn workspace_folders(vm: &VirtualMachine) -> PyListRef {
        let paths: Vec<String> = with_odoo(|session| {
            session
                .sync_odoo
                .get_file_mgr()
                .borrow()
                .get_unique_workspace_folders()
                .into_values()
                .collect()
        });
        vm.ctx.new_list(paths.into_iter().map(|p| vm.new_pyobj(p)).collect())
    }

    #[pyfunction]
    fn reload() {
        with_odoo(|session| {
            let config = session.sync_odoo.config.clone();
            SyncOdoo::reset(session, config);
        });
        super::drain_pending_messages();
    }
}
