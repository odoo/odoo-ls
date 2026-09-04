class Animal:
    def speak(self):
        pass


class Dog(Animal):
    def bark(self):
        pass


class Cat(Animal):
    def meow(self):
        pass


class Other:
    def unrelated(self):
        pass


def get_animal() -> Animal:
    pass


def basic_if(animal: Animal):
    if isinstance(animal, Dog):
        animal


def narrowing_ends_after_if(animal: Animal):
    if isinstance(animal, Dog):
        pass
    animal


def else_branch_not_narrowed_to_subtype(animal: Animal):
    if isinstance(animal, Dog):
        pass
    else:
        animal


def elif_chain(animal: Animal):
    if isinstance(animal, Dog):
        animal
    elif isinstance(animal, Cat):
        animal
    else:
        animal


def negative_guard_return(animal: Animal):
    if not isinstance(animal, Dog):
        return
    animal


def negative_guard_after_unrelated_conditions(animal: Animal, flag: bool, other: int):
    # Reaching the fallthrough still requires *this* test to have been false, regardless of
    # what the unrelated earlier branches checked - so the guarantee holds just the same.
    if flag:
        return
    elif other > 100:
        return
    elif not isinstance(animal, Dog):
        return
    animal


def negative_guard_with_non_exiting_branch(animal: Animal, flag: bool):
    # `flag`'s branch does not exit, so it (unnarrowed) joins the merge alongside the
    # narrowed fallthrough: reaching `animal` below no longer guarantees Dog, since the
    # `flag` path could have gotten here too, leaving `animal` untouched.
    if flag:
        pass
    elif not isinstance(animal, Dog):
        return
    animal


def negative_guard_not_the_last_condition(animal: Animal, other: int):
    # The negated isinstance check is the *first* test, not the last - reaching the elif's
    # own test (or falling further through it) already guarantees Dog either way, since
    # `not isinstance(animal, Dog)` being false is what got us past the first `if` at all.
    if not isinstance(animal, Dog):
        return
    elif other > 100:
        return
    animal


def assert_narrows(animal: Animal):
    assert isinstance(animal, Dog)
    animal


def tuple_of_types(animal: Animal):
    if isinstance(animal, (Dog, Cat)):
        animal


def and_combined_condition(animal: Animal):
    if isinstance(animal, Dog) and animal.bark():
        animal


def reassignment_invalidates_narrowing(animal: Animal):
    if isinstance(animal, Dog):
        animal = get_animal()
        animal


def nested_isinstance(animal: Animal):
    if isinstance(animal, Animal):
        if isinstance(animal, Dog):
            animal


class Holder:
    def __init__(self):
        self.animal: Animal = get_animal()

    def narrows_attribute(self):
        if isinstance(self.animal, Dog):
            attr = self.animal
            attr


def while_condition(animal: Animal):
    while isinstance(animal, Dog):
        animal


def while_negative_guard(animal: Animal):
    while not isinstance(animal, Dog):
        animal = get_animal()
    animal


def while_negative_guard_with_else(animal: Animal):
    while not isinstance(animal, Dog):
        animal = get_animal()
    else:
        animal


def ternary_expression(animal: Animal):
    x = animal if isinstance(animal, Dog) else None
    x


def unrelated_type_check(animal: Animal):
    if isinstance(animal, Other):
        animal
