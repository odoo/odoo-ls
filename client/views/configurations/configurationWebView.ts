import { Disposable, Webview, WebviewPanel, window, Uri, ViewColumn } from "vscode";
import { getUri, getNonce } from "../../utils/utils";
import {ConfigurationsChange} from "../../utils/events"
import * as ejs from "ejs";
import * as vscode from 'vscode';
import * as fs from 'fs';
import { URI } from "vscode-languageclient";
import * as readline from 'readline';

/**
 * This class manages the state and behavior of ConfigurationWebView webview panels.
 *
 * It contains all the data and methods for:
 *
 * - Creating and rendering ConfigurationWebView webview panels
 * - Properly cleaning up and disposing of webview resources when the panel is closed
 * - Setting the HTML (and by proxy CSS/JavaScript) content of the webview panel
 * - Setting message listeners so data can be passed between the webview and extension
 */
export class ConfigurationWebView {
    public static panels: Map<number, ConfigurationWebView> | undefined;
    public static readonly viewType = 'odooConfiguration';
    public configId: number | undefined;
    private readonly _panel: WebviewPanel;
    private _disposables: Disposable[] = [];
    private readonly _context: vscode.ExtensionContext

    /**
     * The ConfigurationWebView class private constructor (called only from the render method).
     *
     * @param panel A reference to the webview panel
     * @param extensionUri The URI of the directory containing the extension
     */
    private constructor(panel: WebviewPanel, configId: number, context: vscode.ExtensionContext) {
        this._panel = panel;
        this._context = context;
        this.configId = configId;

        // Set an event listener to listen for when the panel is disposed (i.e. when the user closes
        // the panel or when the panel is closed programmatically)
        this._panel.onDidDispose(this.dispose, this, this._disposables);

        // Set the HTML content for the webview panel
        this._panel.webview.html = this._getWebviewContent(this._panel.webview, context.extensionUri);

        // Set an event listener to listen for messages passed from the webview context
        this._setWebviewMessageListener(this._panel.webview);
    }

    /**
     * Renders the current webview panel if it exists otherwise a new webview panel
     * will be created and displayed.
     *
     * @param extensionUri The URI of the directory containing the extension.
     */
    public static render(context: vscode.ExtensionContext, configId: number) {
        if (!ConfigurationWebView.panels) {
            ConfigurationWebView.panels = new Map();
        }
        if (ConfigurationWebView.panels.has(configId)) {
            // If a webview panel already exists for a config ID, reveal it
            ConfigurationWebView.panels.get(configId)._panel.reveal(vscode.ViewColumn.One);
        } else {
            // If a webview panel does not already exist create and show a new one
            const configName = context.globalState.get("Odoo.configurations")[configId]["name"];
            const panel = window.createWebviewPanel(
                // Panel view type
                "showConfigurationPanel",
                // Panel title
                `Odoo: ${configName}`,
                // The editor column the panel should be displayed in
                vscode.ViewColumn.One,
                // Extra panel configurations
                {
                    // Enable JavaScript in the webview
                    enableScripts: true,
                }
            );
            ConfigurationWebView.panels.set(configId, new ConfigurationWebView(panel, configId, context));
        }
    }

    /**
     * Cleans up and disposes of webview resources when the webview panel is closed.
     */
    public dispose() {
        ConfigurationWebView.panels.delete(this.configId);
        // Dispose of the current webview panel
        this._panel.dispose();

        // Dispose of all disposables (i.e. commands) for the current webview panel
        while (this._disposables.length) {
            const disposable = this._disposables.pop();
            if (disposable) {
                disposable.dispose();
            }
        }
    }


    /**
     * Defines and returns the HTML that should be rendered within the webview panel.
     *
     * @param webview A reference to the extension webview
     * @param extensionUri The URI of the directory containing the extension
     * @returns A template string literal containing the HTML that should be
     * rendered within the webview panel
     */
    private _getWebviewContent(webview: Webview, extensionUri: Uri) {
        const webviewElementsUri = getUri(webview, extensionUri, ["node_modules", "@bendera", "vscode-webview-elements", "dist", "bundled.js"]);
        const htmlPath = getUri(webview, extensionUri, ["client", "views", "configurations", "configurationWebView.html"]);
        const styleUri = getUri(webview, extensionUri, ["client", "views", "configurations", "style.css"]);
        const codiconStyleUri = getUri(webview, extensionUri, ["node_modules", "@vscode", "codicons", "dist", "codicon.css"]);
        const mainUri = getUri(webview, extensionUri, ["client", "views", "configurations", "configurationWebView.js"]);
        const config = this._context.globalState.get("Odoo.configurations")[this.configId];
        const htmlFile = fs.readFileSync(htmlPath.fsPath, 'utf-8');
        const nonce = getNonce();
        const configsVersion: Map<String, String> = this._context.globalState.get("Odoo.configsVersion");

        let data = {
            webviewElementsUri: webviewElementsUri,
            styleUri: styleUri,
            codiconStyleUri: codiconStyleUri,
            mainUri: mainUri,
            config: config,
            cspSource: webview.cspSource,
            nonce: nonce,
            odooVersion: configsVersion ? configsVersion[`${this.configId}`] : 'No Odoo version found.'
        };
        return ejs.render(htmlFile, data);
    }

