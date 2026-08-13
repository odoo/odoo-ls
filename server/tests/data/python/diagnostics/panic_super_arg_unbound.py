class Base:
    def method(self):
        pass


if int():
    ClassMaybe = Base

Alias = ClassMaybe


class FooUnbound(Base):
    def method(self):
        super(Alias, self).method()
