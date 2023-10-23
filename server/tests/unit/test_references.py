from ...references import *

def isin(elem, target):
    for part in target:
        if elem is part:
            return True
    return False

def test_ref():
    class A(RegisterableObject):
        pass

    class B(RegisterableObject):
        pass

    class C:
        pass

    a = A()
    b = B()
    c = C()
    ref_a = RegisteredRef(a)
    # ref_b = RegisteredRef(b)
    # ref_b1 = RegisteredRef(b1)

    ### RegisterableObject & RegisteredRef
    # deny creating refs to non-RegistrableObjects
    try:
        _ = RegisteredRef(c)
        assert False, "Should not be able to create a reference to a non RegisterableObject"
    except:
        assert True

    # check that a is stored in ref_a and not editable
    assert ref_a
    assert ref_a.ref == a
    try:
        ref_a.ref = b
        assert False, "Should not be able to change ref"
    except:
        assert True
    a_copy = ref_a
    assert a_copy.ref == ref_a.ref

    # check deletion
    a.mark_as_deleted()
    assert not ref_a
    assert ref_a.ref is None

    # check use-after-free
    try:
        _ = RegisteredRef(a)
        assert False, "Should not be able to make new ref to deleted RegisterableObject"
    except:
        assert True

    # copy semantics
    import copy
    a = A()
    ref_a = RegisteredRef(a)
    copy_a = copy.copy(ref_a)
    # When copying a ref, either the ref should be invalid or
    # the copy should be added to the listeners of the obj
    assert copy_a.ref is a and isin(copy_a,a.listeners)
    # More generally, a ref should always be in the listeners
    # of the obj it refers to
    assert isin(ref_a, ref_a.ref.listeners)
    assert isin(copy_a, copy_a.ref.listeners)
    try:
        _ = copy.deepcopy(ref_a)
        assert False, "No deepcopies allowed" # As per discord convo
    except:
        assert True

    # check double-free ­
    global df_counter
    df_counter = 1
    def double_free_callback():
        global df_counter
        assert df_counter, "Callback called after deletion"
        df_counter = df_counter - 1
    a = A()
    ref_a = RegisteredRef(a, double_free_callback)
    a.mark_as_deleted()
    a.mark_as_deleted()


    ### RegisteredRefSet
    # adding, removing, mark_as_delete
    a = A()
    b = B()
    a1 = A()
    b1 = B()
    ref_a = RegisteredRef(a)
    ref_b = RegisteredRef(b)
    ref_a1 = RegisteredRef(a1)
    set_a = RegisteredRefSet()
    set_a.add(a)
    set_a.add(a1)
    set_a.add(b)
    set_a.add(a)
    assert len(set_a) == 3
    assert a in set_a
    assert a1 in set_a
    assert b in set_a
    set_a.remove(a)
    assert len(set_a) == 2
    assert a not in set_a
    assert a1 in set_a
    assert b in set_a
    set_a.add(a)
    assert len(set_a) == 3
    assert a in set_a
    assert ref_a in set_a
    assert a1 in set_a
    assert b in set_a
    set_a.add(ref_a)
    assert len(set_a) == 3
    assert a in set_a
    assert ref_a in set_a
    set_a.remove(ref_a.ref)
    assert len(set_a) == 2
    assert a not in set_a
    assert ref_a not in set_a
    set_a.add(ref_a)
    assert len(set_a) == 3
    assert a in set_a
    assert ref_a in set_a
    b.mark_as_deleted()
    assert len(set_a) == 2
    assert b not in set_a
    assert a in set_a
    assert a1 in set_a
    set_a.add(b1)
    assert len(set_a) == 3
    assert b not in set_a
    assert a in set_a
    assert a1 in set_a
    assert b1 in set_a
    a.mark_as_deleted()
    assert len(set_a) == 2
    assert a not in set_a
    assert ref_a not in set_a
    assert a1 in set_a
    assert b1 in set_a
    set_a.discard(a1)
    assert len(set_a) == 1
    assert a1 not in set_a
    set_a.clear()
    assert len(set_a) == 0
    try:
        set_a.add(c)
        assert False, "Non-registereableobj are not allowed"
    except:
        assert True

    # deleted ref semantics
    b.mark_as_deleted()
    try:
        set_a.add(b)
        assert False, "Cannot add objects marked as deleted"
    except:
        assert True

    try:
        assert ref_b.ref is None
        set_a.add(ref_b)
        assert False, "Cannot add deleted refs"
    except:
        assert True

    # cleanup
    a = A()
    b = B()
    a1 = A()
    b1 = B()
    ref_a = RegisteredRef(a)
    ref_b = RegisteredRef(b)
    ref_a1 = RegisteredRef(a1)
    set_a = RegisteredRefSet()
    set_a.add(a)
    set_a.add(a1)
    set_a.add(b)
    set_a.add(a)

    # copy
    copy_set_a = set_a.copy()
    assert len(copy_set_a) == 3
    a.mark_as_deleted()
    assert len(copy_set_a) == 2
    assert len(set_a) == 2
    try:
        _ = copy.deepcopy(set_a)
        assert 0, "Deepcopy are not allowed on sets"
    except:
        assert 1

    # pop
    copy_set_a.add(b1)
    assert len(copy_set_a) == 3
    assert len(set_a) == 2
    popped = copy_set_a.pop()
    assert popped
    assert len(copy_set_a) == 2

    # set methods
    a = A()
    b = B()
    a1 = A()
    b1 = B()
    ref_a = RegisteredRef(a)
    ref_b = RegisteredRef(b)
    set_a = RegisteredRefSet()
    set_b = RegisteredRefSet()
    set_a.add(a)
    set_a.add(b)
    set_b.add(b)
    assert set_a >= set_b
    assert not set_a <= set_b
    assert len(set_a) == 2
    assert len(set_b) == 1
    diff = set_a - set_b
    assert len(diff) == 1
    assert a in diff
    assert len(set_a) == 2
    assert len(set_b) == 1
    set_a.difference_update(set_b)
    assert len(set_a) == 1
    set_a = RegisteredRefSet()
    set_b = RegisteredRefSet()
    set_a.add(a)
    set_a.add(a1)
    set_b.add(b)
    isec = set_a & set_b
    assert len(isec) == 0
    set_a.update(set_b)
    set_b.add(b1)
    isec = set_a & set_b
    assert len(isec) == 1
    assert b in isec
    leftover_set = set_b.copy()
    leftover_set.intersection_update(set_a)
    assert len(leftover_set) == 1
    xor_set = set_a ^ set_b
    assert len(xor_set) == 3
    assert xor_set.isdisjoint(leftover_set)
    xor_set.update(leftover_set)
    assert (set_a | set_b) == xor_set


    ### RegisteredRefList
    # Only pop/append
    a = A()
    b = B()
    a1 = A()
    b1 = B()
    ref_a = RegisteredRef(a)
    ref_a1 = RegisteredRef(a1)
    ref_b = RegisteredRef(b)
    list_a = RegisteredRefList()
    list_a.append(ref_a)
    list_a.append(ref_a1)
    list_a.append(b)
    list_a.append(a)
    assert len(list_a) == 4
    assert a in list_a
    assert a1 in list_a
    assert b in list_a
    assert list_a[3] == a
    pop_a = list_a.pop(0)
    assert pop_a is ref_a
    assert len(list_a) == 3
    assert a in list_a
    assert a1 in list_a
    assert b in list_a
    list_a.append(b1)
    assert len(list_a) == 4
    assert b1 in list_a
    b.mark_as_deleted()
    assert len(list_a) == 3
    assert b not in list_a

    # deleted ref semantics
    try:
        list_a.add(b)
        assert False, "Cannot add objects marked as deleted"
    except:
        assert True

    try:
        assert ref_b.ref is None
        list_a.add(ref_b)
        assert False, "Cannot add deleted refs"
    except:
        assert True
