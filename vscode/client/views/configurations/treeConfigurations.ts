import * as vscode from 'vscode';
import * as fs from 'fs';
import { ConfigurationWebView } from './configurationWebView';
import * as path from 'path';


class Configuration extends vscode.TreeItem {
    constructor(
        public readonly label: string,
        private version: string,
        public readonly collapsibleState: vscode.TreeItemCollapsibleState
    ) {
        super(label, collapsibleState);
        this.tooltip = `${this.label}-${this.version}`;
        this.description = this.version;
    }

    configId = -1;
    iconPath = new vscode.ThemeIcon('symbol-method');
}

export class TreeConfigurationsDataProvider implements vscode.TreeDataProvider<Configuration> {

    //TODO FDA private?
    public _onDidChangeTreeData: vscode.EventEmitter<Configuration | undefined | null | void> = new vscode.EventEmitter<Configuration | undefined | null | void>();
    readonly onDidChangeTreeData: vscode.Event<Configuration | undefined | null | void> = this._onDidChangeTreeData.event;

    getTreeItem(element: Configuration): vscode.TreeItem {
        element.command = { command: 'odoo.openConfiguration', title: "Open Configuration", arguments: [element.configId], };
		return element;
    }

    getChildren(element?: Configuration): Thenable<Configuration[]> {
        if (!element) {
            return Promise.resolve(this.get_all_configurations());
        }
    }

    private get_all_configurations(): Configuration[] {
        const configs: any = vscode.workspace.getConfiguration("Odoo").get("userDefinedConfigurations");
        const res = [];
        for (const configId in configs) {
            const confItem = new Configuration(
                configs[configId]["name"],
                "",
                vscode.TreeItemCollapsibleState.None
            );
            confItem.configId = configs[configId]["id"];
            res.push(confItem);
        }
        return res;
    }

    private pathExists(p: string): boolean {
        try {
            fs.accessSync(p);
        } catch (err) {
            return false;
        }
        return true;
    }
}

export class ConfigurationsExplorer {
	constructor(context: vscode.ExtensionContext) {
		const treeDataProvider = new TreeConfigurationsDataProvider();
		//context.subscriptions.push(vscode.window.createTreeView('fileExplorer', { treeDataProvider }));
        vscode.window.registerTreeDataProvider(
            'odoo-configurations',
            treeDataProvider
        );
        /*vscode.window.createTreeView('odoo-databases', {
            treeDataProvider: new TreeConfigurationsDataProvider()
        });*/
        vscode.commands.registerCommand('odoo.addConfiguration', () => {
            const configs: any = vscode.workspace.getConfiguration("Odoo").get("userDefinedConfigurations");
            let freeIndex = -1;
            let found = true;
            while (found) {
                found = false;
                freeIndex ++;
                if (freeIndex in Object.keys(configs)) {
                    found = true;
                }
            }
            configs[freeIndex] = {
                "id": freeIndex,
                "name": "Configuration " + freeIndex,
                "odooPath": "path/to/odoo",
                "addons": []
            };
            vscode.workspace.getConfiguration("Odoo").update("userDefinedConfigurations", configs, vscode.ConfigurationTarget.Global);
            treeDataProvider._onDidChangeTreeData.fire();
        });

        vscode.commands.registerCommand('odoo.openConfiguration', (configId) => {
            const configs: any = vscode.workspace.getConfiguration("Odoo").get("userDefinedConfigurations");
            const config = configs[configId];

            // And set its HTML content
            //panel.webview.html = getWebviewContent();
            ConfigurationWebView.render(context, configId);
        });
        
        vscode.workspace.onDidChangeConfiguration(() => {
			treeDataProvider._onDidChangeTreeData.fire();
		});
	}
}