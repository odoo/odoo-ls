def a(x, /, y):
    pass

a(1, 2)
a(1, y=2)
a(x=1, y=2) # OLS01011
a(1, x=2) # OLS01011

def b(x, y, /, z):
    pass

b(1, 2, 3)
b(1, 2, z=3)
b(1, y=2, z=3) # OLS01011
b(x=1, y=2, z=3) # OLS01011

def c(x, /, y, *, z):
    pass

c(1, 2, z=3)
c(1, y=2, z=3)
c(x=1, y=2, z=3) # OLS01011

def d(a, /, d):
    pass

d(**{"a": 5, "d": 6}) # OLS01007

def e(x, /, **kwargs):
    pass

e(1)
e(1, y=2)
e(x=1) # OLS01011
