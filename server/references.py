class RegisterableObject:

    def __init__(self):
        super().__init__()
        self.listeners = []
        self.deleted = False

    def mark_as_deleted(self):
        if not self.deleted:
            for listener in self.listeners:
                listener.delete()
        self.deleted = True


class RegisteredRef:

    __slots__ = ["_ref", "_hash", "callback", "containers"]

    def __init__(self, ref:RegisterableObject, callback=None):
        if not isinstance(ref, RegisterableObject):
            raise TypeError("Only objects of type RegisterableObject are allowed.")
        self._ref = ref
        self.callback = callback
        self.containers = []
        self._hash = hash(ref)
        ref.listeners.append(self)

    @property
    def ref(self):
        return self._ref

    @ref.setter
    def ref(self, value):
        raise AttributeError("Do not update a RegisteredRef. Please create a new one.")

    def __hash__(self):
        return self._hash

    def __eq__(self, other):
        if isinstance(other, RegisteredRef):
            return self.ref == other.ref
        return self.ref == other

    def __deepcopy__(self):
        return self.__class__(self.ref)

    def __copy__(self):
        return self.__class__(self.ref)

    def delete(self):
        if self.callback:
            self.callback()
        for container in self.containers:
            container._add_to_remove(self)
        self._ref = None

    def __bool__(self):
        return self.ref is not None


class RegisteredRefSet():

    def __init__(self, data=None):
        self.data = set()
        self._pending_removals = set()
        if data is not None:
            self.update(data)

    def _commit_removals(self):
        pop = self._pending_removals.pop
        discard = self.data.discard
        while self._pending_removals:
            try:
                item = pop()
            except IndexError:
                return
            discard(item)

    def __deepcopy__(self):
        raise NotImplementedError

    def _add_to_remove(self, item):
        self._pending_removals.add(item)

    def __iter__(self):
        for itemref in self.data:
            item = itemref.ref
            if item is not None:
                # Caveat: the iterator will keep a strong reference to
                # `item` until it is resumed or closed.
                yield item

    def __len__(self):
        if self._pending_removals:
            self._commit_removals()
        return len(self.data)

    def __contains__(self, item):
        for data in self.data:
            if item == data.ref:
                return True
        return False

    def add(self, item):
        if self._pending_removals:
            self._commit_removals()
        if isinstance(item, RegisteredRef):
            if not item.ref:
                raise ValueError("Cannot add a deleted reference.")
            self.data.add(item)
            item.containers.append(self)
        elif isinstance(item, RegisterableObject):
            ref = RegisteredRef(item)
            self.data.add(ref)
            ref.containers.append(self)
        else:
            raise TypeError("Only objects of type RegisteredObject are allowed.")

    def clear(self):
        if self._pending_removals:
            self._commit_removals()
        self.data.clear()

    def copy(self):
        return self.__class__(self)

    def pop(self):
        if self._pending_removals:
            self._commit_removals()
        while True:
            try:
                itemref = self.data.pop()
            except KeyError:
                raise KeyError('pop from empty WeakSet') from None
            try:
                itemref.containers.remove(self)
            except ValueError:
                pass
            item = itemref.ref
            if item is not None:
                return item

    def remove(self, item):
        if self._pending_removals:
            self._commit_removals()
        try:
            self.data.remove(RegisteredRef(item))
        except KeyError:
            raise KeyError(item) from None

    def discard(self, item):
        if self._pending_removals:
            self._commit_removals()
        self.data.discard(RegisteredRef(item))

    def update(self, other):
        if self._pending_removals:
            self._commit_removals()
        for element in other:
            self.add(element)

    def __ior__(self, other):
        self.update(other)
        return self

    def difference(self, other):
        newset = self.copy()
        newset.difference_update(other)
        return newset
    __sub__ = difference

    def difference_update(self, other):
        self.__isub__(other)
    def __isub__(self, other):
        if self._pending_removals:
            self._commit_removals()
        if self is other:
            self.data.clear()
        else:
            self.data.difference_update(RegisteredRef(item) for item in other)
        return self

    def intersection(self, other):
        return self.__class__(item for item in other if item in self)
    __and__ = intersection

    def intersection_update(self, other):
        self.__iand__(other)
    def __iand__(self, other):
        if self._pending_removals:
            self._commit_removals()
        self.data.intersection_update(RegisteredRef(item) for item in other)
        return self

    def issubset(self, other):
        return self.data.issubset(RegisteredRef(item) for item in other)
    __le__ = issubset

    def __lt__(self, other):
        return self.data < set(map(RegisteredRef, other))

    def issuperset(self, other):
        return self.data.issuperset(RegisteredRef(item) for item in other)
    __ge__ = issuperset

    def __gt__(self, other):
        return self.data > set(map(RegisteredRef, other))

    def __eq__(self, other):
        if not isinstance(other, self.__class__):
            return NotImplemented
        return self.data == set(map(RegisteredRef, other))

    def symmetric_difference(self, other):
        newset = self.copy()
        newset.symmetric_difference_update(other)
        return newset
    __xor__ = symmetric_difference

    def symmetric_difference_update(self, other):
        self.__ixor__(other)
    def __ixor__(self, other):
        if self._pending_removals:
            self._commit_removals()
        if self is other:
            self.data.clear()
        else:
            self.data.symmetric_difference_update(RegisteredRef(item) for item in other)
        return self

    def union(self, other):
        return self.__class__(e for s in (self, other) for e in s)
    __or__ = union

    def isdisjoint(self, other):
        return len(self.intersection(other)) == 0

    def __repr__(self):
        return repr(self.data)

class RegisteredRefList(list):

    def __init__(self):
        self._pending_removals = []

    def _add_to_remove(self, item):
        self._pending_removals.append(item)

    def append(self, item):
        if self._pending_removals:
            self._commit_removals()
        if isinstance(item, RegisteredRef):
            if not item.ref:
                raise ValueError("Cannot add a deleted reference.")
            super().append(item)
            item.containers.append(self)
        elif isinstance(item, RegisterableObject):
            ref = RegisteredRef(item)
            super().append(ref)
            ref.containers.append(self)
        else:
            raise TypeError("Only objects of type RegisteredRef are allowed.")

    def _commit_removals(self):
        pop = self._pending_removals.pop
        remove = self.remove
        while self._pending_removals:
            try:
                item = pop()
            except IndexError:
                return
            try:
                remove(item)
            except ValueError:
                pass

    def __iter__(self):
        for element in super().__iter__():
            yield element.ref

    # def remove(self, item):
    #     try:
    #         index = next(i for i, element in enumerate(super().__iter__()) if element.ref == item)
    #         del self[index]
    #     except StopIteration:
    #         pass

    def __len__(self):
        if self._pending_removals:
            self._commit_removals()
        return super().__len__()
