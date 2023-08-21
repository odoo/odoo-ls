import { Disposable, Webview, WebviewPanel, window, Uri } from "vscode";
import { getUri, getNonce } from "../../utils/utils";
import axios from 'axios';
import * as ejs from "ejs";
import * as vscode from 'vscode';
import * as fs from 'fs';
import * as crypto from 'crypto';

export class CrashReportWebView {
    public static panels: Map<String, CrashReportWebView> | undefined;
    public static readonly viewType = 'odooCrashReport';
    public readonly UID: String | undefined;
    private readonly _panel: WebviewPanel;
    private _disposables: Disposable[] = [];
    private readonly _context: vscode.ExtensionContext;
    private readonly _document: vscode.TextDocument;
    private readonly _error: String;

    /**
     * The ConfigurationWebView class private constructor (called only from the render method).
     *
     * @param panel A reference to the webview panel
     * @param extensionUri The URI of the directory containing the extension
     */
    private constructor(panel: WebviewPanel, uid: String ,context: vscode.ExtensionContext, document: vscode.TextDocument, error: String) {
        this._panel = panel;
        this._context = context;
        this._document = document;
        this._error = error;
        this.UID = uid;

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
    public static render(context: vscode.ExtensionContext, document: vscode.TextDocument, error: String) {
        if (!CrashReportWebView.panels) {
            CrashReportWebView.panels = new Map();
        }
        // Create a new webview panel for each crash report
        const panel = window.createWebviewPanel(
            // Panel view type
            "showCrashReportPanel",
            // Panel title
            "Crash report",
            // The editor column the panel should be displayed in
            vscode.ViewColumn.One,
            // Extra panel configurations
            {
                // Enable JavaScript in the webview
                enableScripts: true,
            }
        );
        const UID = crypto.randomBytes(8).toString('hex');
        CrashReportWebView.panels.set(UID, new CrashReportWebView(panel, UID, context, document, error));
    }

    /**
     * Cleans up and disposes of webview resources when the webview panel is closed.
     */
    public dispose() {
        CrashReportWebView.panels.delete(this.UID);
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
        const htmlPath = getUri(webview, extensionUri, ["client", "views", "crash_report", "body.html"]);
        const styleUri = getUri(webview, extensionUri, ["client", "views", "crash_report", "style.css"]);
        const codiconStyleUri = getUri(webview, extensionUri, ["node_modules", "@vscode", "codicons", "dist", "codicon.css"]);
        const mainUri = getUri(webview, extensionUri, ["client", "views", "crash_report", "script.js"]);
        const htmlFile = fs.readFileSync(htmlPath.fsPath, 'utf-8');
        const nonce = getNonce();

        let data = {
            webviewElementsUri: webviewElementsUri,
            styleUri: styleUri,
            codiconStyleUri: codiconStyleUri,
            mainUri: mainUri,
            cspSource: webview.cspSource,
            crashUID: this.UID,
            crash_log: this._error,
            nonce: nonce,
        };
        return ejs.render(htmlFile, data);
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

            switch (command) {
                case "send_report":
                    axios.post('https://iap-services.odoo.com/api/odools/vscode/1/crash_report', {
                        data: {
                            uid: this.UID,
                            document: this._document ? this._document.getText() : null,
                            error: this._error,
                            additional_info: message.additional_info,
                            version: this._context.extension.packageJSON.version
                        }
                    });
                    this.dispose();
                    break;
            }
        },
            undefined,
            this._disposables
        );
    }
}
