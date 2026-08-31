// A vendored library with no header, so Odoo's loader never registers it and no
// name points at it. The export is there to prove the name alone is not enough.

export function bundle_thing() {
    return "bundle";
}
