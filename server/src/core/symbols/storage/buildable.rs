use duplicate::duplicate_item;

use crate::{constants::{BuildStatus, BuildSteps}, core::symbols::{CsvFileSymbol, FileSymbol, FunctionSymbol, JsFileSymbol, ModuleSymbol, PythonPackageSymbol, XmlFileSymbol}};

mod private_accessors {
    use crate::constants::{BuildStatus, BuildSteps};

    pub trait BuildableAccessors {
        fn _build_status(&self) -> BuildStatus;
        fn _build_status_mut(&mut self) -> &mut BuildStatus;
        fn _current_build_step_mut(&mut self) -> &mut BuildSteps;
        fn _current_build_step(&self) -> BuildSteps;
    }
}

#[duplicate_item(name; [ModuleSymbol]; [PythonPackageSymbol]; [FileSymbol]; [FunctionSymbol]; [XmlFileSymbol]; [CsvFileSymbol]; [JsFileSymbol])]
impl private_accessors::BuildableAccessors for name {
    fn _build_status(&self) -> BuildStatus { self.build_status }
    fn _build_status_mut(&mut self) -> &mut BuildStatus { &mut self.build_status }
    fn _current_build_step_mut(&mut self) -> &mut BuildSteps { &mut self.current_build_step }
    fn _current_build_step(&self) -> BuildSteps { self.current_build_step }
}


pub trait Buildable: private_accessors::BuildableAccessors {
    const STEPS: &'static [BuildSteps];
    /// Returns the current build step of the symbol
    fn get_current_build_step(&self) -> BuildSteps { self._current_build_step() }
    fn first_step(&self) -> BuildSteps { Self::STEPS[0] }
    /// Return if the given step is valid for the symbol
    fn is_step_valid(&self, step: BuildSteps) -> bool { Self::STEPS.contains(&step) }
    /// Returns the step that should be built before the given step, if any
    fn previous_build_step(&self, step: BuildSteps) -> Option<BuildSteps> {
        Self::STEPS.iter().rev().find(|&&s| s < step).copied()
    }
    /// Returns the step that should be built after the given step, if any
    fn next_build_step(&self, step: BuildSteps) -> Option<BuildSteps> {
        Self::STEPS.iter().find(|&&s| s > step).copied()
    }
    /// Returns the build status of the symbol for the given step
    fn get_build_status(&self, step: BuildSteps) -> BuildStatus {
        if self.get_current_build_step() > step {
            BuildStatus::DONE
        } else if self.get_current_build_step() == step {
            self._build_status()
        } else {
            BuildStatus::PENDING
        }
    }
    /// Sets the build status of the symbol for the given step. You can only set status for the current step or a future step only!
    /// If you want to reset the status to a previous step, you have to use SymbolTable::invalidate() instead
    fn set_build_status(&mut self, step: BuildSteps, status: BuildStatus) {
        if self.get_current_build_step() > step {
            if status != BuildStatus::DONE {
                panic!("A previous build step should not be changed to a non-DONE status. It should be done through invalidate_build_step() instead")
            }
            return;
        }
        while self.get_current_build_step() != step {
            if let Some(next_step) = self.next_build_step(self.get_current_build_step()) {
                *self._current_build_step_mut() = next_step;
            } else {
                panic!("Cannot set build status for a step that is not the current step or a future step");
            }
        }
        if status == BuildStatus::DONE {
            if let Some(next_step) = self.next_build_step(self.get_current_build_step()) {
                *self._current_build_step_mut() = next_step;
                *self._build_status_mut() = BuildStatus::PENDING;
            } else {
                *self._build_status_mut() = BuildStatus::DONE;
            }
        } else {
            *self._build_status_mut() = status;
        }
    }
}

pub(in crate::core::symbols) trait ResettableBuildable {
    /*
    /!\ SHOULD BE ALWAYS be called by 'invalidate' operation, as reset a step state should trigger dependencies rebuild
     */
    fn reset_build_status(&mut self, step: BuildSteps, status: BuildStatus);
}

#[duplicate_item(name;
    [ModuleSymbol];
    [PythonPackageSymbol];
    [FileSymbol];
    [FunctionSymbol];
    [XmlFileSymbol];
    [CsvFileSymbol];
    [JsFileSymbol])]
impl ResettableBuildable for name {
    fn reset_build_status(&mut self, step: BuildSteps, status: BuildStatus) {
        if step > self.current_build_step {
            return;
        }
        if !self.is_step_valid(step) {
            panic!("Cannot reset build status to an invalid step");
        }
        self.current_build_step = step;
        self.build_status = status;
    }
}

//arch - arch_eval - function_arch - validation
#[duplicate_item(name; [FunctionSymbol];)]
impl Buildable for name {
    const STEPS: &'static [BuildSteps] = &[BuildSteps::ARCH, BuildSteps::ARCH_EVAL, BuildSteps::ODOO_FUNCTION_AE, BuildSteps::VALIDATION];
}

//arch - arch_eval - validation
#[duplicate_item(name; [ModuleSymbol]; [PythonPackageSymbol]; [FileSymbol])]
impl Buildable for name {
    const STEPS: &'static [BuildSteps] = &[BuildSteps::ARCH, BuildSteps::ARCH_EVAL, BuildSteps::VALIDATION];
}

//only arch and validation
#[duplicate_item(name; [XmlFileSymbol]; [CsvFileSymbol])]
impl Buildable for name {
    const STEPS: &'static [BuildSteps] = &[BuildSteps::ARCH, BuildSteps::VALIDATION];
}

// only validation
impl Buildable for JsFileSymbol {
    const STEPS: &'static [BuildSteps] = &[BuildSteps::VALIDATION];
}
