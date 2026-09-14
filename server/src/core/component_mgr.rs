use slotmap::{new_key_type, SlotMap};

use crate::{
    core::{
        js_arch_builder::{
            ComponentDescriptor, JsExportKind, JsImportKind, JsTemplateRef, SuperClassRef,
        },
        js_import_graph::resolve_import_specifier,
    },
    threads::SessionInfo,
    utils::{HashMap, HashSet},
};

new_key_type! { struct ComponentKey; }

// Makes private functions unit-testable without needing to take session
trait ImportResolver {
    fn resolve(&self, specifier: &str, importer: &str) -> Option<String>;
}

impl ImportResolver for SessionInfo<'_> {
    fn resolve(&self, specifier: &str, importer: &str) -> Option<String> {
        resolve_import_specifier(self, specifier, importer)
    }
}

#[derive(Debug, Default)]
pub struct ComponentMgr {
    // storage
    descriptors: SlotMap<ComponentKey, ComponentDescriptor>,
    // keyed by file path
    by_file: HashMap<String, Vec<ComponentKey>>,
    // keyed by template name
    by_template: HashMap<String, Vec<ComponentKey>>,
}

impl ComponentMgr {
    pub fn index_file(&mut self, path: &str, descriptors: Vec<ComponentDescriptor>) {
        self.forget_file(path);
        if descriptors.is_empty() { return; }
        let by_file_entry = self.by_file.entry(path.to_string()).or_default();
        for descriptor in descriptors {
            debug_assert_eq!(descriptor.file_path, path);
            let template_name = descriptor.template.as_ref().map(|t| t.t_name.clone());
            // add to storage
            let component_key = self.descriptors.insert(descriptor);
            // add to by_file map
            by_file_entry.push(component_key);
            // add to by_templates map
            if let Some(template) = template_name {
                self.by_template.entry(template).or_default().push(component_key);
            }
        }
    }

    pub fn forget_file(&mut self, path: &str) {
        let Some(components) = self.by_file.remove(path) else {
            return;
        };
        for component_key in components {
            let Some(descriptor) = self.descriptors.remove(component_key) else {
                continue;
            };
            // Remove entry from by_template map
            if let Some(template_ref) = &descriptor.template
                && let Some(keys) = self.by_template.get_mut(&template_ref.t_name)
            {
                keys.retain(|&k| k != component_key);
                if keys.is_empty() {
                    self.by_template.remove(&template_ref.t_name);
                }
            }
        }
    }

    pub fn components(&self) -> impl Iterator<Item = &ComponentDescriptor> {
        self.descriptors.values()
    }

    pub fn components_in_file(&self, path: &str) -> impl Iterator<Item = &ComponentDescriptor> {
        self.by_file.get(path)
            .into_iter()
            .flatten()
            .map(|&key| &self.descriptors[key])
    }

    pub fn components_by_template(&self, template_name: &str) -> impl Iterator<Item = &ComponentDescriptor> {
        self.by_template.get(template_name)
            .into_iter()
            .flatten()
            .map(|&key| &self.descriptors[key])
    }
    
    // currently only called in tests
    pub fn get_component(&self, path: &str, name: &str) -> Option<&ComponentDescriptor> {
        self.key_by_file_and_name(path, name).map(|key| &self.descriptors[key])
    }
 
    // Returns first match in file
    fn key_by_file_and_name(&self, path: &str, name: &str) -> Option<ComponentKey> {
        self.by_file.get(path)?
            .iter()
            .copied()
            .find(|&key| self.descriptors[key].class_name == name)
    }

    pub fn template_refs_by_file(&self, path: &str) -> impl Iterator<Item=&JsTemplateRef> {
        self.by_file.get(path)
            .into_iter()
            .flatten()
            .filter_map(|&key| self.descriptors[key].template.as_ref())
    }

    pub fn get_super(&self, session: &SessionInfo, descriptor: &ComponentDescriptor) -> Option<&ComponentDescriptor> {
        self.super_key(descriptor, session).map(|key| &self.descriptors[key])
    }

