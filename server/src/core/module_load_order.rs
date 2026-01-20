use std::collections::HashMap;

pub struct SortResult<'a> {
    pub sorted: Vec<&'a str>,
    pub invalid: Vec<&'a str>, // due to missing direct/indirect dependencies or dependency cycle
    pub missing: Vec<&'a str>,
    pub cycles: Vec<Vec<&'a str>>,
}

/// Sort modules based on Odoo module load order logic.
/// 
/// Input: Vec of (module_name, dependencies)
/// 
/// Output: SortResult containing sorted valid modules and invalid modules
/// 
/// Note: 'base' module is expected to be included in the input modules.
pub fn sort_by_load_order<'a>(modules: Vec<(&'a str, Vec<&'a str>)>) -> SortResult<'a> {
    let mut graph = Graph::from(modules);
    let (sorted, invalid) = graph.get_load_order();
    let mut missing = Vec::new();
    let mut cycles = Vec::new();
    for error in graph.errors {
        match error {
            ValidationError::MissingModule(m) => missing.push(m),
            ValidationError::DependencyCycle(cycle) => cycles.push(cycle),
        }
    }
    SortResult {
        sorted,
        invalid,
        missing,
        cycles,
    }
}

/// Replicates (roughly) the module load order logic from the ModuleGraph in Odoo framework
struct Graph<'a> {
    modules_to_dependencies: HashMap<&'a str, Vec<&'a str>>, // keys: nodes, v: edges from that node
    errors: Vec<ValidationError<'a>>,
    // Caches for memoization
    validation_cache: HashMap<&'a str, bool>,
    depth_cache: HashMap<&'a str, usize>,
    order_name_cache: HashMap<&'a str, String>,
}

