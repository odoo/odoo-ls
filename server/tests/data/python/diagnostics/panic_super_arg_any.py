class Base:
    def method(self):
        pass


def get_class_stub():
    ...


ClassAny = get_class_stub()


class FooAny(Base):
    def method(self):
        super(ClassAny, self).method()
