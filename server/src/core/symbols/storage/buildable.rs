use duplicate::duplicate_item;

use crate::{constants::{BuildStatus, BuildSteps}, core::symbols::{CsvFileSymbol, FileSymbol, FunctionSymbol, JsFileSymbol, ModuleSymbol, PythonPackageSymbol, XmlFileSymbol}};


pub trait Buildable {
    /// Returns the current build step of the symbol
    fn get_current_build_step(&self) -> BuildSteps;
    fn first_step(&self) -> BuildSteps;
    /// Return if the given step is valid for the symbol
    fn is_step_valid(&self, step: BuildSteps) -> bool;
    /// Returns the step that should be built before the given step, if any
    fn previous_build_step(&self, step: BuildSteps) -> Option<BuildSteps>;
    /// Returns the step that should be built after the given step, if any
    fn next_build_step(&self, step: BuildSteps) -> Option<BuildSteps>;
    /// Returns the build status of the symbol for the given step
    fn get_build_status(&self, step: BuildSteps) -> BuildStatus;
    /// Sets the build status of the symbol for the given step. You can only set status for the current step or a future step only!
    /// If you want to reset the status to a previous step, you have to use SymbolTable::invalidate() instead
    fn set_build_status(&mut self, step: BuildSteps, status: BuildStatus);
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

//arch - arch_eval - validation
#[duplicate_item(name; [ModuleSymbol]; [PythonPackageSymbol]; [FileSymbol]; [FunctionSymbol])]
impl Buildable for name {
    fn get_current_build_step(&self) -> BuildSteps {
        self.current_build_step
    }
    fn first_step(&self) -> BuildSteps {
        BuildSteps::ARCH
    }
    fn is_step_valid(&self, step: BuildSteps) -> bool {
        match step {
            BuildSteps::ARCH => true,
            BuildSteps::ARCH_EVAL => true,
            BuildSteps::VALIDATION => true,
        }
    }
    fn previous_build_step(&self, step: BuildSteps) -> Option<BuildSteps> {
        match step {
            BuildSteps::ARCH => None,
            BuildSteps::ARCH_EVAL => Some(BuildSteps::ARCH),
            BuildSteps::VALIDATION => Some(BuildSteps::ARCH_EVAL),
        }
    }
    fn next_build_step(&self, step: BuildSteps) -> Option<BuildSteps> {
        match step {
            BuildSteps::ARCH => Some(BuildSteps::ARCH_EVAL),
            BuildSteps::ARCH_EVAL => Some(BuildSteps::VALIDATION),
            BuildSteps::VALIDATION => None,
        }
    }
    fn get_build_status(&self, step: BuildSteps) -> BuildStatus {
        if self.current_build_step > step {
            BuildStatus::DONE
        } else if self.current_build_step == step {
            self.build_status
        } else {
            BuildStatus::PENDING
        }
    }
    fn set_build_status(&mut self, step: BuildSteps, status: BuildStatus) {
        if self.current_build_step > step {
            if status != BuildStatus::DONE {
                panic!("A previous build step should not be changed to a non-DONE status. It should be done through invalidate_build_step() instead")
            }
            return;
        }
        while self.current_build_step != step {
            if let Some(next_step) = self.next_build_step(self.current_build_step) {
                self.current_build_step = next_step;
            } else {
                panic!("Cannot set build status for a step that is not the current step or a future step");
            }
        }
        if status == BuildStatus::DONE {
            if let Some(next_step) = self.next_build_step(self.current_build_step) {
                self.current_build_step = next_step;
                self.build_status = BuildStatus::PENDING;
            } else {
                self.build_status = BuildStatus::DONE;
            }
        } else {
            self.build_status = status;
        }
    }
}

//only arch and validation
#[duplicate_item(name; [XmlFileSymbol]; [CsvFileSymbol])]
impl Buildable for name {
    fn get_current_build_step(&self) -> BuildSteps {
        self.current_build_step
    }
    fn first_step(&self) -> BuildSteps {
        BuildSteps::ARCH
    }
    fn is_step_valid(&self, step: BuildSteps) -> bool {
        match step {
            BuildSteps::ARCH => true,
            BuildSteps::ARCH_EVAL => false,
            BuildSteps::VALIDATION => true,
        }
    }
    fn previous_build_step(&self, step: BuildSteps) -> Option<BuildSteps> {
        match step {
            BuildSteps::ARCH => None,
            BuildSteps::ARCH_EVAL => None,
            BuildSteps::VALIDATION => Some(BuildSteps::ARCH),
        }
    }
    fn next_build_step(&self, step: BuildSteps) -> Option<BuildSteps> {
        match step {
            BuildSteps::ARCH => Some(BuildSteps::VALIDATION),
            BuildSteps::ARCH_EVAL => None,
            BuildSteps::VALIDATION => None,
        }
    }
    fn get_build_status(&self, step: BuildSteps) -> BuildStatus {
        if self.current_build_step > step {
            BuildStatus::DONE
        } else if self.current_build_step == step {
            self.build_status
        } else {
            BuildStatus::PENDING
        }
    }
    fn set_build_status(&mut self, step: BuildSteps, status: BuildStatus) {
        if self.current_build_step > step {
            if status != BuildStatus::DONE {
                panic!("A previous build step should not be changed to a non-DONE status. It should be done through invalidate_build_step() instead")
            }
            return;
        }
        while self.current_build_step != step {
            if let Some(next_step) = self.next_build_step(self.current_build_step) {
                self.current_build_step = next_step;
            } else {
                panic!("Cannot set build status for a step that is not the current step or a future step");
            }
        }
        if status == BuildStatus::DONE {
            if let Some(next_step) = self.next_build_step(self.current_build_step) {
                self.current_build_step = next_step;
                self.build_status = BuildStatus::PENDING;
            } else {
                self.build_status = BuildStatus::DONE;
            }
        } else {
            self.build_status = status;
        }
    }
}

// only validation
impl Buildable for JsFileSymbol {
    fn get_current_build_step(&self) -> BuildSteps {
        self.current_build_step
    }
    fn first_step(&self) -> BuildSteps {
        BuildSteps::VALIDATION
    }
    fn is_step_valid(&self, step: BuildSteps) -> bool {
        match step {
            BuildSteps::ARCH => false,
            BuildSteps::ARCH_EVAL => false,
            BuildSteps::VALIDATION => true,
        }
    }
    fn previous_build_step(&self, _step: BuildSteps) -> Option<BuildSteps> {
        None
    }
    fn next_build_step(&self, _step: BuildSteps) -> Option<BuildSteps> {
        None
    }
    fn get_build_status(&self, step: BuildSteps) -> BuildStatus {
        if self.current_build_step > step {
            BuildStatus::DONE
        } else if self.current_build_step == step {
            self.build_status
        } else {
            BuildStatus::PENDING
        }
    }
    fn set_build_status(&mut self, step: BuildSteps, status: BuildStatus) {
        if self.current_build_step > step {
            if status != BuildStatus::DONE {
                panic!("A previous build step should not be changed to a non-DONE status. It should be done through invalidate_build_step() instead")
            }
            return;
        }
        while self.current_build_step != step {
            if let Some(next_step) = self.next_build_step(self.current_build_step) {
                self.current_build_step = next_step;
            } else {
                panic!("Cannot set build status for a step that is not the current step or a future step");
            }
        }
        if status == BuildStatus::DONE {
            if let Some(next_step) = self.next_build_step(self.current_build_step) {
                self.current_build_step = next_step;
                self.build_status = BuildStatus::PENDING;
            } else {
                self.build_status = BuildStatus::DONE;
            }
        } else {
            self.build_status = status;
        }
    }
}
