class Inference():

    def __init__(self, name, symbol, lineno, instance=False):
        self.name = name
        self.symbol = symbol
        self.lineno = lineno
        self.instance = instance
    
    def __str__(self):
        return f"{self.name} - {self.lineno}"

class Inferencer():

    def __init__(self):
        self.inferences = []
    
    def inferName(self, name, line):
        selected = False
        for infer in self.inferences:
            if infer.name == name and infer.lineno < line and (not selected or infer.lineno > selected.lineno):
                selected = infer
        return selected
    
    @staticmethod
    def inferNameInScope(name, line, scope_symbol):
        """try to resolve a name in the scope of a symbol. If the name is not found, try to resolve it in the scope of the parent symbol, and so on."""
        sym = scope_symbol
        infer = sym.inferencer.inferName(name, line)
        while sym and not infer and sym.type not in ["file", "package", "ext_package"]:
            sym = sym.parent
            infer = sym.inferencer.inferName(name, line)
        return infer

    def addInference(self, inference):
        self.inferences.append(inference)