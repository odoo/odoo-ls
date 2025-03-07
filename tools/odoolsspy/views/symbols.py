

class Symbol():

    def __init__(self, name, value):
        self.name = name
        self.value = value

    def __str__(self):
        return f"{self.name} = {self.value}"
    
    def display(self, parent: int | str):
        pass