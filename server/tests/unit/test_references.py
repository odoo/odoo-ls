import os
import pytest

from server.references import *

def test_ref():
    class A(RegisterableObject):
        pass

    class B(RegisterableObject):
        pass

    class C:
        pass

    a = A()
    a1 = A()
    b = B()
    b1 = B()
    c = C()
    ref_a = RegisteredRef(a)
    ref_a1 = RegisteredRef(a1)
    ref_b = RegisteredRef(b)
    ref_b1 = RegisteredRef(b1)
    try:
        ref_c = RegisteredRef(c)
        assert False, "Should not be able to create a reference to a non RegisterableObject"
    except:
        assert True
    #check that a is stored in ref_a and not editable
    assert ref_a
    assert ref_a.ref == a
    try:
        ref_a.ref = b
        assert False, "Should not be able to change ref"
    except:
        assert True
    a_copy = ref_a
    assert a_copy.ref == ref_a.ref
    #check deletion
    a.mark_as_deleted()
    assert not ref_a
    assert ref_a.ref is None
    a = A()
    ref_a = RegisteredRef(a)

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
    assert a1 in set_a
    assert b in set_a
    b.mark_as_deleted()
    assert len(set_a) == 2
    assert b not in set_a
    assert a in set_a
    assert a1 in set_a

    b = B()

    set_a.add(b1)
    assert len(set_a) == 3
    assert b not in set_a
    assert a in set_a
    assert a1 in set_a
    assert b1 in set_a

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
    list_a.remove(a)
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

    dic = RegisteredRefDictKey()
    dic[a] = 1
    assert len(dic) == 1
    assert a in dic
    assert dic[a] == 1
    dic[ref_a] = 2
    assert len(dic) == 1
    assert a in dic
    assert dic[a] == 2
    dic[b] = 3
    assert len(dic) == 2
    assert a in dic
    assert b in dic
    assert dic[a] == 2
    assert dic[b] == 3
    a.mark_as_deleted()
    assert len(dic) == 1
    assert a not in dic
    assert b in dic
    assert dic[b] == 3