/** @odoo-module **/
import { Component, useState } from "@odoo/owl";

export class Counter extends Component {
    static template = "module_owl.Counter";
    static props = ["initialValue"];

    setup() {
        this.state = useState({ value: 0 });
    }

    increment() {
        this.state.value++;
    }

    get doubled() {
        return this.state.value * 2;
    }
}
