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
    /// The base-most class in case of multiple classes.
    pub fn component_for_template(&self, session: &SessionInfo, template_name: &str) -> Option<&ComponentDescriptor> {
        self.component_key_for_template(template_name, session)
            .map(|key| &self.descriptors[key])
    }
 
    fn component_key_for_template(
        &self,
        template_name: &str,
        import_resolver: &impl ImportResolver,
    ) -> Option<ComponentKey> {
        let mut candidates = self.by_template.get(template_name)?.clone();
        // deterministic when no ancestry relates the declaring classes
        candidates.sort_by(|&a, &b| {
            let (a, b) = (&self.descriptors[a], &self.descriptors[b]);
            a.class_name.cmp(&b.class_name).then_with(|| a.file_path.cmp(&b.file_path))
        });
        let base = candidates.iter().position(|&candidate| {
            candidates.iter().all(|&other| {
                other == candidate || self.is_ancestor(candidate, other, import_resolver)
            })
        });
        candidates.get(base.unwrap_or(0)).copied()
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
