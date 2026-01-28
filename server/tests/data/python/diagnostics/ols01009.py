def b(*args):
    pass

b(6, d=5, e=5) #OLS01008

def a():
    pass

a()

def b(*args):
    pass

b(5)
b(5, d=5) #OLS01008
b(6, d=5, e=5) #OLS01008

if intput():
    def c(x, y):
        pass
else:
    def c(x):
        pass

c(5) #OLS01009


if intput():
    def d(x, y):
        pass
else:
    def d(x, *args):
        pass

d(5) #OLS01009
d(x=1, y=2) #OLS01009
d(1, 2)