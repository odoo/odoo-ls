
if True:
    a = 5
else:
    a = 6

if True:
    b = 5
else:
    b = 6
b = 7

c = 4
if True:
    c = 5
else:
    c = 6

d = 4
if True:
    d = 5

e = 1
if True:
    e = 2
elif True:
    e = 3


f = 33
for y in range(3):
    if True:
        f = 32
    elif False:
        f = 34
else:
    f = 35

g = 99
for _ in range(3):
    g = 98


h = 99
for _ in range(3):
    h = 98
else:
    h = 5

i = 77
while i:
    i = 67
else:
    i = 76

j = 27
while j:
    j = 37

# 1. Only except, both try and except vals are possible
try:
    k = 2
except:
    k = 3
k

# 2. only values from catch-all except and else are possible
try:
    l = 20
except:
    l = 30
else:
    l = 40
l

# 3. `finally` always runs
m = 50
try:
    m = 60
except:
    m = 70
finally:
    m = 80
m

# 4. `finally` always runs even with else
o = 90
try:
    o = 100
except:
    o = 110
else:
    o = 140
finally:
    o = 120
o

# 5. With specific exception handler, all 3 values are possible
try:
    p = 20
except ValueError:
    p = 30
else:
    p = 40
p

q = 33
match p:
    case 20:
        q = 34
    case 30:
        q  = 43
q

r = 33
match p:
    case 20:
        r = 34
    case x:
        r  = 43
r


(_ := [1, 2, 33])[(s := 2): (t := 3)][0]

u = 90
if (u := 92):
    u = 91

v = 70
if (v := 71):
    v = 72
elif (v := 73):
    v = 74
v


w = 70
if (w := 71):
    w = 72
elif w:
    w = 74
w