    fn super_key(
        &self,
        descriptor: &ComponentDescriptor,
        import_resolver: &impl ImportResolver
    ) -> Option<ComponentKey> {
        let super_class = descriptor.super_class.as_ref()?;
        let path = &descriptor.file_path;
        match super_class {
            SuperClassRef::Local(name) => self.key_by_file_and_name(path, name),
            SuperClassRef::Imported(import) => {
                let imported_path = import_resolver.resolve(&import.specifier, path)?;
                self.key_by_import(&imported_path, &import.kind)
            }
        }
    }
    
    fn key_by_import(&self, path: &str, import: &JsImportKind) -> Option<ComponentKey> {
        self.by_file.get(path)?
            .iter()
            .copied()
            .find(|&key| {
                let ComponentDescriptor { class_name, export_kind, ..} = &self.descriptors[key];
                match import {
                    JsImportKind::Named(name) => *export_kind == JsExportKind::Named && class_name == name,
                    JsImportKind::Default => *export_kind == JsExportKind::Default,
                }
            })
    }


    /// The component descriptor backing `template_name`. `None` when nothing declares it.
    /// The base-most class in case of multiple classes. None if the classes are unrelated.
    pub fn component_for_template(&self, session: &SessionInfo, template_name: &str) -> Option<&ComponentDescriptor> {
        self.component_key_for_template(template_name, session)
            .map(|key| &self.descriptors[key])
    }
 
    fn component_key_for_template(
        &self,
        template_name: &str,
        import_resolver: &impl ImportResolver,
    ) -> Option<ComponentKey> {
        let candidates = self.by_template.get(template_name)?;
        candidates.iter().copied().find(|&candidate| {
            candidates.iter().all(|&other| {
                other == candidate || self.is_ancestor(candidate, other, import_resolver)
            })
        })
    }
 
    /// Whether `ancestor` appears on `descendant`'s superclass chain.
    /// `false` when the chain runs out or loops back on itself (cycle-guarded by `seen`).
    fn is_ancestor(
        &self,
        ancestor: ComponentKey,
        descendant: ComponentKey,
        import_resolver: &impl ImportResolver,
    ) -> bool {
        let mut current = descendant;
        let mut seen = HashSet::default();
        while seen.insert(current) {
            let Some(sup) = self.super_key(&self.descriptors[current], import_resolver) else {
                return false;
            };
            if sup == ancestor {
                return true;
            }
            current = sup;
        }
        false // cycle
    }
    
    /// The subclasses of the components in `file_paths`
    pub fn subclasses_of_files(&self, session: &SessionInfo, file_paths: &[String]) -> Vec<&ComponentDescriptor> {
        let subclasses = self.subclass_keys_of_files(file_paths, session);
        subclasses.into_iter().map(|key| &self.descriptors[key]).collect()
    }
    
    fn subclass_keys_of_files(
        &self,
        file_paths: &[String],
        import_resolver: &impl ImportResolver
    ) -> Vec<ComponentKey> {
        let anchor_classes: Vec<_> = file_paths
            .iter()
            .filter_map(|path| self.by_file.get(path))
            .flatten()
            .copied()
            .collect();
        if anchor_classes.is_empty() {
            return vec![];
        }
        let super_of = self.build_super_of(import_resolver);
        Self::collect_subclasses(&super_of, &anchor_classes)
    }
 
    /// `class -> super_class` over every known component — the edge set of the component inheritance graph.
    fn build_super_of(&self, import_resolver: &impl ImportResolver) -> HashMap<ComponentKey, ComponentKey> {
        self.descriptors.iter()
            .filter_map(|(key, descriptor)|
                self.super_key(descriptor, import_resolver).map(|parent| (key, parent))
            )
            .collect()
    }

