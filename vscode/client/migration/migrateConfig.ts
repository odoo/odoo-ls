import * as semver from "semver";
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
export async function migrateAfterDelay(context: ExtensionContext){
    if (String(workspace.getConfiguration().get("Odoo.serverLogLevel")) == "afterDelay"){
        workspace.getConfiguration().update("Odoo.serverLogLevel", "adaptive", ConfigurationTarget.Global)
    }
}
export async function migrateShowHome(context: ExtensionContext) {
    // Reset the welcome view display setting if the extension version is 0.8.0
    const currentSemVer = semver.parse(context.extension.packageJSON.version);
    const lastRecordedSemVer = semver.parse(context.globalState.get("Odoo.lastRecordedVersion", ""));
    const targetSemVer = semver.parse("0.8.0");
    if (currentSemVer >= targetSemVer && lastRecordedSemVer && lastRecordedSemVer < targetSemVer) {
        context.globalState.update('Odoo.displayWelcomeView', undefined);
    }
}