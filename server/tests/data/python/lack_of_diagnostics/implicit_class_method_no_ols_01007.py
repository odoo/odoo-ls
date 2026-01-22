class SomeClass(object):
    @classmethod
    def __init_subclass__(cls):
        super().__init_subclass__()