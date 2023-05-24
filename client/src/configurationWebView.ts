import { Disposable, Webview, WebviewPanel, window, Uri, ViewColumn } from "vscode";
import { getUri } from "./getUri";
import * as vscode from 'vscode';

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
     * @remarks This is also the place where references to CSS and JavaScript files/packages
     * (such as the Webview UI Toolkit) are created and inserted into the webview HTML.
     *
     * @param webview A reference to the extension webview
     * @param extensionUri The URI of the directory containing the extension
     * @returns A template string literal containing the HTML that should be
     * rendered within the webview panel
     */
    private _getWebviewContent(webview: Webview, extensionUri: Uri) {
        const toolkitUri = getUri(webview, extensionUri, [
            "node_modules",
            "@vscode",
            "webview-ui-toolkit",
            "dist",
            "toolkit.js",
        ]);
        const styleUri = getUri(webview, extensionUri, ["client", "webview-ui", "style.css"]);
        const mainUri = getUri(webview, extensionUri, ["client", "webview-ui", "configurationWebView.js"]);
        const config = vscode.workspace.getConfiguration("Odoo").get("userDefinedConfigurations")[ConfigurationWebView.configId];

        // Tip: Install the es6-string-html VS Code extension to enable code highlighting below
        return /*html*/ `
            <!DOCTYPE html>
            <html lang="en">
                <head>
                    <meta charset="UTF-8">
                    <meta name="viewport" content="width=device-width, initial-scale=1.0">
                    <script type="module" src="${toolkitUri}"></script>
                    <script type="module" src="${mainUri}"></script>
                    <link rel="stylesheet" href="${styleUri}">
                    <title>Odoo Configuration ${config["id"]}</title>
                </head>
                <body id="config-body">
                    <h1>Odoo Configuration</h1>
                    <section id="config-form">                        
                        <vscode-text-field id="name" value="${config["name"]}" placeholder="Configuration name">Name</vscode-text-field>
                        <vscode-text-field id="odooPath" value="${config["odooPath"]}" placeholder="Enter the full path to your Odoo directory">Odoo path</vscode-text-field>
                        <vscode-button id="save-config">Save</vscode-button>
                    </section>
                </body>
            </html>
        `;
    }

    /**
     * Sets up an event listener to listen for messages passed from the webview context and
     * executes code based on the message that is recieved.
     *
     * @param webview A reference to the extension webview
     * @param context A reference to the extension context
     */
    private _setWebviewMessageListener(webview: Webview) {
        webview.onDidReceiveMessage(
        (message: any) => {
            const command = message.command;
            const odooPath = message.odooPath;
            const name = message.name;
            const configs: any = vscode.workspace.getConfiguration("Odoo").get("userDefinedConfigurations");

            switch (command) {
            case "save_config":
                configs[ConfigurationWebView.configId] = {
                    "id": ConfigurationWebView.configId,
                    "name": name,
                    "odooPath": odooPath,
                    "addons": []
                };
                vscode.workspace.getConfiguration("Odoo").update("userDefinedConfigurations", configs, vscode.ConfigurationTarget.Global);
                return;
            }
        },
        undefined,
        this._disposables
        );
    }
}
