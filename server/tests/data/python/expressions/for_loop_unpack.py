for x in [1, 2]:
    pass

for x1 in [1, "2"]:
    pass

for x2, y2 in [(1, 2), (3, 4)]:
    pass

for (x3, y3) in [(1, 2), (3, 4)]:
    pass

class Class:

    def __iter__(self) -> int:
        pass

custom_obj = Class()

for x4 in custom_obj:
    pass

for x5 in {1, "2"}:
    pass