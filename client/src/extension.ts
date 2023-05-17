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
    ConfigurationTarget,
    commands,
    ExtensionContext,
    ExtensionMode,
    QuickPick,
    QuickPickItem,
    StatusBarAlignment,
    StatusBarItem,
    workspace,
    window,
    Uri
} from "vscode";
import { ConfigurationsExplorer } from './treeConfigurations';
import { TreeDatabasesDataProvider } from './treeDatabases';
import {
    ConfigurationItem,
    integer,
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    URI,
} from "vscode-languageclient/node";
import { WelcomeWebView } from "./welcomeWebView";
import { PathLike, PathOrFileDescriptor } from "fs";

let client: LanguageClient;
let odooStatusBar: StatusBarItem;
let oldStatusBarText: string;
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

commands.registerCommand('odoo.clickStatusBar',async () => {
    const qpick = window.createQuickPick();
    const configs: Array<Object> = workspace.getConfiguration("Odoo").get("userDefinedConfigurations");
    let currSelect = null;
    const currentConfig = getCurrentConfig();
    const configMap = new Map();
    const separator = {kind: -1};
    const addConfigItem  = {
        label: "$(add) Add new configuration"
    };

    for (const configId in configs) {
        if (configId == currentConfig["id"]) continue; 
        configMap.set({"label": configs[configId]["name"]}, configId)
    }
    let picks = Array.from(configMap.keys());
    const currentConfigItem = {"label": currentConfig["name"], "description": "(current)"};
    picks.splice(currentConfig["id"], 0, currentConfigItem);
    picks.push(...[separator, addConfigItem]);

    qpick.title = "Select a configuration";
    qpick.items = picks;
    qpick.activeItems = [picks[currentConfig["id"]]];
    qpick.onDidChangeSelection(selection => {
        currSelect = selection[0];
    });

    qpick.onDidAccept(async () => {
        if (currSelect == addConfigItem) {
            await addNewConfiguration()
        }
        else if (currSelect && currSelect != currentConfigItem) {
            workspace.getConfiguration("Odoo").update("selectedConfigurations", Number(configMap.get(currSelect)), ConfigurationTarget.Global)
        }
        qpick.hide();
    })
    qpick.onDidHide(() => qpick.dispose());
    qpick.show();
});

async function addNewConfiguration() {
    const configId = getConfigAmount();
    await window.showInputBox({
        title: "New configuration name",
        value: `New Configuration ${configId}`,
        valueSelection: undefined
    }).then(async (name) => {
        const newConfigName = name;
        if (!newConfigName)
            return;
        await window.showOpenDialog({
            canSelectFiles: false,
            canSelectFolders: true,
            canSelectMany: false,
            openLabel: "Select Folder",
            title: "Select Odoo folder"
        }).then((folderPath) => {
            if (!folderPath)
                return;
            const newConfigPath = folderPath[0].path;
            let configs: Map<integer, any> = workspace.getConfiguration("Odoo").get("userDefinedConfigurations");
            workspace.getConfiguration("Odoo").update("userDefinedConfigurations", {...configs, [configId]: {"id": configId, "name": newConfigName, "odooPath": newConfigPath, "addons": []}}, ConfigurationTarget.Global);
        })
    })
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

    context.subscriptions.push(client.start());

	new ConfigurationsExplorer(context);

    odooStatusBar = window.createStatusBarItem(StatusBarAlignment.Left, 100);
    setStatusConfig(odooStatusBar);
    odooStatusBar.show();
    odooStatusBar.command = "odoo.clickStatusBar"
    context.subscriptions.push(odooStatusBar);

    window.registerTreeDataProvider(
		'odoo-databases',
		new TreeDatabasesDataProvider()
	);/*
	window.createTreeView('odoo-databases', {
		treeDataProvider: new TreeDatabasesDataProvider()
	});*/
    workspace.onDidChangeConfiguration(event => {
        let affected = event.affectsConfiguration("Odoo.selectedConfigurations");
        if (affected) setStatusConfig(odooStatusBar);
    })

    WelcomeWebView.render(context.extensionUri);
	client.onReady().then(() => {
		const config = getCurrentConfig();
		if (config) {
			console.log(config);
            odooStatusBar.text = `Odoo (${config["name"]})`
            //TODO this is not calling anything...
			client.sendNotification("Odoo/initWorkspace", [config["odooPath"]]);
		}

        client.onNotification("Odoo/loading", (state: String) => {
            switch (state) {
                case "start":
                    isLoading = true;
                    break;
                case "stop":
                    isLoading = false;
                    break;
            }
            setStatusConfig(odooStatusBar);
        });

        client.sendNotification("Odoo/clientReady");
	});
}

export function deactivate(): Thenable<void> {
    return client ? client.stop() : Promise.resolve();
}

function getCurrentConfig() {
    const configs: any = workspace.getConfiguration("Odoo").get("userDefinedConfigurations");
    const selectedConfig: integer = workspace.getConfiguration("Odoo").get("selectedConfigurations");
    return (selectedConfig != -1 ? configs[selectedConfig] : null);
}

function setStatusConfig(statusItem: StatusBarItem) {
    const config = getCurrentConfig();
    let text = (config ? `Odoo (${config["name"]})`:`Odoo`);
    statusItem.text = (isLoading) ? "$(loading~spin) " + text : text;
}

function getConfigAmount() {
    const configs: any = workspace.getConfiguration("Odoo").get("userDefinedConfigurations");
    let count = 0;
    for (const configId in configs) count++;
    return count;
}
