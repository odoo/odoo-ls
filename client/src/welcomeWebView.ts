import { Disposable, Webview, WebviewPanel, window, Uri, ViewColumn } from "vscode";
import { getUri } from "./getUri";
import * as vscode from 'vscode';
import { cp } from "fs";
/**
 * This class manages the state and behavior of WelcomeWebView webview panels.
 *
 * It contains all the data and methods for:
 *
 * - Creating and rendering WelcomeWebView webview panels
 * - Properly cleaning up and disposing of webview resources when the panel is closed
 * - Setting the HTML (and by proxy CSS/JavaScript) content of the webview panel
 * - Setting message listeners so data can be passed between the webview and extension
 */
export class WelcomeWebView {
    public static currentPanel: WelcomeWebView | undefined;
    private readonly _panel: WebviewPanel;
    private _disposables: Disposable[] = [];

    /**
     * The WelcomeWebView class private constructor (called only from the render method).
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
    }

    /**
     * Renders the current webview panel if it exists otherwise a new webview panel
     * will be created and displayed.
     *
     * @param extensionUri The URI of the directory containing the extension.
     */
    public static render(extensionUri: Uri) {
        if (WelcomeWebView.currentPanel) {
            // If the webview panel already exists reveal it
            WelcomeWebView.currentPanel._panel.reveal(ViewColumn.One);
            //update view
            WelcomeWebView.currentPanel._panel.webview.html = WelcomeWebView.currentPanel._getWebviewContent(WelcomeWebView.currentPanel._panel.webview, extensionUri);
        } else {
            // If a webview panel does not already exist create and show a new one
            const panel = window.createWebviewPanel(
                // Panel view type
                "showWelcomePanel",
                // Panel title
                "Welcome to Odoo",
                // The editor column the panel should be displayed in
                ViewColumn.One,
                // Extra panel configurations
                {
                    // Enable JavaScript in the webview
                    enableScripts: true,
                }
            );
            panel.iconPath = vscode.Uri.joinPath(extensionUri, "images", "odoo_favicon");
            WelcomeWebView.currentPanel = new WelcomeWebView(panel, extensionUri);
        }
    }

    /**
     * Cleans up and disposes of webview resources when the webview panel is closed.
     */
    public dispose() {
        WelcomeWebView.currentPanel = undefined;

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
        const mainUri = getUri(webview, extensionUri, ["client", "webview-ui"]);

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
                </head>
                <body id="welcome-body">
                <div id='welcome-container'>
                    <a href = "https://odoo.com">
                        <img src="https://odoocdn.com/openerp_website/static/src/img/assets/png/odoo_logo.png" id="welcome-logo" />
                    </a>
                    <h1>Welcome to Odoo Extension</h1>
                    <section>                        
                        <h3> More info about how to use extension </h3>
                    </section>
                </div>
                </body>
            </html>
        `;
    }
}