    /// Transitive subclasses of `roots` given a `class -> superclass` edge map. Excludes the roots
    /// themselves and is robust against cycles (each class is added at most once).
    fn collect_subclasses(super_of: &HashMap<ComponentKey, ComponentKey>, roots: &[ComponentKey]) -> Vec<ComponentKey> {
        let root_set: HashSet<ComponentKey> = roots.iter().copied().collect();
        let mut result = vec![];
        let mut frontier: HashSet<ComponentKey> = root_set.clone();
        while !frontier.is_empty() {
            let mut next: HashSet<_> = HashSet::default();
            for (child, parent) in super_of {
                if frontier.contains(parent)
                    && !root_set.contains(child)
                    && !result.iter().any(|r| r == child)
                    && !next.contains(child)
                {
                    next.insert(*child);
                }
            }
            result.extend(next.iter().copied());
            frontier = next;
        }
        result
    }
    
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextRange;
    use crate::core::js_arch_builder::{ImportSource, JsExportKind, JsTemplateRef};
    use super::*;

    /// Resolves the `./{Class}` specifiers built by [`desc`] back to `{Class}.js`.
    struct TestResolver;

    impl ImportResolver for TestResolver {
        fn resolve(&self, specifier: &str, _importer: &str) -> Option<String> {
            Some(format!("{}.js", specifier.trim_start_matches("./")))
        }
    }

    fn desc(
        file_path: &str,
        class: &str,
        super_class: Option<&str>,
        template: Option<&str>,
    ) -> ComponentDescriptor {
        ComponentDescriptor {
            class_name: class.to_string(),
            file_path: file_path.to_string(),
            class_name_byte: 0,
            super_class: super_class.map(|name| SuperClassRef::Imported(ImportSource {
                specifier: format!("./{name}"),
                kind: JsImportKind::Named(name.to_string()),
            })),
            export_kind: JsExportKind::Named,
            template: template.map(|t_name| JsTemplateRef {
                range: TextRange::default(),
                t_name: t_name.to_string(),
            }),
        }
    }

    /// One class per file (`{class}.js`), indexed in the order given — the build order.
    fn indexed(entries: &[(&str, Option<&str>, Option<&str>)]) -> ComponentMgr {
        let mut result = ComponentMgr::default();
        for &(class, super_class, template) in entries {
            let path = format!("{class}.js");
            result.index_file(&path, vec![desc(&path, class, super_class, template)]);
        }
        result
    }

    #[test]
    fn a_default_import_super_resolves_to_the_default_export() {
        let mut default_export = desc("base.js", "Base", None, None);
        default_export.export_kind = JsExportKind::Default;

        let mut mgr = ComponentMgr::default();
        // the named class is indexed first: the pick must be by export kind, not by position
        mgr.index_file("base.js", vec![desc("base.js", "Helper", None, None), default_export]);
        mgr.index_file("plain.js", vec![desc("plain.js", "Plain", None, None)]);

        let leaf = default_import_desc("leaf.js", "Leaf", "./base");
        assert_eq!(super_name(&mgr, &leaf).as_deref(), Some("Base"));

        // `plain.js` declares no default export
        let stray = default_import_desc("stray.js", "Stray", "./plain");
        assert_eq!(super_name(&mgr, &stray), None);

        let missing = default_import_desc("missing.js", "Missing", "./nowhere");
        assert_eq!(super_name(&mgr, &missing), None);
    }

    #[test]
    fn is_ancestor_walks_the_super_chain() {
        // Base <- Middle <- Leaf, plus an Orphan whose superclass is not indexed.
        let mgr = indexed(&[
            ("Base", None, None),
            ("Middle", Some("Base"), None),
            ("Leaf", Some("Middle"), None),
            ("Orphan", Some("NotIndexed"), None),
        ]);
        let k = |class: &str| key(&mgr, class);
        assert!(mgr.is_ancestor(k("Base"), k("Leaf"), &TestResolver)); // transitive
        assert!(mgr.is_ancestor(k("Middle"), k("Leaf"), &TestResolver)); // direct
        assert!(!mgr.is_ancestor(k("Leaf"), k("Base"), &TestResolver)); // wrong direction
        assert!(!mgr.is_ancestor(k("Leaf"), k("Leaf"), &TestResolver)); // not its own ancestor
        assert!(!mgr.is_ancestor(k("Base"), k("Orphan"), &TestResolver)); // unresolvable super
    }

