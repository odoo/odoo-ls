from .symbol import Symbol

class Model():

    def __init__(self, name, symbol):
        self.name = name
        self.impl_sym = [symbol]
    
    def get_main_symbol(self):
        return self.impl_sym[0]
