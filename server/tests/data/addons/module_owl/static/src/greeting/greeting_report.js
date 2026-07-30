/** @odoo-module **/
import { Greeting } from "./greeting";

/** @param {Greeting} greeting */
export function announce(greeting) {
    return greeting.shout(2) + greeting.separator;
}
