import sys
from typing import TYPE_CHECKING, NoReturn, overload

if TYPE_CHECKING:
    from collections import OrderedDict
    OrderedAlias = OrderedDict


# used-before-assignment / undefined-variable

print(later_defined)  # expect: pylint_E0601
later_defined = 1
print(undefined_name)  # expect: pylint_E0602


def local_before_assignment():
    print(value)  # expect: pylint_E0601
    value = 1
    return value


def defined_in_try_used_in_except():
    try:
        result = int("1")
    except ValueError:
        print(result)
    return result


def loops_are_fine(items):
    for item in items:
        if item:
            print(previous)
        previous = item


def closures_are_fine():
    def inner():
        return outer_value
    outer_value = 1
    return inner


class ClassScope:
    attribute = 1
    other = LaterClass  # expect: pylint_E0601

    def method(self):
        return attribute  # expect: pylint_E0602

    def uses_dunder_class(self):
        return __class__


class LaterClass:
    pass


def guarded():
    try:
        unicode
    except NameError:
        pass


def runtime_use_of_type_checking_import():
    return OrderedDict()  # expect: pylint_E0601


def annotation_use(value: "OrderedDict") -> None:
    pass


comprehension = [x for x in range(3) if x]
lambda_function = lambda y: y + later_defined
walrus = [(z := w) for w in range(3)]
ternary = u.strip() if (u := " a ") else u
stripped = [v for s in [" a "] if (v := s.strip())]
print(z)

print(disabled_undefined)  # pylint: disable=undefined-variable
print(noqa_undefined)  # noqa: pylint_E0602
# pylint: disable-next=E0602
print(next_line_undefined)


# eval-used

eval("1 + 1")  # expect: pylint_W0123


def shadowed_eval(eval):
    return eval("1")


# unreachable

def unreachable_after_return():
    return 1
    print("unreachable")  # expect: pylint_W0101


def unreachable_after_exit():
    sys.exit(1)
    print("unreachable")  # expect: pylint_W0101


def never_returns() -> NoReturn:
    raise ValueError()


def unreachable_after_noreturn():
    never_returns()
    print("unreachable")  # expect: pylint_W0101


def empty_generator():
    return
    yield


for i in range(3):
    continue
    print(i)  # expect: pylint_W0101


# function-redefined

def redefined():
    pass


def redefined():  # expect: pylint_E0102
    pass


def _private_redefined():
    pass


def _private_redefined():
    pass


if sys.version_info >= (3, 8):
    def conditional():
        pass
else:
    def conditional():
        pass


class Redefinitions:
    @property
    def prop(self):
        return 1

    @prop.setter
    def prop(self, value):
        pass

    @overload
    def over(self, x: int) -> int: ...

    @overload
    def over(self, x: str) -> str: ...

    def over(self, x):
        return x

    def method(self):
        pass

    def method(self):  # expect: pylint_E0102
        pass


class Redefinitions:  # expect: pylint_E0102
    pass


# gettext

def _(source, *args, **kwargs):
    return source


class UserError(Exception):
    pass


def gettext_usages(name):
    _(name)  # expect: pylint_E8502
    _("%s and %s", name, name)  # expect: pylint_E8505
    _("%r is wrong", name)  # expect: pylint_E8506
    _("%(first)s and %(second)s", first=name, second=name)
    _("100%% sure about %s", name)
    UserError(_("Translated message"))
    UserError(name)
    return UserError("Static message")  # expect: pylint_E8507


# sql-injection

class Report:
    def get_data(self, table):
        self.env.cr.execute("SELECT * FROM %s" % table)  # expect: pylint_E8501
        self.env.cr.execute("SELECT * FROM table WHERE id = %s", (table,))
        self.env.cr.execute("SELECT * FROM %s" % self._table)
        query = "SELECT * FROM " + table
        self.env.cr.execute(query)  # expect: pylint_E8501
        self.env.cr.execute(f"SELECT {self._table} FROM x")
        self.env.cr.execute(f"SELECT {table} FROM x")  # expect: pylint_E8501
        self.env.cr.execute("SELECT * FROM %s" % table)  # pylint: disable=sql-injection

    def _private(self, table):
        self.env.cr.execute("SELECT * FROM %s" % table)


def build_order(param):
    return "ORDER BY " + param


def run_ordered_query(cr, param):
    cr.execute(build_order(param))  # expect: pylint_E8501


def run_filtered_query(cr, param):
    cr.execute(build_where(param))


def build_where(param):  # expect: pylint_E8501
    return "WHERE " + param
