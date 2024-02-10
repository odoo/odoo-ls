import {EventEmitter} from "vscode";

export const selectedConfigurationChange = new EventEmitter();
export const ConfigurationsChange = new EventEmitter();
export const clientStopped = new EventEmitter();
