def a(*, x):
    pass

a() # OLS01010
a(x=5)
a(y=5) # OLS01010, OLS01008

def b(*, x=5):
    pass

b()
b(x=6)
b(y=7) # OLS01008

def c(*, x, y, z):
    pass

c(z=5) # OLS01010
c(x=5, y=6, z=7)
c(x=5, y=6, z=7, w=8) # OLS01008
c(z=5, y=5, x=5)
c(y=5, z=5) # OLS01010
c(x=5, z=5) # OLS01010
c(x=5, y=5) # OLS01010