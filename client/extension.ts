/* -------------------------------------------------------------------------
 * Original work Copyright (c) Microsoft Corporation. All rights reserved.
 * Original work licensed under the MIT License.
 * See ThirdPartyNotices.txt in the project root for license information.
 * All modifications Copyright (c) Open Law Library. All rights reserved.
 *
 * Licensed under the Apache License, Version 2.0 (the "License")
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http: // www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 * ----------------------------------------------------------------------- */
"use strict";

import * as net from "net";
import * as path from "path";
import {
    commands,
    ExtensionContext,
    ExtensionMode,
    QuickPickItem,
    StatusBarAlignment,
    StatusBarItem,
    ThemeIcon,
    workspace,
    window,
    QuickPickItemKind,
    TextDocument,
    OutputChannel,
    Uri,
} from "vscode";
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    integer,
} from "vscode-languageclient/node";
import { WelcomeWebView } from "./views/welcome/welcomeWebView";
import { ConfigurationWebView } from './views/configurations/configurationWebView';
import { CrashReportWebView } from './views/crash_report/crashReport'
import {
    selectedConfigurationChange,
    ConfigurationsChange
} from './utils/events'
import { execSync } from "child_process";
import { getCurrentConfig } from "./utils/utils";
import * as fs from 'fs';
import { getConfigurationStructure, stateInit } from "./utils/validation";
import * as os from 'os';
let client: LanguageClient;
let odooStatusBar: StatusBarItem;
let isLoading: boolean;
let debugFile = `pygls-${new Date().toISOString().replaceAll(":","_")}.log`

function getClientOptions(): LanguageClientOptions {
    return {
        // Register the server for plain text documents
        documentSelector: [
            { scheme: "file", language: "python" },
            { scheme: "untitled", language: "python" },
        ],
        synchronize: {
            // Notify the server about file changes to '.clientrc files contain in the workspace
            fileEvents: workspace.createFileSystemWatcher("**/.clientrc"),
        },
    };
}

function validateState(context: ExtensionContext, outputChannel: OutputChannel) {
    try {
        let globalState = context.globalState
        let stateVersion = globalState.get('Odoo.stateVersion', false)
        if (!stateVersion || stateVersion != stateInit['Odoo.stateVersion']) {
            for (let key of Object.keys(stateInit)) {
                let state = globalState.get(key, null)
                let versionState = stateInit[key]
                if (!state) {
                    globalState.update(key, versionState)
                }
                else {
                    let updates = false
                    let configurations = {}
                    if (key === 'Odoo.configurations' && Object.keys(state).length > 0) {
                        for (let configId of Object.keys(state)) {
                            let config = state[configId]
                            let configStruct = getConfigurationStructure()
                            for (let confKey of Object.keys(configStruct)) {
                                if (!(confKey in config)) {
                                    config[confKey] = configStruct[confKey]
                                    updates = true
                                }
                            }

                            configurations = {
                                ...configurations,
                                [configId]: config,
                            }
                        }
                    }
                    if (updates) {
                        globalState.update(key, configurations)
                    }
                }

            }
            globalState.update('Odoo.stateVersion', stateInit['Odoo.stateVersion'])
        }

    }
    catch (error) {
        outputChannel.appendLine(error)
        displayCrashMessage(context, error, outputChannel, 'func.validateState')
    }
}

function setMissingStateVariables(context: ExtensionContext, outputChannel: OutputChannel) {
    const globalStateKeys = context.globalState.keys();
    const workspaceStateKeys = context.workspaceState.keys();
    let globalVariables = new Map<string, any>([
        ["Odoo.configurations", stateInit["Odoo.configuration"]],
        ["Odoo.nextConfigId", stateInit["Odoo.nextConfigId"]],
        ["Odoo.stateVersion", stateInit["Odoo.stateVersion"]],
    ]);
    const workspaceVariables = new Map([["Odoo.selectedConfiguration", [-1]]]);

    for (let key of globalVariables.keys()) {
        if (!globalStateKeys.includes(key)) {
            outputChannel.appendLine(`${key} was missing in global state. Setting up the variable.`);
            context.globalState.update(key, globalVariables.get(key));
        }
    }

    for (let key of workspaceVariables.keys()) {
        if (!workspaceStateKeys.includes(key)) {
            outputChannel.appendLine(`${key} was missing in workspace state. Setting up the variable.`);
            context.workspaceState.update(key, workspaceVariables.get(key));
        }
    }
}

function isPythonModuleInstalled(pythonPath: string, moduleName: string): boolean {
    try {
        execSync(pythonPath + ' -c "import ' + moduleName + '"');
        return true;
    } catch (error) {
        return false;
    }
}

