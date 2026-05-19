use crate::utils::HashMap;
use crate::{constants::OYarn, core::symbols::symbol_keys::SymbolKey, oyarn};

#[derive(Debug)]
pub struct NamespaceDirectory {
    pub path: String,
    pub(super) module_symbols: HashMap<OYarn, SymbolKey>,
}

impl NamespaceDirectory {
    pub fn module_symbols(&self) -> &HashMap<OYarn, SymbolKey> {
        &self.module_symbols
    }
}

#[derive(Debug)]
pub struct NamespaceSymbol {
    pub name: OYarn,
    pub is_external: bool,
    pub in_workspace: bool,

    // parent / child symbols
    parent: SymbolKey,
    pub(super) directories: Vec<NamespaceDirectory>,
}

impl NamespaceSymbol {

    pub fn new(name: &str, paths: Vec<String>, parent: SymbolKey, is_external: bool) -> Self {
        let directories = paths.into_iter().map(|p| NamespaceDirectory {
            path: p,
            module_symbols: HashMap::default(),
        }).collect();
        Self {
            name: oyarn!("{}", name),
            directories,
            is_external,
            parent,
            in_workspace: false,
        }
    }

    pub fn paths(&self) -> Vec<String> {
        self.directories.iter().map(|x| {x.path.clone()}).collect()
    }

    pub fn add_path(&mut self, path: String) {
        self.directories.push(NamespaceDirectory { path: path, module_symbols: HashMap::default() });
    }

    pub fn directories(&self) -> &[NamespaceDirectory] {
        &self.directories
    }

    pub fn parent(&self) -> SymbolKey {
        self.parent
    }

    pub fn children(&self) -> Vec<SymbolKey> {
        self.directories.iter()
            .flat_map(|d| d.module_symbols.values())
            .copied().collect()
    }

}
