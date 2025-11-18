def a():
    pass

a()
a(5) #OLS01007

def b(value):
    pass

b() # OLS01007
b(10)

def c(x = 7):
    pass

c()
c(5)
c(5, 9) # OLS01007

def d(x, y=0):
    pass

d() # OLS01007
d(5)
d(5, 6)
d(5, 6, 7) # OLS01007

def e(*args):
    pass

e()
e(1)
e(1, 2, 3)

def f(x, *args):
    pass

f() # OLS01007
f(1)
f(1, 2)
f(1, 2, 3)

def g(x, y, *args):
    pass

g() # OLS01007
g(1) # OLS01007
g(1, 2)
g(1, 2, 3)
g(1, 2, 3, 4)

def h(x, y=10, *args):
    pass

h() # OLS01007
h(1)
h(1, 2)
h(1, 2, 3)

def i(x, y, z=0):
    pass

i() # OLS01007
i(1) # OLS01007
i(1, 2)
i(1, 2, 3)
i(1, 2, 3, 4) # OLS01007

# Position-only parameters (Python 3.8+)
def j(x, y, /):
    pass

j(1, 2)
j(1) # OLS01007
j(x=1, y=2) # OLS01007
j(1, 2, 3) # OLS01007

def k(x, /, y, z=0):
    pass

k(1, 2)
k(1, 2, 3)
k(1) # OLS01007
k(1, 2, 3, 4) # OLS01007
k(x=1, y=2) # OLS01007

# Keyword-only parameters (after a bare *)
def l(*, x):
    pass

l(x=1)
l() # OLS01007
l(1) # OLS01007

def m(a, *, x, y=3):
    pass

m(1, x=2)
m(1, x=2, y=4)
m(1) # OLS01007
m(1, 2) # OLS01007

# Mixed positional-only + regular + keyword-only
def n(a, b, /, c, *, d, e=0):
    pass

n(1, 2, 3, d=4)
n(1, 2, 3, d=4, e=5)
n(1, 2, c=3, d=4)
n(a=1, b=2, c=3, d=4) # OLS01007 (a/b positional-only)
n(1, 2) # OLS01007
n(1, 2, 3) # OLS01007 (missing keyword-only d)
n(1, 2, 3, 4, d=5) # OLS01007 (too many positional arguments)

# Functions mixing defaults and varargs + keywords
def o(a, b=1, *args, c, d=0):
    pass

o(1, c=2)
o(1, 2, c=3)
o(1, 2, 3, 4, c=5)
o() # OLS01007
o(1) # OLS01007 (missing c)
o(1, 2, 3, d=5) # OLS01007 (missing c)
o(1, 2, c=3, 5) # OLS01000 (positional after keyword)

# Fully variadic keyword-only
def p(**kwargs):
    pass

p()
p(x=1, y=2)

# Positional + keyword-only interaction
def q(a, b, *, x, y=10):
    pass

q(1, 2, x=3)
q(1, 2, x=3, y=4)
q(1, 2) # OLS01007
q(1, 2, 3) # OLS01007
q(a=1, b=2, x=3) # valid
q(a=1, b=2, 3) # OLS01000 (positional after keyword)