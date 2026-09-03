/** @odoo-module **/
import { queryOne } from "@odoo/hoot-dom";
import { test_helper } from "@module_owl/../tests/helpers";
import { answer_is_correct } from "@module_owl/greeting/utils";

export function run_tests() {
    queryOne(".o_greeting");
    return answer_is_correct(test_helper());
}
