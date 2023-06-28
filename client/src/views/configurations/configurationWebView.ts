import { Disposable, Webview, WebviewPanel, window, Uri, ViewColumn } from "vscode";
import { getUri } from "../../utils/getUri";
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
    public static currentPanel: ConfigurationWebView | undefined;
    public static configId: number | undefined;
    private readonly _panel: WebviewPanel;
    private _disposables: Disposable[] = [];

    /**
     * The ConfigurationWebView class private constructor (called only from the render method).
     *
     * @param panel A reference to the webview panel
     * @param extensionUri The URI of the directory containing the extension
     */
    private constructor(panel: WebviewPanel, extensionUri: Uri) {
        this._panel = panel;


        // Set an event listener to listen for when the panel is disposed (i.e. when the user closes
        // the panel or when the panel is closed programmatically)
        this._panel.onDidDispose(this.dispose, null, this._disposables);

        // Set the HTML content for the webview panel
        this._panel.webview.html = this._getWebviewContent(this._panel.webview, extensionUri);

        // Set an event listener to listen for messages passed from the webview context
        this._setWebviewMessageListener(this._panel.webview);
    }

    /**
     * Renders the current webview panel if it exists otherwise a new webview panel
     * will be created and displayed.
     *
     * @param extensionUri The URI of the directory containing the extension.
     */
    public static render(extensionUri: Uri, configId: number) {
        if (ConfigurationWebView.currentPanel) {
            // If the webview panel already exists reveal it
            ConfigurationWebView.currentPanel._panel.reveal(ViewColumn.One);
            //update view
            ConfigurationWebView.configId = configId;
            ConfigurationWebView.currentPanel._panel.webview.html = ConfigurationWebView.currentPanel._getWebviewContent(ConfigurationWebView.currentPanel._panel.webview, extensionUri);
        } else {
            // If a webview panel does not already exist create and show a new one
            const panel = window.createWebviewPanel(
            // Panel view type
            "showConfigurationPanel",
            // Panel title
            "Odoo Configurations",
            // The editor column the panel should be displayed in
            ViewColumn.One,
            // Extra panel configurations
            {
                // Enable JavaScript in the webview
                enableScripts: true,
            }
            );

            ConfigurationWebView.configId = configId;
            ConfigurationWebView.currentPanel = new ConfigurationWebView(panel, extensionUri);
        }
    }

    /**
     * Cleans up and disposes of webview resources when the webview panel is closed.
     */
    public dispose() {
        ConfigurationWebView.currentPanel = undefined;
        ConfigurationWebView.configId = undefined;

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
        const htmlPath = getUri(webview, extensionUri, ["client", "src", "views", "configurations", "configurationWebView.html"]);
        const styleUri = getUri(webview, extensionUri, ["client", "src", "views", "configurations", "style.css"]);
        const codiconStyleUri = getUri(webview, extensionUri, ["node_modules", "@vscode", "codicons", "dist", "codicon.css"]);
        const mainUri = getUri(webview, extensionUri, ["client", "src", "views", "configurations", "configurationWebView.js"]);
        const config = vscode.workspace.getConfiguration("Odoo").get("userDefinedConfigurations")[ConfigurationWebView.configId];
        const htmlFile = fs.readFileSync(htmlPath.fsPath, 'utf-8');
        
        let data = {
            webviewElementsUri: webviewElementsUri,
            styleUri: styleUri,
            codiconStyleUri: codiconStyleUri,
            mainUri: mainUri,
            config: config
        };
        return ejs.render(htmlFile, data);
    }

    private _saveConfig(configs: any, odooPath: String, name: String, addons: Array<String>): void {
        configs[ConfigurationWebView.configId] = {
            "id": ConfigurationWebView.configId,
            "name": name,
            "odooPath": odooPath,
            "addons": addons
        };
        vscode.workspace.getConfiguration("Odoo").update("userDefinedConfigurations", configs, vscode.ConfigurationTarget.Global);
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
            const configs: any = vscode.workspace.getConfiguration("Odoo").get("userDefinedConfigurations");

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
                    addons: configs[ConfigurationWebView.configId]["addons"]
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
                        let config = configs[ConfigurationWebView.configId];
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
                        let config = configs[ConfigurationWebView.configId];
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
                    webview.postMessage({
                        command: "update_config_folder_validity",
                        version: null
                    });
                } else {
                // Folder is valid if a version was found
                    const versionRegEx = /\(([^)]+)\)/; // Regex to obtain the info in the parentheses
                    const versionArray = versionRegEx.exec(versionString)[1].split(', ');
                    const version = `${versionArray[0]}.${versionArray[1]}.${versionArray[2]}` + (versionArray[3] == 'FINAL' ? '' : ` ${versionArray[3]}${versionArray[4]}`);
                    webview.postMessage({
                        command: "update_config_folder_validity",
                        version: version
                    });
                }
            });
        } else {
            // Folder is invalid if odoo/release.py was never found
            webview.postMessage({
                command: "update_config_folder_validity",
                version: null
            });
        }
    }
}
