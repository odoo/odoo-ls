import functools
from functools import cached_property


class Inner:
    def helper(self):
        pass


class Holder:
    @property
    def plain_prop(self) -> Inner:
        pass

    @functools.cached_property
    def via_functools(self) -> Inner:
        pass

    @cached_property
    def via_cached_bare(self):
        pass

    @tools.lazy_property
    def via_lazy_attr(self):
        pass

    @lazy_property
    def via_lazy_bare(self):
        pass

    def plain_method(self):
        pass


h = Holder()
h.via_functools
h.plain_prop.helper
h.via_functools.helper
