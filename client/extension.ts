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
} from "vscode";
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    integer,
} from "vscode-languageclient/node";
import { WelcomeWebView } from "./views/welcome/welcomeWebView";
import { ConfigurationWebView } from './views/configurations/configurationWebView';
import {
    selectedConfigurationChange,
    ConfigurationsChange
} from './utils/events'

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
        outputChannelName: "Odoo",
        synchronize: {
            // Notify the server about file changes to '.clientrc files contain in the workspace
            fileEvents: workspace.createFileSystemWatcher("**/.clientrc"),
        },
    };
}

function startLangServerTCP(addr: number): LanguageClient {
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

    return new LanguageClient(
        `tcp lang server (port ${addr})`,
        serverOptions,
        getClientOptions()
    );
}

function startLangServer(
    command: string,
    args: string[],
    cwd: string
): LanguageClient {
    const serverOptions: ServerOptions = {
        args,
        command,
        options: { cwd },
    };

    return new LanguageClient(command, serverOptions, getClientOptions());
}

function getCurrentConfig(context: ExtensionContext) {
    const configs: any = context.globalState.get("Odoo.configurations");
    const activeConfig: number = Number(context.workspaceState.get('Odoo.selectedConfiguration'));
    return (configs && activeConfig > -1 ? configs[activeConfig] : null);
}

function setStatusConfig(context: ExtensionContext, statusItem: StatusBarItem) {
    const config = getCurrentConfig(context);
    let text = (config ? `Odoo (${config["name"]})`:`Odoo`);
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

export function activate(context: ExtensionContext): void {
    if (context.extensionMode === ExtensionMode.Development) {
        // Development - Run the server manually
        client = startLangServerTCP(2087);
    } else {
        // Production - Client is going to run the server (for use within `.vsix` package)
        const cwd = path.join(__dirname, "..", "..");
        const pythonPath = workspace
            .getConfiguration("python")
            .get<string>("interpreterPath");

        if (!pythonPath) {
            throw new Error("`python.interpreterPath` is not set");
        }

        client = startLangServer(pythonPath, ["-m", "server"], cwd);
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
            const separator = {kind: -1};
            const addConfigItem  = {
                label: "$(add) Add new configuration"
            };
            const gearIcon = new ThemeIcon("gear");
        
            for (const configId in configs) {
                if (currentConfig && configId == currentConfig["id"])
                    continue; 
                configMap.set({"label": configs[configId]["name"], "buttons": [{iconPath: gearIcon}]}, configId)
            }
            
            let picks = Array.from(configMap.keys());
            if (picks.length)
                picks.push(separator);

            if (currentConfig) {
                currentConfigItem = {"label": currentConfig["name"], "description": "(current)", "buttons": [{iconPath: gearIcon}]};
                picks.splice(currentConfig["id"], 0, currentConfigItem);
            }
            
            picks.push(addConfigItem);
            qpick.title = "Select a configuration";
            qpick.items = picks;
            qpick.activeItems = currentConfig ? [picks[currentConfig["id"]]] : [];

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
                else if (selectedConfiguration && selectedConfiguration != currentConfigItem) {
                    context.workspaceState.update("Odoo.selectedConfiguration", configMap.get(selectedConfiguration));
                    selectedConfigurationChange.fire(null);
                }
                qpick.hide();
            });
            qpick.onDidHide(() => qpick.dispose());
            qpick.show();
        })
    );
    
    /*
    window.registerTreeDataProvider(
		'odoo-databases',
		new TreeDatabasesDataProvider()
	);
	window.createTreeView('odoo-databases', {
		treeDataProvider: new TreeDatabasesDataProvider()
	});*/

    // Listen to changes to Configurations
    context.subscriptions.push(
        ConfigurationsChange.event(() => {
            setStatusConfig(context, odooStatusBar);
        })
    );

    // Listen to changes to the selected Configuration
    context.subscriptions.push(
        selectedConfigurationChange.event(() => {
            setStatusConfig(context, odooStatusBar);
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
    });

    client.sendNotification(
        "Odoo/clientReady",
        {"config": getCurrentConfig(context)}
    );
    client.start();
}

export function deactivate(): Thenable<void> | undefined {
	if (!client) {
		return undefined;
	}
	return client.stop();
}