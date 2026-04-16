use tracing::debug;
use crate::core::symbols::SymbolTable;

macro_rules! log_symbol_table {
    ($st:ident, $method:ident; $($field:ident),* $(,)?) => {{
        $(debug!("{}: {}", stringify!($field), $st.$field.$method());)*
    }};
}

pub fn log_symbol_counts(symbol_table: &SymbolTable) {
    debug!("=== Symbol counts ===");
    log_symbol_table!(symbol_table, len; roots, disk_dirs, namespaces, python_packages, modules, files, compiled, classes, functions, variables, xml_files, csv_files);
}

pub fn log_slotmap_capacities(symbol_table: &SymbolTable) {
    debug!("=== Slotmap capacities ===");
    log_symbol_table!(symbol_table, capacity; roots, disk_dirs, namespaces, python_packages, modules, files, compiled, classes, functions, variables, xml_files, csv_files);
}

pub fn log_memory_usage() {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        debug!("===Memory usage===");
        for line in status.lines() {
            if line.starts_with("VmRSS:") || line.starts_with("VmPeak:") || line.starts_with("VmHWM:") {
                debug!("{}", line);
            }
        }
    }
}

pub fn log_is_in_workspace(symbol_table: &SymbolTable) {
    debug!("=== Symbols in workspace ===");
    let total_files = symbol_table.files.len();
    let in_workspace_files = symbol_table.files.values().filter(|f| f.in_workspace).count();
    debug!("Files in workspace: {}/{}", in_workspace_files, total_files);

    let total_packages = symbol_table.python_packages.len();
    let in_workspace_packages = symbol_table.python_packages.values().filter(|p| p.in_workspace).count();
    debug!("Python packages in workspace: {}/{}", in_workspace_packages, total_packages);

    let total_modules = symbol_table.modules.len();
    let in_workspace_modules = symbol_table.modules.values().filter(|m| m.in_workspace).count();
    debug!("Modules in workspace: {}/{}", in_workspace_modules, total_modules);

    let total_xml_files = symbol_table.xml_files.len();
    let in_workspace_xml_files = symbol_table.xml_files.values().filter(|f| f.in_workspace).count();
    debug!("XML files in workspace: {}/{}", in_workspace_xml_files, total_xml_files);

    let total_csv_files = symbol_table.csv_files.len();
    let in_workspace_csv_files = symbol_table.csv_files.values().filter(|f| f.in_workspace).count();
    debug!("CSV files in workspace: {}/{}", in_workspace_csv_files, total_csv_files);
}
