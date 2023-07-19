import { Disposable, Webview, WebviewPanel, window, Uri, ViewColumn } from "vscode";
import { getUri } from "../../utils/utils";
import * as vscode from 'vscode';


export class WelcomeWebView {
    public static currentPanel: WelcomeWebView | undefined;
    private readonly _panel: WebviewPanel;
    private readonly _context: vscode.ExtensionContext;
    private _disposables: Disposable[] = [];

    private constructor(panel: WebviewPanel, context: vscode.ExtensionContext) {
        this._panel = panel;

        this._context = context;
        // Set an event listener to listen for when the panel is disposed (i.e. when the user closes
        // the panel or when the panel is closed programmatically)
        this._panel.onDidDispose(this.dispose, null, this._disposables);

        // Set the HTML content for the webview panel
        this._panel.webview.html = this._getWebviewContent(this._panel.webview, this._context.extensionUri);

        this._setWebviewMessageListener(this._panel.webview, this._context);
    }

    /**
     * Renders the current webview panel if it exists otherwise a new webview panel
     * will be created and displayed.
     */
    public static render(context: vscode.ExtensionContext) {
        if (WelcomeWebView.currentPanel) {
            WelcomeWebView.currentPanel._panel.reveal(ViewColumn.One);
            WelcomeWebView.currentPanel._panel.webview.html = WelcomeWebView.currentPanel._getWebviewContent(WelcomeWebView.currentPanel._panel.webview, context.extensionUri);
        } else {
            const panel = window.createWebviewPanel(
                "showWelcomePanel",
                "Welcome to Odoo",
                ViewColumn.One,
                {
                    enableScripts: true,
                }
            );
            panel.iconPath = vscode.Uri.joinPath(context.extensionUri, "images", "odoo_favicon");
            WelcomeWebView.currentPanel = new WelcomeWebView(panel, context);
        }
    }

    /**
     * Cleans up and disposes of webview resources when the webview panel is closed.
     */
    public dispose() {
        WelcomeWebView.currentPanel = undefined;
        this._panel.dispose();

        while (this._disposables.length) {
            const disposable = this._disposables.pop();
            if (disposable) {
                disposable.dispose();
            }
        }
    }

    /**
     * Defines and returns the HTML that should be rendered within the webview panel.
     */
    private _getWebviewContent(webview: Webview, extensionUri: Uri) {
        const toolkitUri = getUri(webview, extensionUri, [
            "node_modules",
            "@vscode",
            "webview-ui-toolkit",
            "dist",
            "toolkit.js",
        ]);

        const styleUri = getUri(webview, extensionUri, ["client", "views", "welcome", "style.css"]);
        const mainUri = getUri(webview, extensionUri, ["client", "views", "welcome", "welcomeWebView.js"]);
        const defaultState = this._context.globalState.get('Odoo.displayWelcomeView', null);

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
                    <div id="welcome-container">
                        <a href = "https://odoo.com">
                            <img src="https://odoocdn.com/openerp_website/static/src/img/assets/png/odoo_logo.png" id="welcome-logo" />
                        </a>
                        <h1>Welcome to Odoo Extension</h1>
                        <section>                        
                            <h3> More info about how to use extension </h3>
                        </section>
                        <div class="display-welcome-checkbox">
                            <vscode-checkbox id="displayOdooWelcomeOnStart" ${defaultState ? 'checked': ''}>Show Odoo welcome page on startup</vscode-checkbox>
                        </div>
                    </div>
                </body>
            </html>
        `;
    }

    private _setWebviewMessageListener(webview: Webview, context: vscode.ExtensionContext) {
        webview.onDidReceiveMessage(
        (message: any) => {
            const command = message.command;
            const toggled = message.toggled;
            switch (command) {
            case "changeWelcomeDisplayValue":
                context.globalState.update('Odoo.displayWelcomeView', toggled);
                return;
            }
        },
        undefined,
        this._disposables
        );
    }
}
