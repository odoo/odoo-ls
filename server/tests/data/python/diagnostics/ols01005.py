a = 5

class Test():
    def func(self):
        super().func()
        super(Test, self).func()
        super(a, self).func() # OLS01005
