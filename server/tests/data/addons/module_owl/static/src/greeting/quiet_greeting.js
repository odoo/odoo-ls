/** @odoo-module **/
import { Greeting } from "./greeting";

export class QuietGreeting extends Greeting {
    mute() {
        return this.shout(1);
    }
}
