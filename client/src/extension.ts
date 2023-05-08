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
    StatusBarAlignment,
    StatusBarItem,
    workspace,
    window
} from "vscode";
import { ConfigurationsExplorer } from './treeConfigurations';
import { TreeDatabasesDataProvider } from './treeDatabases';
import {
    ConfigurationItem,
    integer,
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from "vscode-languageclient/node";
import { WelcomeWebView } from "./welcomeWebView";

let client: LanguageClient;
let odooStatusBar: StatusBarItem;

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
    const configs: Array<Object> = workspace.getConfiguration("Odoo").get("userDefinedConfigurations");
    const configMap = new Map();
    for (const configId in configs) configMap.set(configs[configId]["name"], configId);
    const confPick = await window.showQuickPick(
        Array.from(configMap.keys()),
        {
            title: 'Select a configuration'
        }
    );
    if (confPick) {
        await confPick.resolve(
            workspace.getConfiguration("Odoo").update("selectedConfigurations", configMap.get(confPick), ConfigurationTarget.Global)
        )
    }
});


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
            // small hack to make Pylance import odoo modules in other workspaces
            //TODO only do it if addon directory is detected and do it for each root folder if multiple addons paths
            if (workspace.getConfiguration("python.analysis")) {
                const currentExtraPaths = workspace.getConfiguration("python.analysis").extraPaths;
                if (currentExtraPaths.indexOf(config["odooPath"]) == -1) {
                    //workspace.workspaceFolders.inspect() can help ?
                    workspace.getConfiguration("python.analysis").update("extraPaths", currentExtraPaths.concat(config["odooPath"]), ConfigurationTarget.Workspace);
                }
            }
            //TODO this is not calling anything...
			client.sendNotification("Odoo/initWorkspace", [config["odooPath"]]);
		}
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
    statusItem.text = (config ? `Odoo (${config["name"]})`:`Odoo`);
}
