import { sharedHelper } from "@module_1/scoped/shared";
import { localHelper } from "@module_2/scoped/local";

export const total = () => sharedHelper() + localHelper();
