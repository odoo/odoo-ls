from odoo import api, fields, models

class ModelA(models.Model):

    _name = "self_propagation_module.modela"

    ch_field = fields.Char()

    def create_new(self): # -> Self@ModelA
        return super().create([])

    def create_from_self_env(self): # -> ModelA
        return self.env['self_propagation_module.modela'].create([])

    def search_in_a(self): # -> Self@ModelA
        # Check that env to with_user is properly propagated to be Self.
        return self.env['self_propagation_module.modela'].with_user(self.internal_user).search(
        [('ch_field', '=', "dummy_str")], # no diagnostic
        limit=1)

class ModelB(models.Model):

    _name = "self_propagation_module.modelb"
    _inherit = "self_propagation_module.modela"

    def method(self):
        # Test self propagation in the presence of inheritance and multiple calls to self methods
        self.create_new() # create_new is from ModelA, returns an instance of ModelB
        self.create_from_self_env() # create_from_self_env is from ModelA, returns an instance of ModelA, because it uses self.env['self_propagation_module.modela']

class A:
    def method_self(self):
        return self
    def method_hard_a(self):
        return A()

class B(A):
    def method_b_self(self): # -> Self@B
        return self.method_self() # method_self is from A, returns an instance of B since it's called on B

    def method_b_hard_a(self): # -> A
        return self.method_hard_a() # method_hard_a is from A, returns an instance of A