function checkPythonDependencies(pythonPath: string, notification: boolean = true) {
    let missingDep: Array<string> = [];
    if (!pythonPath) return false;

    if (!isPythonModuleInstalled(pythonPath, 'pygls')) {
        missingDep.push('pygls');
    }
    if (!isPythonModuleInstalled(pythonPath, 'parso')) {
        missingDep.push('parso');
    }

    if (missingDep.length) {
        if (notification)
            window.showErrorMessage(`Odoo: Couldn't start the Language Server. Missing Python ${missingDep.length == 1 ? 'dependency': 'dependencies'}: ${missingDep.join(", ")}`);
        return false
    }
    return true;
}

function startLangServerTCP(addr: number, outputChannel: OutputChannel): LanguageClient {
    const serverOptions: ServerOptions = () => {
        return new Promise((resolve /*, reject */) => {
            const clientSocket = new net.Socket();
            clientSocket.connect(addr, "127.0.0.1", () => {
                resolve({
                    reader: clientSocket,
                    writer: clientSocket,
                });
            });
        });
    };

    const clientOptions: LanguageClientOptions = getClientOptions();

    clientOptions.outputChannel = outputChannel;

    return new LanguageClient(
        `tcp lang server (port ${addr})`,
        serverOptions,
        clientOptions
    );
}

function startLangServer(
    command: string,
    args: string[],
    cwd: string,
    outputChannel: OutputChannel
): LanguageClient {
    const serverOptions: ServerOptions = {
        args,
        command,
        options: { cwd },
    };
    const clientOptions: LanguageClientOptions = getClientOptions();
    clientOptions.outputChannel = outputChannel;

    return new LanguageClient(command, serverOptions, clientOptions);
}

function setStatusConfig(context: ExtensionContext, statusItem: StatusBarItem) {
    const config = getCurrentConfig(context);
    let text = (config ? `Odoo (${config["name"]})` : `Odoo (Disabled)`);
    statusItem.text = (isLoading) ? "$(loading~spin) " + text : text;
}

function getLastConfigId(context: ExtensionContext): number | undefined {
    return context.globalState.get("Odoo.nextConfigId");
}

function IncrementLastConfigId(context: ExtensionContext) {
    const lastId: number = context.globalState.get("Odoo.nextConfigId");
    context.globalState.update("Odoo.nextConfigId", lastId + 1);
}

async function addNewConfiguration(context: ExtensionContext) {
    const configId = getLastConfigId(context);
    let configs: Map<number, object> = context.globalState.get("Odoo.configurations");
    await context.globalState.update(
        "Odoo.configurations",
        {
            ...configs,
            [configId]: getConfigurationStructure(configId),
        }
    );
    ConfigurationsChange.fire(null);
    IncrementLastConfigId(context);
    ConfigurationWebView.render(context, configId);
}

function changeSelectedConfig(context: ExtensionContext, configId: Number) {
    context.workspaceState.update("Odoo.selectedConfiguration", configId);
    selectedConfigurationChange.fire(null);
}

async function displayCrashMessage(context: ExtensionContext, crashInfo: string, outputChannel: OutputChannel, command: string = null) {
    // Capture the content of the file active when the crash happened
    let activeFile: TextDocument;
    if (window.activeTextEditor) {
        activeFile = window.activeTextEditor.document;
    } else {
        activeFile = null;
    }
    const selection = await window.showErrorMessage(
        "The Odoo extension encountered an error and crashed. Do you wish to send a crash report ?",
        "Send crash report",
        "Open logs",
        "Cancel"
    );

    switch (selection) {
        case ("Send crash report"):
            CrashReportWebView.render(context, activeFile, crashInfo, command, debugFile);
            break
        case ("Open logs"):
            outputChannel.show();
            break
    }
}

function activateVenv(pythonPath: String) {
    let activatePathArray = pythonPath.split(path.sep).slice(0, pythonPath.split(path.sep).length - 1)
    let activatePath = `${activatePathArray.join(path.sep)}${path.sep}activate`
    if (fs.existsSync(activatePath)) {
        switch (os.type()) {
            case 'Linux':
                execSync(`. ${activatePath}`)
                break;
            case 'Windows_NT':
                execSync(`${activatePath}`)
                break;
        }
    }
}

function getPythonPath(context: ExtensionContext) {
    const config = getCurrentConfig(context);
    const pythonPath = config && config["pythonPath"] != '' ? config["pythonPath"] : "python3";
    activateVenv(pythonPath)
    return pythonPath
}

