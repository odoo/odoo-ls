class TestClass:
    @property
    def answer(self):
        return 42
    
    @property
    def ambiguous_answer(self):
        return 42 if True else "forty-two"

a = TestClass()     # a: TestClass
b = a               # b: TestClass
if True:
    b = 1           # b: int
b                   # b: (int | TestClass)
c = b               # c: (int | TestClass)
if True:
    c = "hi"        # c: str
c                   # c: (str | int | TestClass)
d = c               # d: (str | int | TestClass)