    #[test]
    fn is_ancestor_survives_a_cyclic_chain() {
        // A extends B extends A — must terminate, not loop forever.
        let mgr = indexed(&[("A", Some("B"), None), ("B", Some("A"), None), ("Other", None, None)]);
        let k = |class: &str| key(&mgr, class);
        assert!(mgr.is_ancestor(k("B"), k("A"), &TestResolver));
        assert!(!mgr.is_ancestor(k("Other"), k("A"), &TestResolver));
    }

    #[test]
    fn subclasses_of_files_walks_transitively_and_excludes_the_roots() {
        // A <- B <- C, A <- D, plus an unrelated E <- F.
        let mgr = indexed(&[
            ("A", None, None),
            ("B", Some("A"), None),
            ("C", Some("B"), None),
            ("D", Some("A"), None),
            ("E", None, None),
            ("F", Some("E"), None),
        ]);
        assert_eq!(subclass_names(&mgr, &["A.js"]), ["B", "C", "D"]);
        assert!(subclass_names(&mgr, &["C.js"]).is_empty()); // a leaf has none
    }

    #[test]
    fn subclasses_of_files_unions_every_root_file() {
        let mgr = indexed(&[
            ("A", None, None),
            ("B", Some("A"), None),
            ("E", None, None),
            ("F", Some("E"), None),
        ]);
        assert_eq!(subclass_names(&mgr, &["A.js", "E.js"]), ["B", "F"]);
    }

    #[test]
    fn subclasses_of_files_survives_a_cycle() {
        let mgr = indexed(&[("X", Some("Y"), None), ("Y", Some("X"), None)]);
        assert_eq!(subclass_names(&mgr, &["X.js"]), ["Y"]);
    }

    #[test]
    fn subclasses_of_files_is_empty_when_the_files_declare_no_component() {
        let mgr = indexed(&[("A", None, None), ("B", Some("A"), None)]);
        assert!(subclass_names(&mgr, &["nothing.js"]).is_empty());
    }

    #[test]
    fn component_for_template_keeps_the_base_whatever_the_build_order() {
        // Base <- Middle <- Leaf, with Base and Leaf declaring the template and Middle declaring
        // nothing: the super-chain walk has to reach outside the declaring set to find the base.
        let entries: [(&str, Option<&str>, Option<&str>); 3] = [
            ("Base", None, Some("mod.T")),
            ("Middle", Some("Base"), None),
            ("Leaf", Some("Middle"), Some("mod.T")),
        ];
        let forward = indexed(&entries);
        assert_eq!(base_of(&forward, "mod.T"), Some("Base"));

        let mut reversed = entries;
        reversed.reverse();
        assert_eq!(base_of(&indexed(&reversed), "mod.T"), Some("Base"));
    }

    #[test]
    fn component_for_template_refuses_to_guess_between_unrelated_classes() {
        let entries: [(&str, Option<&str>, Option<&str>); 2] =
            [("Other", None, Some("mod.T")), ("Base", None, Some("mod.T"))];
        assert_eq!(base_of(&indexed(&entries), "mod.T"), None);

        let mut reversed = entries;
        reversed.reverse();
        assert_eq!(base_of(&indexed(&reversed), "mod.T"), None);
    }

    #[test]
    fn component_for_template_refuses_to_guess_between_siblings() {
        // web_studio's AvatarHook/ButtonHook: a shared base is no reason to prefer either.
        let mgr = indexed(&[
            ("Base", None, None),
            ("Avatar", Some("Base"), Some("mod.T")),
            ("Button", Some("Base"), Some("mod.T")),
        ]);
        assert_eq!(base_of(&mgr, "mod.T"), None);
    }

