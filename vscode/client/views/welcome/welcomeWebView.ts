import { Disposable, Webview, WebviewPanel, window, Uri, ViewColumn } from "vscode";
import { getNonce, getUri } from "../../common/utils";
import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import * as ejs from "ejs";


export class WelcomeWebView {
    public static currentPanel: WelcomeWebView | undefined;
    private readonly _panel: WebviewPanel;
    private readonly _context: vscode.ExtensionContext;
    private _disposables: Disposable[] = [];
    private htmlContent: string;
    private htmlAlertContent: string;

    private constructor(panel: WebviewPanel, context: vscode.ExtensionContext) {
        this._panel = panel;

        this._context = context;
        const htmlPath: vscode.Uri = vscode.Uri.file(path.join(context.extensionPath, 'client', 'views', 'welcome', 'welcomeWebView.html'));
        this.htmlContent = fs.readFileSync(htmlPath.fsPath, 'utf8');
        const alertHtmlPath: vscode.Uri = vscode.Uri.file(path.join(context.extensionPath, 'client', 'views', 'welcome', 'welcomeAlertView.html'));
        this.htmlAlertContent = fs.readFileSync(alertHtmlPath.fsPath, 'utf8');
        // Set an event listener to listen for when the panel is disposed (i.e. when the user closes
        // the panel or when the panel is closed programmatically)
        this._panel.onDidDispose(this.dispose, this, this._disposables);

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
            let panel = window.createWebviewPanel(
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

        const htmlPath = getUri(webview, extensionUri, ["client", "views", "welcome", "welcomeWebView.html"]);
        const htmlFile = fs.readFileSync(htmlPath.fsPath, 'utf-8');
        const alertPath = getUri(webview, extensionUri, ["client", "views", "welcome", "welcomeAlertView.html"]);
        let alertData;
        const alertFile = fs.readFileSync(alertPath.fsPath, 'utf-8');
        const styleUri = getUri(webview, extensionUri, ["client", "views", "welcome", "style.css"]);
        const mainUri = getUri(webview, extensionUri, ["client", "views", "welcome", "welcomeWebView.js"]);
        const defaultState = this._context.globalState.get('Odoo.displayWelcomeView', false);
        const nonce = getNonce();

        if (this._context.extension.packageJSON.version.includes("alpha") || this._context.extension.packageJSON.version.includes("beta") ||
            this._context.extension.packageJSON.version.split(".")[1] % 2 == 0) {
            alertData = {
                version: this._context.extension.packageJSON.version
            }
        } else {
            alertData = null;
        }

        const help_1 = getUri(webview, extensionUri, ['images', 'help_1.png']);
        const help_2 = getUri(webview, extensionUri, ['images', 'help_2.png']);
        const help_3 = getUri(webview, extensionUri, ['images', 'help_3.png']);
        const help_4 = getUri(webview, extensionUri, ['images', 'help_4.png']);

        const data = {
            styleUri: styleUri,
            mainUri: mainUri,
            toolkitUri: toolkitUri,
            cspSource: webview.cspSource,
            alertHTML: alertData ? ejs.render(alertFile, alertData) : '',
            image_1: help_1,
            image_2: help_2,
            image_3: help_3,
            image_4: help_4,
            displayOnStartupCheckbox: defaultState ? 'checked': '',
            nonce: nonce,
        }

        return ejs.render(htmlFile, data);
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
        this,
        this._disposables
        );
    }
}
