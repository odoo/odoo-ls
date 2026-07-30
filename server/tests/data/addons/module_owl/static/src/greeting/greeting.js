/** @odoo-module **/
import { Component, props, t } from "@odoo/owl";
import { clamp } from "@web/core/utils/numbers";
import { capitalize } from "@web/core/utils/strings";

export class Greeting extends Component {
    static template = "module_owl.Greeting";

    props = props({
        name: t.string(),
        exclamations: t.number().optional(1),
    });

    separator = ", ";

    setup() {
        this.punctuation = "!";
    }

    get title() {
        return capitalize(this.props.name);
    }

    /** Repeats the punctuation, clamped to a sane length. */
    shout(count) {
        return this.punctuation.repeat(clamp(count, 1, 5));
    }

    onClick() {
        this.punctuation = "?";
    }
}
