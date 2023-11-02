export const STATE_VERSION = 100

export const stateInit = {
    "Odoo.configurations": {},
    "Odoo.nextConfigId": 0,
    "Odoo.stateVersion": STATE_VERSION,
}

export function getConfigurationStructure(id: number = 0) {
    return {
        "id": id,
        "name": `New Configuration ${id}`,
        "odooPath": "",
        "addons": [],
    }
}
