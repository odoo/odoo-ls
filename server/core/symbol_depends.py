from ..references import RegisteredRefSet
from ..constants import BuildSteps

class DependencyArch():
    arch = None

    def get_arch(self):
        if not self.arch:
            self.arch = RegisteredRefSet()
        return self.arch

    def __getitem__(self, level):
        if level == BuildSteps.ARCH:
            return self.get_arch()
        else:
            raise KeyError("Invalid key: " + str(level) + " for DependencyArch")

    def existing_items(self):
        if self.arch:
            yield BuildSteps.ARCH, self.arch


class DependencyOdoo():
    arch = None
    arch_eval = None
    odoo = None

    def get_arch(self):
        if not self.arch:
            self.arch = RegisteredRefSet()
        return self.arch

    def get_arch_eval(self):
        if not self.arch_eval:
            self.arch_eval = RegisteredRefSet()
        return self.arch_eval

    def get_odoo(self):
        if not self.odoo:
            self.odoo = RegisteredRefSet()
        return self.odoo

    def existing_items(self):
        if self.arch:
            yield BuildSteps.ARCH, self.arch
        if self.arch_eval:
            yield BuildSteps.ARCH_EVAL, self.arch_eval
        if self.odoo:
            yield BuildSteps.ODOO, self.odoo

    def __getitem__(self, level):
        if level == BuildSteps.ARCH:
            return self.get_arch()
        elif level == BuildSteps.ARCH_EVAL:
            return self.get_arch_eval()
        elif level == BuildSteps.ODOO:
            return self.get_odoo()
        else:
            raise KeyError("Invalid key: " + str(level) + " for DependencyOdoo")


class Dependencies():
    arch = DependencyArch() #symbols needed to build arch of this symbol
    arch_eval = DependencyArch()
    odoo = DependencyOdoo()
    validation = DependencyOdoo()

    def __getitem__(self, level):
        if level == BuildSteps.ARCH:
            return self.arch
        elif level == BuildSteps.ARCH_EVAL:
            return self.arch_eval
        elif level == BuildSteps.ODOO:
            return self.odoo
        elif level == BuildSteps.VALIDATION:
            return self.validation


class DependentsArch():
    arch = None
    arch_eval = None
    odoo = None
    validation = None

    def get_arch(self):
        if not self.arch:
            self.arch = RegisteredRefSet()
        return self.arch

    def get_arch_eval(self):
        if not self.arch_eval:
            self.arch_eval = RegisteredRefSet()
        return self.arch_eval

    def get_odoo(self):
        if not self.odoo:
            self.odoo = RegisteredRefSet()
        return self.odoo

    def get_validation(self):
        if not self.validation:
            self.validation = RegisteredRefSet()
        return self.validation

    def existing_items(self):
        if self.arch:
            yield BuildSteps.ARCH, self.arch
        if self.arch_eval:
            yield BuildSteps.ARCH_EVAL, self.arch_eval
        if self.odoo:
            yield BuildSteps.ODOO, self.odoo
        if self.validation:
            yield BuildSteps.VALIDATION, self.validation

    def __getitem__(self, level):
        if level == BuildSteps.ARCH:
            return self.get_arch()
        elif level == BuildSteps.ARCH_EVAL:
            return self.get_arch_eval()
        elif level == BuildSteps.ODOO:
            return self.get_odoo()
        elif level == BuildSteps.VALIDATION:
            return self.get_validation()


class DependentsArchEval():
    odoo = None
    validation = None

    def get_odoo(self):
        if not self.odoo:
            self.odoo = RegisteredRefSet()
        return self.odoo

    def get_validation(self):
        if not self.validation:
            self.validation = RegisteredRefSet()
        return self.validation

    def existing_items(self):
        if self.odoo:
            yield BuildSteps.ODOO, self.odoo
        if self.validation:
            yield BuildSteps.VALIDATION, self.validation

    def __getitem__(self, level):
        if level == BuildSteps.ODOO:
            return self.get_odoo()
        elif level == BuildSteps.VALIDATION:
            return self.get_validation()
        else:
            raise KeyError("Invalid key: " + str(level) + " for DependentsArchEval")


class Dependents():
    arch = DependentsArch() #set of symbol that need to be rebuilt when this symbol is modified at arch level
    arch_eval = DependentsArchEval()
    odoo = DependentsArchEval()

    def __getitem__(self, level):
        if level == BuildSteps.ARCH:
            return self.arch
        elif level == BuildSteps.ARCH_EVAL:
            return self.arch_eval
        elif level == BuildSteps.ODOO:
            return self.odoo
        else:
            raise KeyError("Invalid key: " + str(level) + " for Dependents")
