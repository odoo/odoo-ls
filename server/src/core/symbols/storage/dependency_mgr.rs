use std::ops::{Index, IndexMut};

use crate::constants::{BuildStatus, BuildSteps};
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::core::symbols::{CsvFileSymbol, FileSymbol, FunctionSymbol, ModuleSymbol, PythonPackageSymbol, XmlFileSymbol};
use crate::weak_collections::WeakSet;

/// Triangular dependency table, indexed by target build step, then by required dep level.
/// `dependencies[step][level]` = set of files that must reach `level` before self can reach `step`.
#[derive(Debug, Clone, Default)]
pub struct DependenciesTable {
    // ARCH needs: [ARCH]
    pub(super) arch: [WeakSet<SourceFileKey>; 1],
    // ARCH_EVAL needs: [ARCH, ARCH_EVAL]
    pub(super) arch_eval: [WeakSet<SourceFileKey>; 2],
    // VALIDATION needs: [ARCH, ARCH_EVAL, VALIDATION]
    pub(super) validation: [WeakSet<SourceFileKey>; 3],
}

impl Index<usize> for DependenciesTable {
    type Output = [WeakSet<SourceFileKey>];
    fn index(&self, i: usize) -> &[WeakSet<SourceFileKey>] {
        match i {
            0 => &self.arch,
            1 => &self.arch_eval,
            2 => &self.validation,
            _ => panic!("DependenciesTable: invalid step index {}", i),
        }
    }
}

impl IndexMut<usize> for DependenciesTable {
    fn index_mut(&mut self, i: usize) -> &mut [WeakSet<SourceFileKey>] {
        match i {
            0 => &mut self.arch,
            1 => &mut self.arch_eval,
            2 => &mut self.validation,
            _ => panic!("DependenciesTable: invalid step index {}", i),
        }
    }
}

/// Triangular dependents table, indexed by current level, then by dependent step offset (step - level).
/// `dependents[level][offset]` = set of files whose step `level + offset` depends on self reaching `level`.
#[derive(Debug, Clone, Default)]
pub struct DependentsTable {
    // At ARCH: dependent steps are ARCH, ARCH_EVAL, VALIDATION (offsets 0, 1, 2)
    pub(super) arch: [WeakSet<SourceFileKey>; 3],
    // At ARCH_EVAL: dependent steps are ARCH_EVAL, VALIDATION (offsets 0, 1)
    pub(super) arch_eval: [WeakSet<SourceFileKey>; 2],
    // At VALIDATION: dependent step is VALIDATION (offset 0)
    pub(super) validation: [WeakSet<SourceFileKey>; 1],
}

impl Index<usize> for DependentsTable {
    type Output = [WeakSet<SourceFileKey>];
    fn index(&self, i: usize) -> &[WeakSet<SourceFileKey>] {
        match i {
            0 => &self.arch,
            1 => &self.arch_eval,
            2 => &self.validation,
            _ => panic!("DependentsTable: invalid level index {}", i),
        }
    }
}

impl IndexMut<usize> for DependentsTable {
    fn index_mut(&mut self, i: usize) -> &mut [WeakSet<SourceFileKey>] {
        match i {
            0 => &mut self.arch,
            1 => &mut self.arch_eval,
            2 => &mut self.validation,
            _ => panic!("DependentsTable: invalid level index {}", i),
        }
    }
}

pub trait Dependencies {
    fn dependencies(&self) -> &DependenciesTable;
    fn dependents(&self) -> &DependentsTable;
    fn is_in_workspace(&self) -> bool;
    fn set_in_workspace(&mut self, in_workspace: bool);

    fn get_all_dependencies(&self, step: usize) -> &[WeakSet<SourceFileKey>] {
        &self.dependencies()[step]
    }

    fn get_dependents(&self, level: usize, offset: usize) -> &WeakSet<SourceFileKey> {
        &self.dependents()[level][offset]
    }

    fn get_all_dependents(&self, level: usize) -> &[WeakSet<SourceFileKey>] {
        &self.dependents()[level]
    }
}

macro_rules! impl_dependencies {
    ($($t:ty),* $(,)?) => { $(
        impl Dependencies for $t {
            fn dependencies(&self) -> &DependenciesTable { &self.dependencies }
            fn dependents(&self) -> &DependentsTable { &self.dependents }
            fn is_in_workspace(&self) -> bool { self.in_workspace }

            fn set_in_workspace(&mut self, in_workspace: bool) {
                self.in_workspace = in_workspace;
                if !in_workspace { return; }
                self.dependencies = DependenciesTable::default();
                self.dependents = DependentsTable::default();
            }
        }
    )* }
}

impl_dependencies!(
    FileSymbol,
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
