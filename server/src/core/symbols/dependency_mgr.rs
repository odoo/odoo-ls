use crate::constants::{BuildStatus, BuildSteps};
use crate::core::symbols::{csv_file_symbol::CsvFileSymbol, file_symbol::FileSymbol, function_symbol::FunctionSymbol, module_symbol::ModuleSymbol, namespace_symbol::NamespaceSymbol, package_symbol::PythonPackageSymbol, symbol_table::SymbolKey, xml_file_symbol::XmlFileSymbol};
use crate::weak_hash_set::WeakSet;

type DepSet = WeakSet<SymbolKey>;
// @arena: why an Option here?  An emtpy set should be enough no?
type DepLevel = Vec<Option<DepSet>>;
type DepTable = Vec<DepLevel>;

pub trait Dependencies {
    fn dependencies(&self) -> &DepTable;
    // @arena: probably not needed
    fn dependencies_mut(&mut self) -> &mut DepTable;
    fn dependents(&self) -> &DepTable;
    // @arena: probably not needed
    fn dependents_mut(&mut self) -> &mut DepTable;
    fn is_in_workspace(&self) -> bool;
    fn set_in_workspace(&mut self, in_workspace: bool);

    fn get_dependencies(&self, step: usize, level: usize) -> Option<&DepSet> {
        self.dependencies().get(step)?.get(level)?.as_ref()
    }

    fn get_all_dependencies(&self, step: usize) -> Option<&DepLevel> {
        self.dependencies().get(step)
    }

    fn get_dependents(&self, level: usize, step: usize) -> Option<&DepSet> {
        self.dependents().get(level)?.get(step)?.as_ref()
    }

    fn get_all_dependents(&self, level: usize) -> Option<&DepLevel> {
        self.dependents().get(level)
    }
}

macro_rules! impl_dependencies {
    ($($t:ty),* $(,)?) => { $(
        impl Dependencies for $t {
            fn dependencies(&self) -> &DepTable { &self.dependencies }
            fn dependencies_mut(&mut self) -> &mut DepTable { &mut self.dependencies }
            fn dependents(&self) -> &DepTable { &self.dependents }
            fn dependents_mut(&mut self) -> &mut DepTable { &mut self.dependents }
            fn is_in_workspace(&self) -> bool { self.in_workspace }

            fn set_in_workspace(&mut self, in_workspace: bool) {
                self.in_workspace = in_workspace;
                if !in_workspace { return; }
                self.dependencies = vec![
                    vec![ //ARCH
                        None //ARCH
                    ],
                    vec![ //ARCH_EVAL
                        None, //ARCH,
                        None, //ARCH_EVAL
                    ],
                    vec![
                        None, // ARCH
                        None, //ARCH_EVAL
                        None, //VALIDATIOn
                    ]
                ];
                self.dependents = vec![
                    vec![ //ARCH
                        None, //ARCH
                        None, //ARCH_EVAL
                        None, //VALIDATION
                    ],
                    vec![ //ARCH_EVAL
                        None, //ARCH_EVAL
                        None //VALIDATION
                    ],
                    vec![ //VALIDATION
                        None //VALIDATION
                    ]
                ];
            }
        }
    )* }
}

impl_dependencies!(
    FileSymbol,
    NamespaceSymbol,
    ModuleSymbol,
    PythonPackageSymbol,
    XmlFileSymbol,
    CsvFileSymbol,
);

pub trait Buildable {
    fn build_status(&self, step: BuildSteps) -> BuildStatus;
    fn set_build_status(&mut self, step: BuildSteps, status: BuildStatus);
}

macro_rules! impl_buildable {
    ($($t:ty),+ $(,)?) => {$(
        impl Buildable for $t {
            fn build_status(&self, step: BuildSteps) -> BuildStatus {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => self.arch_status,
                    BuildSteps::ARCH_EVAL => self.arch_eval_status,
                    BuildSteps::VALIDATION => self.validation_status,
                }
            }
            fn set_build_status(&mut self, step: BuildSteps, status: BuildStatus) {
                match step {
                    BuildSteps::SYNTAX => panic!(),
                    BuildSteps::ARCH => self.arch_status = status,
                    BuildSteps::ARCH_EVAL => self.arch_eval_status = status,
                    BuildSteps::VALIDATION => self.validation_status = status,
                }
            }
        }
    )+}
}

impl_buildable!(ModuleSymbol, PythonPackageSymbol, FileSymbol, FunctionSymbol);