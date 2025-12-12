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

the_answer = a.answer  # the_answer: int
the_answer2 = d.answer # the_answer2: int

ambiguous_answer = a.ambiguous_answer  # ambiguous_answer: (int | str)
ambiguous_answer2 = d.ambiguous_answer # ambiguous_answer2: (int | str)

