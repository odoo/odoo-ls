use crate::{constants::{BuildStatus, BuildSteps}, core::symbols::{FileSymbol, FunctionSymbol, ModuleSymbol, PythonPackageSymbol}};

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
