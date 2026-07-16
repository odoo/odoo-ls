// Deliberately nested: Odoo puts @types at ~22 different depths, so a shallow scan misses most.
declare module "models" {
    export interface Store {
        module_2_record: unknown;
    }
}