    private _saveConfig(configs: any, odooPath: String, name: String, addons: Array<String>): void {
        configs[this.configId] = {
            "id": this.configId,
            "name": name,
            "odooPath": odooPath,
            "addons": addons
        };
        this._context.globalState.update("Odoo.configurations", configs);
        ConfigurationsChange.fire(null);
    }

    /**
     * Sets up an event listener to listen for messages passed from the webview context and
     * executes code based on the message that is recieved.
     *
     * @param webview A reference to the extension webview
     * @param context A reference to the extension context
     */
    private _setWebviewMessageListener(webview: Webview) {
        webview.onDidReceiveMessage((message: any) => {
            const command = message.command;
            const configs: any = this._context.globalState.get("Odoo.configurations");

            switch (command) {
                case "save_config":
                    const odooPath = message.odooPath;
                    const name = message.name;
                    const addons = message.addons;
                    this._saveConfig(configs, odooPath, name, addons);
                    break;
                case "view_ready":
                    webview.postMessage({
                        command: 'render_addons',
                        addons: configs[this.configId]["addons"]
                    });
                    break;
                case "open_odoo_folder":
                    const odooFolderOptions: vscode.OpenDialogOptions = {
                        title: "Add Odoo folder",
                        openLabel: 'Add folder',
                        canSelectMany: false,
                        canSelectFiles: false,
                        canSelectFolders: true
                    };
                    window.showOpenDialog(odooFolderOptions).then(fileUri => {
                        if (fileUri && fileUri[0]) {
                            let config = configs[this.configId];
                            const odooFolderPath = fileUri[0].fsPath;
                            this._saveConfig(configs, odooFolderPath, config["name"], config["addons"]);
                            webview.postMessage({
                                command: "update_path",
                                path: odooFolderPath
                            });
                            this._getOdooVersion(odooFolderPath, webview);
                        }
                    });
                    break;
                case "add_addons_folder":
                    const addonsFolderOptions: vscode.OpenDialogOptions = {
                        title: "Add addons folder",
                        openLabel: 'Add folder',
                        canSelectMany: false,
                        canSelectFiles: false,
                        canSelectFolders: true
                    };
                    window.showOpenDialog(addonsFolderOptions).then(fileUri => {
                        if (fileUri && fileUri[0]) {
                            let config = configs[this.configId];
                            const newAddons = [...config["addons"], fileUri[0].fsPath];
                            this._saveConfig(configs, config["odooPath"], config["name"], newAddons);
                            webview.postMessage({
                                command: "render_addons",
                                addons: newAddons
                            });
                        }
                    });
                    break;
            }
        },
            undefined,
            this._disposables
        );
    }

    private _getOdooVersion(odooPath: URI, webview: Webview) {
        let versionString = null;
        const releasePath = odooPath + '/odoo/release.py';
        if (fs.existsSync(releasePath)) {
            const rl = readline.createInterface({
                input: fs.createReadStream(releasePath),
                crlfDelay: Infinity,
            });

            rl.on('line', (line) => {
                if (line.startsWith('version_info')) {
                    versionString = line;
                    rl.close();
                }
            });
            rl.on('close', () => {
                // Folder is invalid if we don't find any version info
                if (!versionString) {
                    let versions = this._context.globalState.get('Odoo.configsVersion', {});
                    versions[`${this.configId}`] = null;
                    this._context.globalState.update('Odoo.configsVersion', versions);
                    webview.postMessage({
                        command: "update_config_folder_validity",
                        version: null
                    });
                } else {
                    // Folder is valid if a version was found
                    const versionRegEx = /\(([^)]+)\)/; // Regex to obtain the info in the parentheses
                    const versionArray = versionRegEx.exec(versionString)[1].split(', ');
                    const version = `${versionArray[0]}.${versionArray[1]}.${versionArray[2]}` + (versionArray[3] == 'FINAL' ? '' : ` ${versionArray[3]}${versionArray[4]}`);
                    let versions = this._context.globalState.get('Odoo.configsVersion', {});
                    versions[`${this.configId}`] = version;
                    this._context.globalState.update('Odoo.configsVersion', versions);
                    webview.postMessage({
                        command: "update_config_folder_validity",
                        version: version
                    });
                }
            });
        } else {
            // Folder is invalid if odoo/release.py was never found
            let versions = this._context.globalState.get('Odoo.configsVersion', {});
            versions[`${this.configId}`] = null;
            this._context.globalState.update('Odoo.configsVersion', versions);
            webview.postMessage({
                command: "update_config_folder_validity",
                version: null
            });
        }
    }
}