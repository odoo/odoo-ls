class Inference():

    def __init__(self, name, symbol, lineno, instance=False):
        self.name = name
        self.symbol = symbol
        self.lineno = lineno
        self.instance = instance

class Inferencer():

    def __init__(self):
        self.inferences = []
    
    def inferName(self, name, line):
        selected = False
        for infer in self.inferences:
            if infer.name == name and (not selected or (infer.lineno > selected.lineno and infer.lineno < line)):
                selected = infer
        return selected

    def addInference(self, inference):
        self.inferences.append(inference)