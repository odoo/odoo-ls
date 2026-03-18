
def a():
    pass

def b(self):
    pass

def c(x):
    pass

def f(y=0):
    pass

class Test():

    def a(self):
        pass

    def b(other_name):
        pass

    @staticmethod
    def c():
        pass

    @classmethod
    def d(cls):
        pass

    @classmethod
    def e(): # OLS01004
        pass

    def oups(): # OLS01004
        pass