function startLanguageServerClient(context: ExtensionContext, pythonPath:string, outputChannel: OutputChannel) {
    let client: LanguageClient;
    if (context.extensionMode === ExtensionMode.Development) {
        // Development - Run the server manually
        client = startLangServerTCP(2087, outputChannel);
        debugFile='pygls.log'
    } else {
        // Production - Client is going to run the server (for use within `.vsix` package)
        const cwd = path.join(__dirname, "..", "..");

        if (!pythonPath) {
            outputChannel.appendLine("[INFO] pythonPath is not set, defaulting to python3.");
        }
        client = startLangServer(pythonPath, ["-m", "server", "--log", debugFile], cwd, outputChannel);
    }

    return client;
}

function deleteOldFiles(context: ExtensionContext, outputChannel: OutputChannel) {
    const files = fs.readdirSync(context.extensionUri.fsPath).filter(fn => fn.startsWith('pygls-') && fn.endsWith('.log'));
    for (const file of files) {
        let dateLimit = new Date()
        dateLimit.setDate(dateLimit.getDate() - 2);
        let date = new Date(file.slice(6, -4).replaceAll("_",":"))
        if (date < dateLimit) {
            fs.unlinkSync(Uri.joinPath(context.extensionUri, file).fsPath)
        }
    }
}

async function checkAddons(context: ExtensionContext, odooOutputChannel: OutputChannel) {
    let files = await workspace.findFiles('**/__manifest__.py')
    let currentConfig = getCurrentConfig(context)
    let missingFiles = files.filter(file => {
        return !currentConfig.addons.some((addon) => file.fsPath.startsWith(addon))
    })
    let missingPaths = [...new Set(missingFiles.map(file => {
        let filePath = file.fsPath.split(path.sep)
        return filePath.slice(0, filePath.length - 2).join(path.sep)
    }))]
    odooOutputChannel.appendLine("Missing addon paths : " + missingPaths.length)
    if (missingPaths.length > 0) {
        odooOutputChannel.appendLine("Missing addon paths : " + JSON.stringify(missingPaths))
        const selection = await window.showWarningMessage(
            `We detected addon paths that weren't added in the current configuration. Would you like to add them?`,
            "Update current configuration",
            "View Paths",
            "Ignore"
        );
        switch (selection) {
            case ("Update current configuration"):
                ConfigurationWebView.render(context, currentConfig.id);
                break
            case ("View Paths"):
                odooOutputChannel.show();
                break
        }
    }
}

async function checkOdooPath(context: ExtensionContext) {
    let currentConfig = getCurrentConfig(context)
    for (const f of workspace.workspaceFolders) {
        let odoo_path = undefined
        if (fs.existsSync(Uri.joinPath(f.uri, 'odoo-bin').fsPath)) {
            odoo_path = f.uri
        }
        else if (fs.existsSync(Uri.joinPath(Uri.joinPath(f.uri, 'odoo'), 'odoo-bin').fsPath)) {
            odoo_path = Uri.joinPath(f.uri, 'odoo')
        }
        if (odoo_path && currentConfig && odoo_path.fsPath !== currentConfig.odooPath) {
            const selection = await window.showWarningMessage(
                `The Odoo configuration selected does not match the odoo path in the workspace. Would you like to change it?`,
                "Update current configuration",
                "Ignore"
            );
            switch (selection) {
                case ("Update current configuration"):
                    ConfigurationWebView.render(context, currentConfig.id);
                    break
            }
        }
    }
}

