
class Cl():

    def func(self):
        pass

class SubCl(Cl):
    pass

cl = Cl()
subcl = SubCl()

super(SubCl, subcl).func() #working

super().func() #OLS01006