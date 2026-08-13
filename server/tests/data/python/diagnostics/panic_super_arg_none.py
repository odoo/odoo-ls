class Base:
    def method(self):
        pass


def get_class_no_return():
    pass


ClassNone = get_class_no_return()


class FooNone(Base):
    def method(self):
        super(ClassNone, self).method()
