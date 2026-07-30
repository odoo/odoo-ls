/** @odoo-module **/
import { Greeting } from "./greeting";

export class LoudGreeting extends Greeting {
    static template = "module_owl.LoudGreeting";

    volume = 3;

    banner() {
        return this.separator.repeat(this.volume);
    }
}
