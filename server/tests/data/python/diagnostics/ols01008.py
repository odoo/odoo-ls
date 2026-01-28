def a():
    pass

a()
a(d=5) # OLS01008
a(a=3) # OLS01008

def b(x, y, *, z):
    pass

b(1, 2, z=3)
b(1, 2, z=3, z2=5) # OLS01008
b(1, 2, z2=5, z=3) # OLS01008
b(1, 2, z2=3) # OLS01008, OLS01010

def c(*, x):
    pass

c(x=10)
c(y=5, x=10) # OLS01008
c(x=10, y=5) # OLS01008

def d(a, *args):
    pass

d(5)
d(5, d=3) # OLS01008
d(5, d=3, e=5) #OLS01008

def e(a, **kwargs):
    pass

e(5)
e(5, d=3)
e(6, d=5, e=6, f=7)

def e(a, *args, **kwargs):
    pass

e(5)
e(5, 6, 7, d=3)
e(6, 6, 7, d=5, e=6, f=7)