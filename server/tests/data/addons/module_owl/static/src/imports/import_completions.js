// `module_owl` does not depend on `module_unrelated`.
// But this import puts its file in tsserver's program.
import "@module_unrelated/tools";
// `bundle.js` has no module header, so Odoo's loader never registers it.
import "@module_owl/../lib/bundle/bundle";

export function find_helper() {
    const helper = unrelatedH;
    const thing = bundle_th;
    return [helper, thing];
}
