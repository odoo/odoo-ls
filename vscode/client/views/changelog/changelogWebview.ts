import { Disposable, Uri, Webview, WebviewPanel, window } from "vscode";
import * as vscode from 'vscode';
import {readFileSync} from 'fs';
import * as ejs from "ejs";
import MarkdownIt = require('markdown-it');
import { getNonce, getUri } from "../../common/utils";

const md = new MarkdownIt('commonmark');

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
export class ChangelogWebview {
    public static readonly viewType = 'changelogView';
    public static currentPanel: ChangelogWebview | undefined;
    private readonly _panel: WebviewPanel;
    private _disposables: Disposable[] = [];
    private readonly _context: vscode.ExtensionContext

    /**
     * The ConfigurationWebView class private constructor (called only from the render method).
     *
     * @param panel A reference to the webview panel
     * @param extensionUri The URI of the directory containing the extension
     */
    private constructor(panel: WebviewPanel, context: vscode.ExtensionContext) {
        this._panel = panel;
        this._context = context;

        // Set an event listener to listen for when the panel is disposed (i.e. when the user closes
        // the panel or when the panel is closed programmatically)
        this._panel.onDidDispose(this.dispose, this, this._disposables);

        // Set the HTML content for the webview panel
        this._panel.webview.html = this._getWebviewContent(this._panel.webview, context.extensionUri);
   }

    /**
     * Close the current webview panel if one exists then a new webview panel
     * will be created and displayed.
     *
     * @param extensionUri The URI of the directory containing the extension.
     */
    public static render(context: vscode.ExtensionContext) {
      const column = vscode.window.activeTextEditor
      ? vscode.window.activeTextEditor.viewColumn
      : undefined;

      // If we already have a panel, close it.
      if (ChangelogWebview.currentPanel) {
         ChangelogWebview.currentPanel._panel.dispose();
      }
      
      const panel = window.createWebviewPanel(
            // Panel view type
            "changelogView",
            // Panel title
            `Odoo: Changelog`,
            // The editor column the panel should be displayed in
            column,
            // Extra panel configurations
            {
               // Enable JavaScript in the webview
               enableScripts: true,
            }
      );

      ChangelogWebview.currentPanel = new ChangelogWebview(panel, context);
   }


    /**
     * Cleans up and disposes of webview resources when the webview panel is closed.
     */
    public dispose() {
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
      // HTML Rendering is done here
      const changelogUri = Uri.joinPath(extensionUri, "CHANGELOG.md");
      const changelogContent: string = readFileSync(changelogUri.fsPath, 'utf8');
      const htmlPath = getUri(webview, extensionUri, ["client", "views", "changelog", "body.html"]);
      const styleUri = getUri(webview, extensionUri, ["client", "views", "changelog", "style.css"]);
      const htmlFile = readFileSync(htmlPath.fsPath, 'utf-8');

      const data = {
         styleUri: styleUri,
         content: md.render(changelogContent),
         cspSource: webview.cspSource,
         nonce: getNonce()
      }

      return ejs.render(htmlFile, data);
   }
}
