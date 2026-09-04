/** @odoo-module **/
import { aliased_thing } from "@fixture/aliased";
// `@fixture/opted_out` is the alias of an `@odoo-module ignore` file, so it names nothing.
import { opted_out_thing } from "@fixture/opted_out";
import { mini_thing } from "@module_owl/../lib/mini/mini";

export function all_lib_things() {
    return [aliased_thing(), opted_out_thing(), mini_thing()].join(" ");
}
