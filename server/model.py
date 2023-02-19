from .symbol import Symbol

class Model():

    def __init__(self, name):
        self.name = name
        self.inherit = [str]
        self.inherited_by = [str]
        self.impl_sym = [Symbol]