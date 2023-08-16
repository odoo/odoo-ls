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

let client: LanguageClient;
let odooStatusBar: StatusBarItem;
let isLoading: boolean;

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

function getCurrentConfig(context: ExtensionContext) {
    const configs: any = context.globalState.get("Odoo.configurations");
    const activeConfig: number = Number(context.workspaceState.get('Odoo.selectedConfiguration'));
    return (configs && activeConfig > -1 ? configs[activeConfig] : null);
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
            [configId]: {
                "id": configId,
                "name": `New Configuration ${configId}`,
                "odooPath": "",
                "addons": []
            }
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

async function displayCrashMessage(context: ExtensionContext, crashInfo: string, outputChannel: OutputChannel) {
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
            CrashReportWebView.render(context, activeFile, crashInfo);
            break
        case ("Open logs"):
            outputChannel.show();
            break
    }
}

function startLanguageServerClient(context: ExtensionContext, pythonPath:string, outputChannel: OutputChannel) {
    let client: LanguageClient
    if (context.extensionMode === ExtensionMode.Development) {
        // Development - Run the server manually
        client = startLangServerTCP(2087, outputChannel);
        context.subscriptions.push(commands.registerCommand("odoo.testCrashMessage", () => { displayCrashMessage(context, "Test crash message", outputChannel); }));
    } else {
        // Production - Client is going to run the server (for use within `.vsix` package)
        const cwd = path.join(__dirname, "..", "..");

        if (!pythonPath) {
            outputChannel.appendLine("[INFO] Odoo.pythonPath is not set, defaulting to python3.");
        }
        client = startLangServer(pythonPath ? pythonPath: "python3", ["-m", "server"], cwd, outputChannel);
    }

    return client;
}

export function activate(context: ExtensionContext): void {
    const odooOutputChannel: OutputChannel = window.createOutputChannel('Odoo', 'python');
    let pythonPath = workspace.getConfiguration("Odoo").get<string>("pythonPath", "python3");
    pythonPath = pythonPath == '' ? "python3" : pythonPath;
    let client = startLanguageServerClient(context, pythonPath, odooOutputChannel);

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

    odooStatusBar = window.createStatusBarItem(StatusBarAlignment.Left, 100);
    setStatusConfig(context, odooStatusBar);
    odooStatusBar.show();
    odooStatusBar.command = "odoo.clickStatusBar"
    context.subscriptions.push(odooStatusBar);

    // Initialize some settings on the extension's first launch.
    if (context.globalState.get("Odoo.firstLaunch", true)) {
        context.globalState.update("Odoo.configurations", {});
        context.globalState.update('Odoo.nextConfigId', 0);
        context.globalState.update("Odoo.firstLaunch", 0);
    }

    if (context.workspaceState.get("Odoo.selectedConfiguration", null) == null) {
        context.workspaceState.update("Odoo.selectedConfiguration", -1);
    }

    context.subscriptions.push(
        commands.registerCommand('odoo.clickStatusBar', async () => {
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
                    ConfigurationWebView.render(context, Number(buttonConfigId));
                }
            });

            qpick.onDidAccept(async () => {
                if (selectedConfiguration == addConfigItem) {
                    await addNewConfiguration(context);
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
        })
    );

    // Listen to changes to Configurations
    context.subscriptions.push(
        ConfigurationsChange.event((changes: Array<String> | null) => {
            setStatusConfig(context, odooStatusBar);
            if (changes && (changes.includes('odooPath') || changes.includes('addons'))) {
                client.diagnostics.clear();
                client.sendNotification("Odoo/configurationChanged");
            }
        })
    );

    // Listen to changes to the selected Configuration
    context.subscriptions.push(
        selectedConfigurationChange.event(() => {
            setStatusConfig(context, odooStatusBar);
            if (!checkPythonDependencies(pythonPath)) return;
            if (getCurrentConfig(context)) {
                if (!client.isRunning()) {
                    client.start().then(() => {
                        client.sendNotification(
                            "Odoo/clientReady",
                        );
                    });
                } else {
                    client.diagnostics.clear();
                    client.sendNotification("Odoo/configurationChanged");
                }
            } else if (client.isRunning()) {
                client.stop();
                isLoading = false;
            }
        })
    );

    // Temporary. Ideally I'd dispose the current client and regenerate a new one
    // but it would require far too much effort for what it achieves.
    context.subscriptions.push(
        workspace.onDidChangeConfiguration(async event => {
            let affected = event.affectsConfiguration("Odoo.pythonPath");
            if (affected) {
                window.showInformationMessage(
                    "Odoo: Modifying Odoo.pythonPath requires a reload for the change to take effect.",
                    "Reload VSCode",
                    "Later"
                ).then(selection => {
                    switch (selection) {
                        case ("Reload VSCode"):
                            commands.executeCommand("workbench.action.reloadWindow");
                            break;
                    }
                });
            }
        })
    );

    context.subscriptions.push(
        commands.registerCommand("odoo.openWelcomeView", () => {
            WelcomeWebView.render(context);
        })
    );

    switch (context.globalState.get('Odoo.displayWelcomeView', null)) {
        case null:
            context.globalState.update('Odoo.displayWelcomeView', false);
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

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