    #[test]
    fn component_for_template_handles_the_ordinary_and_degenerate_cases() {
        let mgr = indexed(&[("Base", None, None), ("Leaf", Some("Base"), Some("mod.T"))]);
        assert_eq!(base_of(&mgr, "mod.T"), Some("Leaf"));
        assert_eq!(base_of(&mgr, "mod.Unknown"), None);
    }

    #[test]
    fn forgetting_a_file_drops_its_classes_from_every_lookup() {
        let mut mgr = indexed(&[("Base", None, Some("mod.T")), ("Leaf", Some("Base"), Some("mod.T"))]);
        mgr.forget_file("Leaf.js");

        assert!(mgr.get_component("Leaf.js", "Leaf").is_none());
        assert_eq!(mgr.components_in_file("Leaf.js").count(), 0);
        assert_eq!(mgr.components().count(), 1);
        // A dead key still sitting in `by_template` must not be a candidate.
        assert_eq!(base_of(&mgr, "mod.T"), Some("Base"));

        mgr.forget_file("Base.js");
        assert_eq!(base_of(&mgr, "mod.T"), None);
    }

    #[test]
    fn reindexing_a_file_drops_its_old_class_names() {
        // The rename-while-typing case: a half-typed name must not outlive the next build.
        let mut mgr = ComponentMgr::default();
        mgr.index_file("widget.js", vec![desc("widget.js", "MyWidget", None, Some("mod.T"))]);
        mgr.index_file("widget.js", vec![desc("widget.js", "MyPanel", None, Some("mod.T"))]);

        assert!(mgr.get_component("widget.js", "MyWidget").is_none());
        assert_eq!(mgr.components().count(), 1);
        assert_eq!(base_of(&mgr, "mod.T"), Some("MyPanel"));
    }

    #[test]
    fn two_files_declaring_the_same_class_name_both_survive() {
        let mut mgr = ComponentMgr::default();
        mgr.index_file("b.js", vec![desc("b.js", "SearchBar", None, None)]);
        mgr.index_file("a.js", vec![desc("a.js", "SearchBar", None, None)]);

        assert_eq!(mgr.components().count(), 2);
        assert!(mgr.get_component("a.js", "SearchBar").is_some());
        assert!(mgr.get_component("b.js", "SearchBar").is_some());

        // Forgetting one file leaves the other file's class of the same name alone.
        mgr.forget_file("a.js");
        assert!(mgr.get_component("a.js", "SearchBar").is_none());
        assert!(mgr.get_component("b.js", "SearchBar").is_some());
    }

    fn base_of<'a>(mgr: &'a ComponentMgr, template_name: &str) -> Option<&'a str> {
        mgr.component_key_for_template(template_name, &TestResolver)
            .map(|key| mgr.descriptors[key].class_name.as_str())
    }

    fn default_import_desc(file_path: &str, class: &str, specifier: &str) -> ComponentDescriptor {
        let mut result = desc(file_path, class, None, None);
        result.super_class = Some(SuperClassRef::Imported(ImportSource {
            specifier: specifier.to_string(),
            kind: JsImportKind::Default,
        }));
        result
    }

    fn super_name(mgr: &ComponentMgr, descriptor: &ComponentDescriptor) -> Option<String> {
        mgr.super_key(descriptor, &TestResolver)
            .map(|key| mgr.descriptors[key].class_name.clone())
    }

    fn key(mgr: &ComponentMgr, class: &str) -> ComponentKey {
        mgr.key_by_file_and_name(&format!("{class}.js"), class)
            .unwrap_or_else(|| panic!("{class} should be indexed"))
    }

    fn subclass_names(mgr: &ComponentMgr, file_paths: &[&str]) -> Vec<String> {
        let file_paths: Vec<String> = file_paths.iter().map(|p| p.to_string()).collect();
        let mut names: Vec<String> = mgr
            .subclass_keys_of_files(&file_paths, &TestResolver)
            .into_iter()
            .map(|key| mgr.descriptors[key].class_name.clone())
            .collect();
        names.sort();
        names
    }
}
