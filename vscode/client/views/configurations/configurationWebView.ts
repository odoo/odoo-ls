import { Disposable, Webview, WebviewPanel, window, Uri, workspace, ConfigurationTarget } from "vscode";
import { getUri, getNonce, evaluateOdooPath, buildFinalPythonPath, validateAddonPath } from "../../common/utils";
import * as ejs from "ejs";
import * as vscode from 'vscode';
import * as fs from 'fs';
import { URI } from "vscode-languageclient";
import untildify from 'untildify';
import { checkStandalonePythonVersion } from "../../extension";

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
    public config;
    public finalPythonPath: String;
    private readonly _panel: WebviewPanel;
    private _disposables: Disposable[] = [];
    private readonly _context: vscode.ExtensionContext
    private addons: Array<String> = [];

    /**
     * The ConfigurationWebView class private constructor (called only from the render method).
     *
     * @param panel A reference to the webview panel
     * @param extensionUri The URI of the directory containing the extension
     */
    private constructor(panel: WebviewPanel, config, context: vscode.ExtensionContext) {
        this._panel = panel;
        this._context = context;
        this.configId = config.id;
        this.config = config;
        this.addons = config.addons;
        this.finalPythonPath = config.finalPythonPath;

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
    public static render(context: vscode.ExtensionContext, config) {
        if (!ConfigurationWebView.panels) {
            ConfigurationWebView.panels = new Map();
        }
        if (ConfigurationWebView.panels.has(config.id)) {
            // If a webview panel already exists for a config ID, reveal it
            ConfigurationWebView.panels.get(config.id)._panel.reveal(vscode.ViewColumn.One);
        } else {
            // If a webview panel does not already exist create and show a new one

            const panel = window.createWebviewPanel(
                // Panel view type
                "showConfigurationPanel",
                // Panel title
                `Odoo: ${config.name}`,
                // The editor column the panel should be displayed in
                vscode.ViewColumn.One,
                // Extra panel configurations
                {
                    // Enable JavaScript in the webview
                    enableScripts: true,
                    retainContextWhenHidden: true,
                }
            );
            ConfigurationWebView.panels.set(config.id, new ConfigurationWebView(panel, config, context));
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
        const webviewElementsUri = getUri(webview, extensionUri, ["node_modules", "@vscode-elements", "elements", "dist", "bundled.js"]);
        const htmlPath = getUri(webview, extensionUri, ["client", "views", "configurations", "configurationWebView.html"]);
        const styleUri = getUri(webview, extensionUri, ["client", "views", "configurations", "style.css"]);
        const codiconStyleUri = getUri(webview, extensionUri, ["node_modules", "@vscode", "codicons", "dist", "codicon.css"]);
        const mainUri = getUri(webview, extensionUri, ["client", "views", "configurations", "configurationWebView.js"]);
        const htmlFile = fs.readFileSync(htmlPath.fsPath, 'utf-8');
        const nonce = getNonce();
        const configsVersion: Map<String, String> = this._context.globalState.get("Odoo.configsVersion", null);

        let data = {
            webviewElementsUri: webviewElementsUri,
            styleUri: styleUri,
            codiconStyleUri: codiconStyleUri,
            mainUri: mainUri,
            config: this.config,
            cspSource: webview.cspSource,
            nonce: nonce,
            odooVersion: configsVersion ? configsVersion[`${this.configId}`] : null,
            pythonExtensionMode: global.IS_PYTHON_EXTENSION_READY,
        };
        return ejs.render(htmlFile, data);
    }

    private _updateWebviewTitle(panel: WebviewPanel, title: string){
        panel.title = `Odoo: ${title}`
    }

    private async _saveConfig(configs: any, rawOdooPath: string, name: string, addons: Array<String>, pythonPath: string = "python3"): Promise<void> {
        let changes = [];
        let oldAddons = configs[this.configId]["addons"]


        if (configs[this.configId]["rawOdooPath"] != rawOdooPath) {
            changes.push("rawOdooPath");
        }

        if (configs[this.configId]["name"] != name) {
            changes.push("name");
        }

        if (configs[this.configId]["pythonPath"] != pythonPath) {
            changes.push("pythonPath");
        }

        if (oldAddons.length != addons.length) {
            changes.push("addons");
        } else {
            oldAddons.sort();
            addons.sort();
            for (let i = 0; i < oldAddons.length; i++) {
                if (oldAddons[i] != addons[i]) {
                    changes.push("addons");
                    break;
                }
            }
        }

        global.OUTPUT_CHANNEL.appendLine("[INFO] saving ".concat(changes.toString()))

        configs[this.configId] = {
            "id": this.configId,
            "name": name,
            "odooPath": configs[this.configId]["odooPath"],
            "rawOdooPath": untildify(rawOdooPath),
            "addons": addons,
            "pythonPath": untildify(pythonPath),
            "validatedAddonsPaths": configs[this.configId]["validatedAddonsPaths"],
            "finalPythonPath": await buildFinalPythonPath(this._context, untildify(pythonPath))
        };
        workspace.getConfiguration().update("Odoo.configurations",configs, ConfigurationTarget.Global);

        if (changes.includes('name')){
            this._updateWebviewTitle(this._panel, name)
        }
    }

    private _deleteConfig(configs: any): void {
        delete configs[this.configId]
        workspace.getConfiguration().update("Odoo.configurations",configs, ConfigurationTarget.Global);
        this.dispose()
    }
    /**
     * Sets up an event listener to listen for messages passed from the webview context and
     * executes code based on the message that is recieved.
     *
     * @param webview A reference to the extension webview
     * @param context A reference to the extension context
     */
    private _setWebviewMessageListener(webview: Webview) {
        webview.onDidReceiveMessage(async (message: any) => {
            const command = message.command;
            const configs: any = JSON.parse(JSON.stringify(workspace.getConfiguration().get("Odoo.configurations")));

            switch (command) {
                case "save_config":
                    const rawOdooPath = message.rawOdooPath;
                    const name = message.name;
                    const addons = message.addons;
                    const pythonPath = message.pythonPath;
                    await this._saveConfig(configs, rawOdooPath, name, addons, pythonPath);
                    break;
                case "view_ready":
                    // Check odooPath, pythonPath and addonsPath on startup
                    await Promise.all([
                        this._verifyRenderAddons(webview),
                        this._verifyPythonPath(this.config.pythonPath, webview),
                        this._verifyPath(this.config.rawOdooPath, webview),
                    ]);
                    break;
                case "open_odoo_folder":
                    const odooFolderOptions: vscode.OpenDialogOptions = {
                        title: "Add Odoo folder",
                        openLabel: 'Add folder',
                        canSelectMany: false,
                        canSelectFiles: false,
                        canSelectFolders: true
                    };
                    window.showOpenDialog(odooFolderOptions).then(async (fileUri) => {
                        if (fileUri && fileUri[0]) {
                            let config = configs[this.configId];
                            const odooFolderPath = fileUri[0].fsPath;
                            webview.postMessage({
                                command: "update_path",
                                path: odooFolderPath
                            });
                            await this._verifyPath(odooFolderPath,webview);
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
                            webview.postMessage({
                                command: "read_addons_folder",
                                addonPath: fileUri[0].fsPath,
                            });
                        }
                    });
                    break;
                case "add_addons_path":
                    const addonPath = message.addonPath;
                    if (!addonPath){
                        break;
                    }
                    this.addons = [...this.addons, addonPath];
                    webview.postMessage({
                        command: "clear_addons_folder",
                    });
                    await this._verifyRenderAddons(webview);
                    break;
                case "delete_addons_folder":
                    this.addons = message.addons;
                    break;
                case "delete_config":
                    this._deleteConfig(configs);
                    break;
                case "update_version":
                    await this._verifyPath(message.rawOdooPath, webview);
                    break;
                case "open_python_path":
                    const pythonPathOptions: vscode.OpenDialogOptions = {
                        title: "Add Python path",
                        openLabel: 'Add path',
                        canSelectMany: false,
                        canSelectFiles: false,
                        canSelectFolders: false,
                    };
                    window.showOpenDialog(pythonPathOptions).then(async fileUri => {
                        if (fileUri && fileUri[0]) {
                            let config = configs[this.configId];
                            const odooPythonPath = fileUri[0].fsPath;
                            webview.postMessage({
                                command: "update_python_path",
                                pythonPath: odooPythonPath
                            });
                            await this._verifyPythonPath(odooPythonPath, webview);
                        }
                    });
                    break;
                case "change_python_path":
                    await this._verifyPythonPath(message.pythonPath, webview);
                    break;

            }
        },
            undefined,
            this._disposables
        );
    }

    private async _verifyPath(rawOdooPath: string, webview: Webview){
        const displayOdooVersion = (version)=>{
            webview.postMessage({
                command: "update_config_folder_validity",
                version: version
            });
        };

        let versions = this._context.globalState.get('Odoo.configsVersion', {});
        const odoo = await evaluateOdooPath(rawOdooPath);
        if (odoo){
            versions[`${this.configId}`] = odoo.version;
            this._context.globalState.update('Odoo.configsVersion', versions);
            let configs: any = JSON.parse(JSON.stringify(workspace.getConfiguration().get("Odoo.configurations")));
            configs[this.configId]["odooPath"] = odoo.path;
            configs[this.configId]["rawOdooPath"] = rawOdooPath;

            workspace.getConfiguration().update("Odoo.configurations",configs, ConfigurationTarget.Global);
            displayOdooVersion(odoo.version);
        }else{
            // no valid odoo found, setting the odoo version to null
            versions[`${this.configId}`] = null;
	        this._context.globalState.update('Odoo.configsVersion', versions);
            displayOdooVersion(null);
        }
    }

    private async _verifyRenderAddons(webview: Webview){
        let validAddons = await Promise.all(this.addons.map(async (addon) => { return (await validateAddonPath(addon) !== null)}));
        webview.postMessage({
            command: "render_addons",
            addons: this.addons,
            validAddons: validAddons
        });
    }

    private async _verifyPythonPath(pythonPath: string, webview: Webview){
        const valid = await checkStandalonePythonVersion(this._context, pythonPath);
        webview.postMessage({
            command: "update_python_path_validity",
            valid: valid
        });
    }
}