enum ValidationError<'a> {
    MissingModule(&'a str),
    DependencyCycle(Vec<&'a str>),
}

/// Simple macro to cache some method calls in Graph.
/// Limitation: $compute block cannot have return statements.
macro_rules! cached {
    ($cache:expr, $key:expr, $compute:block) => {{
        if let Some(value) = $cache.get($key) {
            value.clone() // this is a simple copy for &str and usize
        } else {
            let value = $compute;
            $cache.insert($key, value.clone());
            value
        }
    }};
}

impl<'a> Graph<'a> {
    fn from(nodes: Vec<(&'a str, Vec<&'a str>)>) -> Self {
        let mut graph = Self {
            modules_to_dependencies: nodes.into_iter().collect(),
            validation_cache: HashMap::new(),
            depth_cache: HashMap::new(),
            order_name_cache: HashMap::new(),
            errors: Vec::new(),
        };
        graph.add_base_dependency();
        graph
    }

    /// Returns (sorted valid modules, invalid modules)
    fn get_load_order(&mut self) -> (Vec<&'a str>, Vec<&'a str>) {
        self.errors.clear();
        // sort modules between valid and invalid
        let modules: Vec<_> = self.modules_to_dependencies.keys().cloned().collect();
        let mut valid_modules = vec![];
        let mut invalid_modules = vec![];
        for module in modules {
            if self.is_valid_module(module, &mut Vec::new()) {
                valid_modules.push(module);
            } else {
                invalid_modules.push(module);
            }
        }
        // sort valid modules by load order
        valid_modules.sort_by_cached_key(|m| self.get_sort_key(m));
        // sort invalid modules lexicographically (avoid hashmap indeterministic order)
        invalid_modules.sort();
        (valid_modules, invalid_modules)
    }

    // ====== Setup ==========

    // `depends` = [] in the manifest implies a dependency on 'base'
    // See _load_manifest @ module.py in Odoo
    fn add_base_dependency(&mut self) {
        for (module, dependencies) in self.modules_to_dependencies.iter_mut() {
            if *module != "base" && dependencies.is_empty() {
                dependencies.push("base");
            }
        }
    }

    // ====== Validation ==========

    fn is_valid_module(&mut self, name: &'a str, recursion_stack: &mut Vec<&'a str>) -> bool {
        // Cache key is just `name` (not recursion_stack) because:
        // - recursion_stack is only for cycle detection during traversal
        // - if a cycle is detected, the module is cached as `false` from the inner recursive call
        // - once cached, the result is valid for all future lookups regardless of traversal path
        // Note: the block for cached! macro cannot contain return statements (therefore the indirection)
        cached!(self.validation_cache, name, {
            self._is_valid_module(name, recursion_stack)
        })
    }

    /// A module is valid when:
    /// - it exists
    /// - is not part of a dependency cycle
    /// - all its dependencies are valid modules
    ///
    /// This implementation is different from the Odoo python framework (see
    /// ModuleGraph._update_depends and _update_depth)
    fn _is_valid_module(&mut self, name: &'a str, recursion_stack: &mut Vec<&'a str>) -> bool {
        if recursion_stack.contains(&name) {
            // dependency cycle detected
            self.errors
                .push(ValidationError::new_dep_cycle_error(recursion_stack));
            return false;
        }
        let Some(dependencies) = self.modules_to_dependencies.get(name).cloned() else {
            // module does not exist
            self.errors.push(ValidationError::MissingModule(name));
            return false;
        };
        recursion_stack.push(name);
        let is_valid = dependencies
            .iter()
            .all(|&dep| self.is_valid_module(dep, recursion_stack));
        recursion_stack.pop();
        is_valid
    }

    // ===== Sort algo (topological sort) =======

    /// Sorting key for a module: max depth of its dependencies + 1, then
    /// lexicographical by module name.
    /// 
    /// test_* modules have a special rule: they load right after their last
    /// loaded dependency.
    fn get_sort_key(&mut self, module: &'a str) -> (usize, String) {
        let depth = self.get_depth(module);
        let order_name = self.get_order_name(module);
        (depth, order_name)
    }

    fn get_depth(&mut self, module: &'a str) -> usize {
        cached!(self.depth_cache, module, {
            let dependencies = self
                .modules_to_dependencies
                .get(module)
                .expect("module to exist")
                .clone();
            let deps_max_depth = dependencies.iter().map(|&dep| self.get_depth(dep)).max();
            match deps_max_depth {
                None => 0,                                   // empty dependencies (base module)
                Some(d) if module.starts_with("test_") => d, // test_ modules
                Some(d) => d + 1,                            // regular module
            }
        })
    }

    fn get_order_name(&mut self, module: &'a str) -> String {
        cached!(self.order_name_cache, module, {
            if module.starts_with("test_") {
                let last_loaded_dep = self
                    .get_last_loaded_dep(module)
                    .expect("test_ module to have at least 'base' as dependency");
                self.get_order_name(last_loaded_dep) + " " + module
            } else {
                return module.to_string();
            }
        })
    }

    fn get_last_loaded_dep(&mut self, module: &'a str) -> Option<&'a str> {
        let dependencies = self
            .modules_to_dependencies
            .get(module)
            .expect("module to exist")
            .clone();
        dependencies
            .into_iter()
            .max_by_key(|&d| self.get_sort_key(d))
    }
}

impl<'a> ValidationError<'a> {
    pub fn new_dep_cycle_error(recursion_stack: &[&'a str]) -> Self {
        let top = *recursion_stack.last().expect("non-empty recursion stack");
        let mut modules_in_cycle = vec![top];
        for &module in recursion_stack.iter().rev().skip(1) {
            if module == top {
                break;
            }
            modules_in_cycle.push(module);
        }
        modules_in_cycle.reverse();
        Self::DependencyCycle(modules_in_cycle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_linear_dependency() {
        // a -> b -> c -> base
        let nodes = vec![
            ("a", vec!["b"]),
            ("b", vec!["c"]),
            ("c", vec!["base"]),
            ("base", vec![]),
        ];
        let mut graph = Graph::from(nodes);
        let (order, _) = graph.get_load_order();
        // c must come before b, b before a
        assert_eq!(order, vec!["base", "c", "b", "a"]);
    }

    #[test]
    fn test_base_loads_first() {
        let nodes = vec![
            ("base", vec![]),
            ("a", vec![]), // implicitly depends on base
            ("b", vec!["base"]),
            ("c", vec!["a", "b"]),
        ];
        let mut graph = Graph::from(nodes);
        let (order, _) = graph.get_load_order();
        assert_eq!(order, vec!["base", "a", "b", "c"]);
    }

    #[test]
    fn test_multiple_independent_modules() {
        // base dependency is implicit
        let nodes = vec![
            ("a", vec![]),
            ("b", vec![]),
            ("c", vec![]),
            ("base", vec![]),
        ];
        let mut graph = Graph::from(nodes);
        let (order, _) = graph.get_load_order();
        assert_eq!(order, vec!["base", "a", "b", "c"]);
    }

    #[test]
    fn test_branching_dependencies() {
        let nodes = vec![
            ("a", vec!["b", "c"]),
            ("b", vec!["d"]),
            ("c", vec!["d"]),
            ("d", vec!["base"]),
            ("base", vec![]),
        ];
        let mut graph = Graph::from(nodes);
        let (order, _) = graph.get_load_order();
        assert_eq!(order, vec!["base", "d", "b", "c", "a"]);
    }

    #[test]
    fn test_test_modules() {
        let nodes = vec![
            ("base", vec![]),
            ("a", vec!["base"]),
            ("b", vec!["a"]),
            ("test_b", vec!["a", "b"]), // should load right after b, before c
            ("c", vec!["a", "b"]),
        ];
        let mut graph = Graph::from(nodes);
        let (order, _) = graph.get_load_order();
        assert_eq!(order, vec!["base", "a", "b", "test_b", "c"]);
    }

    #[test]
    fn test_nested_test_modules() {
        let nodes = vec![
            ("base", vec![]),
            ("a", vec!["base"]),
            ("b", vec!["a"]),
            ("test_a", vec!["a"]),      // should load right after a
            ("test_x", vec!["test_a"]), // should load right after test_a
        ];
        let mut graph = Graph::from(nodes);
        let (order, _) = graph.get_load_order();
        assert_eq!(order, vec!["base", "a", "test_a", "test_x", "b"]);
    }

    #[test]
    fn test_dependency_cycle_detection() {
        let nodes = vec![
            ("base", vec![]),
            ("a", vec!["b"]),
            ("b", vec!["c"]),
            ("c", vec!["a"]), // cycle here
            ("d", vec!["base"]),
        ];
        let mut graph = Graph::from(nodes);
        let (order, _) = graph.get_load_order();
        // a, b, c should be excluded due to cycle
        assert_eq!(order, vec!["base", "d"]);
        assert!(!graph.errors.is_empty());
        match &graph.errors[0] {
            ValidationError::DependencyCycle(cycle) => {
                // cycle should contain a, b, c (order might differ, due to hashmap iteration)
                let mut cycle = cycle.clone();
                cycle.sort();
                assert_eq!(cycle, vec!["a", "b", "c"]);
            }
            _ => panic!("Expected DependencyCycle error"),
        }
    }

    #[test]
    fn test_missing_module_detection() {
        let nodes = vec![
            ("base", vec![]),
            ("a", vec!["b"]), // b is missing
            ("c", vec!["base"]),
        ];
        let mut graph = Graph::from(nodes);
        let (order, _) = graph.get_load_order();
        // a should be excluded due to missing dependency
        assert_eq!(order, vec!["base", "c"]);
        assert!(!graph.errors.is_empty());
        match &graph.errors[0] {
            ValidationError::MissingModule(missing) => {
                assert_eq!(*missing, "b");
            }
            _ => panic!("Expected MissingModule error"),
        }
    }
}