function initializeSubscriptions(context: ExtensionContext, client: LanguageClient, odooOutputChannel: OutputChannel): void {

    function checkRestartPythonServer(){
        if (getCurrentConfig(context)) {
            let oldPythonPath = pythonPath
            pythonPath = getPythonPath(context);
            if (oldPythonPath != pythonPath) {
                odooOutputChannel.appendLine('[INFO] Python path changed, restarting language server: ' + oldPythonPath + " " + pythonPath);
                if (client.diagnostics) client.diagnostics.clear();
                if (client.isRunning()) client.stop();
                if (client) client.dispose();
                client = startLanguageServerClient(context, pythonPath, odooOutputChannel);
                for (const disposable of context.subscriptions) {
                    try {
                        disposable.dispose();
                    } catch (e) {
                        console.error(e);
                    }
                }
                if (context.extensionMode === ExtensionMode.Development) {
                    context.subscriptions.push(commands.registerCommand("odoo.testCrashMessage", () => { displayCrashMessage(context, "Test crash message", odooOutputChannel); }));
                }
                initializeSubscriptions(context, client, odooOutputChannel)
                if (context.extensionMode === ExtensionMode.Production) {
                    if (checkPythonDependencies(pythonPath)) {
                        client.start();
                    }
                } else {
                    client.start();
                }
            }
        }
    }

    let pythonPath = getPythonPath(context);
    checkOdooPath(context);
    checkAddons(context, odooOutputChannel);
    odooStatusBar = window.createStatusBarItem(StatusBarAlignment.Left, 100);
    setStatusConfig(context, odooStatusBar);
    odooStatusBar.show();
    odooStatusBar.command = "odoo.clickStatusBar"
    context.subscriptions.push(odooStatusBar);

    context.subscriptions.push(
        commands.registerCommand('odoo.clickStatusBar', async () => {
            try {
                const qpick = window.createQuickPick();
                const configs: Map<integer, object> = context.globalState.get("Odoo.configurations");
                let selectedConfiguration = null;
                const currentConfig = getCurrentConfig(context);
                let currentConfigItem: QuickPickItem;
                const configMap = new Map();
                const separator = { kind: QuickPickItemKind.Separator };
                const addConfigItem = {
                    label: "$(add) Add new configuration"
                };
                const disabledItem = {
                    label: "Disabled"
                }
                const gearIcon = new ThemeIcon("gear");

                for (const configId in configs) {
                    if (currentConfig && configId == currentConfig["id"])
                        continue;
                    configMap.set({ "label": configs[configId]["name"], "buttons": [{ iconPath: gearIcon }] }, configId)
                }

                let picks = [disabledItem, ...Array.from(configMap.keys())];
                if (picks.length)
                    picks.push(separator);

                if (currentConfig) {
                    currentConfigItem = { "label": currentConfig["name"], "description": "(current)", "buttons": [{ iconPath: gearIcon }] };
                    picks.splice(currentConfig["id"] + 1, 0, currentConfigItem);
                }

                picks.push(addConfigItem);
                qpick.title = "Select a configuration";
                qpick.items = picks;
                qpick.activeItems = currentConfig ? [picks[currentConfig["id"] + 1]] : [picks[0]];

                qpick.onDidChangeSelection(selection => {
                    selectedConfiguration = selection[0];
                });

                qpick.onDidTriggerItemButton(buttonEvent => {
                    if (buttonEvent.button.iconPath == gearIcon) {
                        let buttonConfigId = (buttonEvent.item == currentConfigItem) ? currentConfig["id"] : configMap.get(buttonEvent.item);
                        try{
                            ConfigurationWebView.render(context, Number(buttonConfigId));
                        }
                        catch (error) {
                            odooOutputChannel.appendLine(error)
                            displayCrashMessage(context, error, odooOutputChannel, 'render.ConfigurationWebView')
                        }
                    }
                });

                qpick.onDidAccept(async () => {
                    if (selectedConfiguration == addConfigItem) {
                        try {
                            await addNewConfiguration(context);
                        }
                        catch (error) {
                            odooOutputChannel.appendLine(error)
                            displayCrashMessage(context, error, odooOutputChannel, 'render.ConfigurationWebView')
                        }
                    }
                    else if (selectedConfiguration == disabledItem) {
                        changeSelectedConfig(context, -1);
                    }
                    else if (selectedConfiguration && selectedConfiguration != currentConfigItem) {
                        changeSelectedConfig(context, configMap.get(selectedConfiguration));
                    }
                    qpick.hide();
                });
                qpick.onDidHide(() => qpick.dispose());
                qpick.show();
            }
            catch (error) {
                odooOutputChannel.appendLine(error)
                displayCrashMessage(context, error, odooOutputChannel, 'odoo.clickStatusBar')
            }
        })
    );
    // Listen to changes to Configurations
    context.subscriptions.push(
        ConfigurationsChange.event((changes: Array<String> | null) => {
            try {
                setStatusConfig(context, odooStatusBar);
                if (changes && (changes.includes('odooPath') || changes.includes('addons'))) {
                    checkOdooPath(context);
                    checkAddons(context,odooOutputChannel);
                    if (client.diagnostics) client.diagnostics.clear();
                    client.sendNotification("Odoo/configurationChanged");
                }
                if (changes && changes.includes('pythonPath')) {
                    checkRestartPythonServer()
                    if (checkPythonDependencies(pythonPath)) {
                        client.sendNotification("Odoo/configurationChanged");
                    } else {
                        isLoading = false;
                        setStatusConfig(context, odooStatusBar);

                    }
                }
            }
            catch (error) {
                odooOutputChannel.appendLine(error)
                displayCrashMessage(context, error, odooOutputChannel, 'event.ConfigurationsChange')
            }
        })
    );

    // Listen to changes to the selected Configuration
    context.subscriptions.push(
        selectedConfigurationChange.event(() => {
            try {
                if (getCurrentConfig(context)) {
                    checkRestartPythonServer()
                    if (checkPythonDependencies(pythonPath)) {
                        if (!client.isRunning()) {
                            client.start().then(() => {
                                client.sendNotification(
                                    "Odoo/clientReady",
                                );
                            });
                        } else {
                            if (client.diagnostics) client.diagnostics.clear();
                            client.sendNotification("Odoo/configurationChanged");
                        }
                    }
                    else {
                        isLoading = false;
                    }
                } else {
                    if (client.isRunning()) client.stop();
                    isLoading = false;
                }
                setStatusConfig(context, odooStatusBar);
            }
            catch (error) {
                odooOutputChannel.appendLine(error)
                displayCrashMessage(context, error, odooOutputChannel, 'event.selectedConfigurationChange')
            }
        })
    );

    // COMMANDS
    context.subscriptions.push(
        commands.registerCommand("odoo.openWelcomeView", () => {
            try {
                WelcomeWebView.render(context);
            }
            catch (error) {
                odooOutputChannel.appendLine(error)
                displayCrashMessage(context, error, odooOutputChannel, 'odoo.openWelcomeView')
            }
        })
    );

    context.subscriptions.push(
        commands.registerCommand("odoo.clearState", () => {
            try {
                for (let key of context.globalState.keys()) {
                    odooOutputChannel.appendLine(`[INFO] Wiping ${key} from global storage.`);
                    context.globalState.update(key, undefined);
                }

                for (let key of context.workspaceState.keys()) {
                    odooOutputChannel.appendLine(`[INFO] Wiping ${key} from workspace storage.`);
                    context.workspaceState.update(key, undefined);
                }
                commands.executeCommand("workbench.action.reloadWindow");
            }
            catch (error) {
                odooOutputChannel.appendLine(error)
                displayCrashMessage(context, error, odooOutputChannel, 'odoo.clearState')
            }
        }
        ));

    context.subscriptions.push(
        client.onNotification("Odoo/loadingStatusUpdate", (state: String) => {
            switch (state) {
                case "start":
                    isLoading = true;
                    break;
                case "stop":
                    isLoading = false;
                    break;
            }
            setStatusConfig(context, odooStatusBar);
        })
    );
    context.subscriptions.push(client.onRequest("Odoo/getConfiguration", (params) => {
        return getCurrentConfig(context);
    }));

    context.subscriptions.push(client.onNotification("Odoo/displayCrashNotification", (params) => {
        displayCrashMessage(context, params["crashInfo"], odooOutputChannel);
    }));

}
export function activate(context: ExtensionContext): void {
    const odooOutputChannel: OutputChannel = window.createOutputChannel('Odoo', 'python');
    try {
        let pythonPath = getPythonPath(context);
        let client = startLanguageServerClient(context, pythonPath, odooOutputChannel);
        deleteOldFiles(context, odooOutputChannel)
        odooOutputChannel.appendLine('[INFO] Starting the extension.');

        if (getCurrentConfig(context)) {
            if (context.extensionMode === ExtensionMode.Production) {
                if (checkPythonDependencies(pythonPath)) {
                    client.start();
                }
            } else {
                client.start();
            }
        }

        // new ConfigurationsExplorer(context);

        initializeSubscriptions(context, client, odooOutputChannel)
        // Initialize some settings on the extension's launch if they're missing from the state.
        setMissingStateVariables(context, odooOutputChannel);
        validateState(context, odooOutputChannel)

        switch (context.globalState.get('Odoo.displayWelcomeView', null)) {
            case null:
                context.globalState.update('Odoo.displayWelcomeView', true);
                WelcomeWebView.render(context);
                break;
            case true:
                WelcomeWebView.render(context);
                break;
        }

        const config = getCurrentConfig(context);
        if (config) {
            odooStatusBar.text = `Odoo (${config["name"]})`;
        }

        if (getCurrentConfig(context)) {
            if (context.extensionMode === ExtensionMode.Production) {
                if (checkPythonDependencies(pythonPath, false)) {
                    client.sendNotification(
                        "Odoo/clientReady",
                    );
                }
            } else {
                client.sendNotification(
                    "Odoo/clientReady",
                );
            }
        }
    }
    catch (error) {
        odooOutputChannel.appendLine(error)
        displayCrashMessage(context, error, odooOutputChannel, 'odoo.activate')
    }
}

export function deactivate(context:ExtensionContext): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    if (client.diagnostics) client.diagnostics.clear();
    if (client.isRunning()) client.stop();
    return client.dispose();
}
