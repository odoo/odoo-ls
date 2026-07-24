import functools
from functools import cached_property


class Holder:
    @functools.cached_property
    def via_functools(self):
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
