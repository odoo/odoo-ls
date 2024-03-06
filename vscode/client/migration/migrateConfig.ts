import {
    ExtensionContext,
    workspace,
    ConfigurationTarget,
} from "vscode";


export async function migrateConfigToSettings(context: ExtensionContext){
    let oldConfig = context.globalState.get("Odoo.configurations");
    if(oldConfig){
        await context.globalState.update("Odoo.configurations", undefined); // deleting the config in globalStorage
        workspace.getConfiguration().update("Odoo.configurations", oldConfig, ConfigurationTarget.Global); // putting it the settings
    }
}
