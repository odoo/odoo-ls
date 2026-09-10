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


def completion_first_statement(animal: Animal):
    if isinstance(animal, Dog):
        animal.


def completion_second_statement(animal: Animal):
    if isinstance(animal, Dog):
        pass
        animal.


def single_char_name(a: Animal):
    if isinstance(a, Dog):
        a


def break_in_loop(cond: bool):
    for i in [1, 2]:
        if cond:
            found = get_animal()
            break
    found


def and_chain_nested_or(flag: bool, other: bool):
    if flag and (other or (m := get_animal())):
        m


def colliding_narrowings(animal: Animal):
    if not isinstance(animal, Cat):
        assert isinstance(animal, Dog)
    animal


def explicit_else_of_negated_check(animal: Animal):
    if not isinstance(animal, Dog):
        pass
    else:
        animal


def one_liner_no_space(animal: Animal):
    if isinstance(animal, Dog):animal


def one_liner_semicolons(animal: Animal):
    if isinstance(animal, Dog):animal;animal


def one_liner_boolop_no_space(animal: Animal, flag: bool):
    if flag and isinstance(animal, Dog):animal


def one_liner_while(animal: Animal):
    while isinstance(animal, Dog):animal


def and_chain_check_last(animal: Animal, flag: bool):
    if flag and isinstance(animal, Dog):
        animal


def elif_and_chain_walrus(animal: Animal, cond: bool):
    if cond:
        pass
    elif (z := get_animal()) and isinstance(animal, Dog):
        animal
        z


# Module-level decoy: strengthens break_does_not_erase_variable_after_loop above. If `found`
# ever becomes locally unreachable there, resolution falls through to *this*, so the test fails
# either way instead of the wrong-symbol case silently reading as "unresolvable, but harmless".
found = "MODULE_LEVEL_DECOY"


def unresolvable_type_name(animal: Animal):
    if isinstance(animal, NotDefinedAnywhere):
        animal


def while_and_chain(animal: Animal, flag: bool):
    while flag and isinstance(animal, Dog):
        animal


def one_liner_assert(animal: Animal):
    assert isinstance(animal, Dog);animal


def one_liner_assert_completion(animal: Animal):
    assert isinstance(animal, Dog);animal.


def get_cat() -> Cat:
    pass


def narrowing_collides_with_reassignment(animal: Animal):
    if isinstance(animal, Dog):
        animal = get_cat()
        animal


def or_negative_guard(animal: Animal, flag: bool):
    if not isinstance(animal, Dog) or flag:
        return
    animal


def or_operand_sees_previous_negation(animal: Animal, flag: bool):
    if not isinstance(animal, Dog) or animal:
        pass


def or_of_positive_checks_else(animal: Animal):
    if isinstance(animal, Dog) or isinstance(animal, Cat):
        pass
    else:
        animal


def or_of_positive_checks(animal: Animal):
    if isinstance(animal, Dog) or isinstance(animal, Cat):
        animal


def or_with_non_check_operand(animal: Animal, flag: bool):
    if isinstance(animal, Dog) or flag:
        animal


def or_on_different_names(animal: Animal, other: Animal):
    if isinstance(animal, Dog) or isinstance(other, Cat):
        animal


def and_of_negated_checks_else(animal: Animal):
    if not isinstance(animal, Dog) and not isinstance(animal, Cat):
        pass
    else:
        animal


def while_else_then_after(animal: Animal):
    while not isinstance(animal, Dog):
        pass
    else:
        animal
    animal


def while_else_sees_body_binding(animal: Animal):
    while not isinstance(animal, Dog):
        found = get_animal()
    else:
        found
        animal


def for_else_sees_body_binding():
    for i in [1, 2]:
        found = get_animal()
    else:
        found
