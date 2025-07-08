"use strict";

import * as net from "net";
import * as path from "path";
import * as fs from "fs";
import * as semver from "semver";
import {homedir} from "os"
import {
    extensions,
    commands,
    ExtensionContext,
    ExtensionMode,
    StatusBarAlignment,
    ViewColumn,
    workspace,
    window,
    TextDocument,
    OutputChannel,
    Uri,
    ConfigurationTarget,
    Range,
    TextEditor,
    DecorationOptions,
} from "vscode";
import {
    LanguageClientOptions,
    ServerOptions,
} from "vscode-languageclient/node";
import { WelcomeWebView } from "./views/welcome/welcomeWebView";
import { CrashReportWebView } from './views/crash_report/crashReport';
import { ChangelogWebview } from "./views/changelog/changelogWebview";
import {
    clientStopped
} from './common/events'
import {
    onDidChangePythonInterpreterEvent
} from "./common/python";
import { getCurrentConfig } from "./common/utils";
import { getConfigurationStructure, stateInit } from "./common/validation";
import {
    migrateAfterDelay,
    migrateConfigToSettings,
    migrateShowHome
} from "./migration/migrateConfig";
import { SafeLanguageClient } from "./common/safeLanguageClient";
import { constants } from "fs/promises";
import { ThemeIcon } from "vscode";


let CONFIG_HTML_MAP: Record<string, string> = {};
let CONFIG_FILE: any = undefined;

function handleSetConfigurationNotification(payload: { html: Record<string, string>, configFile: any }) {
    CONFIG_HTML_MAP = payload.html || {};
    CONFIG_FILE = payload.configFile;
}

function getClientOptions(): LanguageClientOptions {
    return {
        // Register the server for plain text documents
        documentSelector: [
            { scheme: "file", language: "python" },
            { scheme: "file", language: "xml" },
            { scheme: "file", language: "csv" },
            { scheme: "untitled", language: "python" },
        ],
        synchronize: {
        },
    };
}

function validateState(context: ExtensionContext) {
    try {
        let globalState = context.globalState
        let stateVersion = globalState.get('Odoo.stateVersion', false)
        if (!stateVersion || stateVersion != stateInit['Odoo.stateVersion']) {
            for (let key of Object.keys(stateInit)) {
                let state = globalState.get(key, null)
                let versionState = stateInit[key]
                if (!state) {
                    globalState.update(key, versionState)
                }
                else {
                    let updates = false
                    let configurations = {}
                    if (key === 'Odoo.configurations' && Object.keys(state).length > 0) {
                        for (let configId of Object.keys(state)) {
                            let config = state[configId]
                            let configStruct = getConfigurationStructure()
                            for (let confKey of Object.keys(configStruct)) {
                                if (!(confKey in config)) {
                                    config[confKey] = configStruct[confKey]
                                    updates = true
                                }
                            }

                            configurations = {
                                ...configurations,
                                [configId]: config,
                            }
                        }
                    }
                    if (updates && key === 'Odoo.configurations') {
                        workspace.getConfiguration().update(key, configurations, ConfigurationTarget.Global)
                    }else if (updates) {
                        globalState.update(key, configurations)
                    }
                }

            }
            globalState.update('Odoo.stateVersion', stateInit['Odoo.stateVersion'])
        }

    }
    catch (error) {
        global.LSCLIENT.error(error);
        displayCrashMessage(context, error, global.SERVER_PID, 'func.validateState')
    }
}

function setMissingStateVariables(context: ExtensionContext) {
    const globalStateKeys = context.globalState.keys();
    let globalVariables = new Map<string, any>([
        ["Odoo.nextConfigId", stateInit["Odoo.nextConfigId"]],
        ["Odoo.stateVersion", stateInit["Odoo.stateVersion"]],
        ["Odoo.lastRecordedVersion", context.extension.packageJSON.version],
    ]);

    for (let key of globalVariables.keys()) {
        if (!globalStateKeys.includes(key)) {
            global.LSCLIENT.info(`${key} was missing in global state. Setting up the variable.`);
            context.globalState.update(key, globalVariables.get(key));
        }
    }
}

function isExtensionUpdated(context: ExtensionContext) {
    const currentSemVer = semver.parse(context.extension.packageJSON.version);
    const lastRecordedSemVer = semver.parse(context.globalState.get("Odoo.lastRecordedVersion", ""));

    if (currentSemVer > lastRecordedSemVer) return true;
    return false;
}

async function displayUpdatedNotification(context: ExtensionContext) {
    if(context.extensionMode === ExtensionMode.Development){
        return
    }

    window.showInformationMessage(
        "The Odoo extension has been updated.",
        "Show changelog",
        "Dismiss"
    ).then(selection => {
        switch (selection) {
            case "Show changelog":
                ChangelogWebview.render(context);
                break;
        }
    });
}

function updateLastRecordedVersion(context: ExtensionContext) {
    context.globalState.update("Odoo.lastRecordedVersion", context.extension.packageJSON.version);
}

function startLangServerTCP(addr: number, outputChannel: OutputChannel): SafeLanguageClient {
    const serverOptions: ServerOptions = () => {
        return new Promise((resolve /*, reject */) => {
            const clientSocket = new net.Socket();
            clientSocket.connect(addr, "127.0.0.1", () => {
                resolve({
                    reader: clientSocket,
                    writer: clientSocket,
                });
            });
        });
    };

    const clientOptions: LanguageClientOptions = getClientOptions();

    clientOptions.outputChannel = outputChannel;

    return new SafeLanguageClient(
        'odooServer',
        `Odoo Server`,
        serverOptions,
        clientOptions
    );
}

function startLangServer(
    command: string,
    args: string[],
    cwd: string,
    outputChannel: OutputChannel
): SafeLanguageClient {
    const serverOptions: ServerOptions = {
        command,
        args,
        options: { cwd, env: process.env },
    };
    const clientOptions: LanguageClientOptions = getClientOptions();
    clientOptions.outputChannel = outputChannel;

    return new SafeLanguageClient('odooServer', 'Odoo Server', serverOptions, clientOptions);
}

async function setStatusConfig(context: ExtensionContext) {
    const config = await getCurrentConfig(context);
    let text = (config ? `Odoo (${config})` : `Odoo (Disabled)`);
    global.STATUS_BAR.text = (global.IS_LOADING) ? "$(loading~spin) " + text : text;
}


async function changeSelectedConfig(context: ExtensionContext, configName: string) {
  try {
    if (configName == "Disabled"){
        configName = undefined;
    }
    await workspace.getConfiguration().update("Odoo.selectedProfile", configName, ConfigurationTarget.Workspace);
    return true;
  } catch (err) {
    window.showErrorMessage(`Failed to change configuration: ${err}`);
    return false;
  }
}

async function findLastLogFile(context: ExtensionContext, pid: number) {
    let prefix = "odoo_logs";
    let suffix =  `.${pid}.log`
    let cwd = path.join(__dirname, "..", "..");
    if (context.extensionMode === ExtensionMode.Development) {
        cwd = path.join(cwd, "..", "server", "target", "debug");
    }
    let directory = path.join(cwd, "logs");
    const files = fs.readdirSync(directory);

    // filter files with format 'prefix-yyyy-MM-dd-HH'
    const logFiles = files.filter(file => {
        const regex = new RegExp(`^${prefix}\.\\d{4}-\\d{2}-\\d{2}-\\d{2}${suffix}$`);
        return regex.test(file);
    });

    if (logFiles.length === 0) {
        return null;
    }

    // Sort files by date
    logFiles.sort((a, b) => {
        const dateA = a.slice(prefix.length + 1); // delete prefix and dot
        const dateB = b.slice(prefix.length + 1);
        return dateB.localeCompare(dateA);
    });

    // Retourner le chemin complet du dernier fichier de log
    return path.join(directory, logFiles[0]);
}

async function displayCrashMessage(context: ExtensionContext, crashInfo: string, pid = 0, command: string = null, outputChannel = global.LSCLIENT.outputChannel) {
    // Capture the content of the file active when the crash happened
    let activeFile: TextDocument;
    if (window.activeTextEditor) {
        activeFile = window.activeTextEditor.document;
    } else {
        activeFile = null;
    }
    const selection = await window.showErrorMessage(
        "The Odoo extension encountered an error and crashed. Do you wish to send a crash report ?",
        "Send crash report",
        "Open logs",
        "Cancel"
    );

    let log_file = await findLastLogFile(context, pid);

    switch (selection) {
        case ("Send crash report"):
            CrashReportWebView.render(context, activeFile, crashInfo, command, log_file);
            break
        case ("Open logs"):
            outputChannel.show();
            break
    }
}

async function initLanguageServerClient(context: ExtensionContext, outputChannel: OutputChannel, autoStart = false) {
    let client : SafeLanguageClient;
    try {
        if (!workspace.getConfiguration('Odoo').get("disablePythonLanguageServerPopup", false)){
            displayDisablePythonLSMessage();
        }

        global.SERVER_PID = 0;
        let serverPath = "./odoo_ls_server.exe";
        if (process.platform !== 'win32') {
            serverPath = "./odoo_ls_server"
        }

        if (context.extensionMode === ExtensionMode.Development) {
            // Development - Run the server manually
            await commands.executeCommand('setContext', 'odoo.showCrashNotificationCommand', true);
            client = startLangServerTCP(2087, outputChannel);
        } else {
            // Production - Client is going to run the server (for use within `.vsix` package)
            const cwd = path.join(__dirname, "..", "..");
            let log_level = String(workspace.getConfiguration().get("Odoo.serverLogLevel"));
            client = startLangServer(serverPath, ["--log-level", log_level], cwd, outputChannel);
        }

        context.subscriptions.push(
            client.onNotification("$Odoo/loadingStatusUpdate", async (state: String) => {
                switch (state) {
                    case "start":
                        global.IS_LOADING = true;
                        break;
                    case "stop":
                        global.IS_LOADING = false;
                        break;
                }
                await setStatusConfig(context);
            }),
            client.onNotification("$Odoo/setPid", async(params) => {
                global.SERVER_PID = params["server_pid"];
            }),
            client.onNotification("$Odoo/setConfiguration", handleSetConfigurationNotification),
            client.onNotification("$Odoo/invalid_python_path", async(params) => {
                await window.showErrorMessage(
                    "The Odoo extension is unable to start Python with the path you provided. Verify your configuration"
                );
            }),
            client.onNotification("Odoo/displayCrashNotification", async (params) => {
                await displayCrashMessage(context, params["crashInfo"], params["pid"]);
            }),
            client.onNotification("$Odoo/restartNeeded", async () => {
                if (global.LSCLIENT) {
                    global.LSCLIENT.restart();
                    global.IS_LOADING = false;
                    setStatusConfig(context);
                }
            })
        );
        global.PATH_VARIABLES = {"userHome" : homedir().replaceAll("\\","/")};
        if (autoStart) {
            await client.start();
        }
        return client;
    } catch (error) {
        outputChannel.appendLine("Couldn't Start Language server.");
        outputChannel.appendLine(error);
        await displayCrashMessage(context, error, global.SERVER_PID, 'initLanguageServer', outputChannel);
    }
}

function extractDateFromFileName(fileName: string): Date | null {
    const regex = /^odoo_logs_\d{1,7}\.(\d{4})-(\d{2})-(\d{2})-(\d{2})\.log$/;
    const match = fileName.match(regex);

    if (match) {
        const [_, year, month, day] = match;
        return new Date(parseInt(year), parseInt(month) - 1, parseInt(day));
    }

    return null;
}

function deleteOldFiles(context: ExtensionContext) {
    const logDir = Uri.joinPath(context.extensionUri, 'logs');
    fs.access(logDir.fsPath, constants.W_OK, (err) => {
        if (!err) {
            const files = fs.readdirSync(logDir.fsPath).filter(fn => fn.startsWith('odoo_logs_'));
            let dateLimit = new Date();
            dateLimit.setDate(dateLimit.getDate() - 2);

            for (const file of files) {
                const date = extractDateFromFileName(file);
                if (date && date < dateLimit) {
                    fs.unlinkSync(Uri.joinPath(logDir, file).fsPath);
                }
            }
        }
    });
}

async function initStatusBar(context: ExtensionContext): Promise<void> {
    global.STATUS_BAR = window.createStatusBarItem(StatusBarAlignment.Left, 100);
    global.STATUS_BAR.command = "odoo.clickStatusBar";
    global.STATUS_BAR.tooltip = "Odoo: Change Configuration";
    context.subscriptions.push(global.STATUS_BAR);
    await setStatusConfig(context);
    global.STATUS_BAR.show();

    // Add a restart button to the status bar
    global.STATUS_BAR_RESTART = window.createStatusBarItem(StatusBarAlignment.Left, 99);
    global.STATUS_BAR_RESTART.text = "$(refresh)";
    global.STATUS_BAR_RESTART.tooltip = "Odoo: Restart Language Server";
    global.STATUS_BAR_RESTART.command = "odoo.restartServer";
    context.subscriptions.push(global.STATUS_BAR_RESTART);
    global.STATUS_BAR_RESTART.show();
}


async function initializeSubscriptions(context: ExtensionContext): Promise<void> {
    let terminal = window.terminals.find(t => t.name === 'close-odoo-client')
    if (!terminal){
        window.createTerminal({ name: `close-odoo-client`, hideFromUser:true})
    }

    context.subscriptions.push(
        window.onDidCloseTerminal(async (terminal) => {
        if (terminal.name === 'close-odoo-client') await stopClient();
        }),

        // Listen to changes to the selected Configuration
        workspace.onDidChangeConfiguration(async (event) => {
            try {
                if (!global.CAN_QUEUE_CONFIG_CHANGE) return;

                if (global.CLIENT_IS_STOPPING) {
                    global.CAN_QUEUE_CONFIG_CHANGE = false;
                    await waitForClientStop();
                    global.CAN_QUEUE_CONFIG_CHANGE = true;
                }

                if (!global.IS_PYTHON_EXTENSION_READY){
                    onDidChangePythonInterpreterEvent.fire(null);
                }
                let client = global.LSCLIENT;
                if (!client) {
                    global.LSCLIENT = await initLanguageServerClient(context, global.OUTPUT_CHANNEL);
                    client = global.LSCLIENT;
                }
                if (client.needsStart()) {
                    await client.start();
                }
                await setStatusConfig(context);
                if (event.affectsConfiguration("Odoo.disablePythonLanguageServerPopup") && !workspace.getConfiguration('Odoo').get("disablePythonLanguageServerPopup", false)){
                    displayDisablePythonLSMessage()
                }
            }
            catch (error) {
                global.LSCLIENT?.error(error);
                await displayCrashMessage(context, error, global.SERVER_PID, 'event.onDidChangeConfiguration');
            }
        }),


        // COMMANDS
        commands.registerCommand("odoo.openWelcomeView", async () => {
            try {
                WelcomeWebView.render(context);
            }
            catch (error) {
                global.LSCLIENT.error(error)
                await displayCrashMessage(context, error, global.SERVER_PID, 'odoo.openWelcomeView')
            }
        }),
        commands.registerCommand("odoo.clearState", async () => {
            try {
                for (let key of context.globalState.keys()) {
                    global.LSCLIENT.info(`Wiping ${key} from global storage.`);
                    await context.globalState.update(key, undefined);
                }

                for (let key of context.workspaceState.keys()) {
                    global.LSCLIENT.info(`Wiping ${key} from workspace storage.`);
                    await context.workspaceState.update(key, undefined);
                }
                await commands.executeCommand("workbench.action.reloadWindow");
            }
            catch (error) {
                global.LSCLIENT.error(error);
                await displayCrashMessage(context, error, global.SERVER_PID, 'odoo.clearState');
            }
        }),
        commands.registerCommand("odoo.openChangelogView", () => {
            ChangelogWebview.render(context);
        }),
        commands.registerCommand('odoo.clickStatusBar', async () => {
            await showConfigProfileQuickPick(context);
        }),
        commands.registerCommand(
            "odoo.disablePythonLanguageServerCommand", setPythonLSNone
            ),
        commands.registerCommand(
            "odoo.restartServer", async () => {
                if (global.LSCLIENT) {
                    global.LSCLIENT.restart();
                    global.IS_LOADING = false;
                    setStatusConfig(context);
                }
        }),
        commands.registerCommand("odoo.showServerConfig", async () => {
            showConfigPreview("__all__");
        }),
    );

    if (context.extensionMode === ExtensionMode.Development) {
        context.subscriptions.push(
            commands.registerCommand(
                "odoo.testCrashMessage", async () => {
                    await displayCrashMessage(context, "Test crash message", global.SERVER_PID);
                }
            )
        );
    }
}

function generateHSLColors(count: number): string[] {
    const colors: string[] = [
        "hsl(200, 70%, 55%)",
        
        "hsl(30, 70%, 55%)",
        
        "hsl(60, 70%, 55%)",
        
        "hsl(100, 70%, 55%)",
    ];
    const saturation = 70;
    const lightness = 65;
    const angle = 137.508;
    const baseHue = 100

    for (let i = 4; i < count; i++) {
        const hue = (baseHue + i * angle) % 360;
        const hsl = `hsl(${hue.toFixed(1)}, ${saturation}%, ${lightness}%)`;
        colors.push(hsl);
    }

    return colors;
}

async function initializeCSVSemanticTokenProvider(context: ExtensionContext): Promise<void> {
    const rainbowCsv = extensions.getExtension('mechatroner.rainbow-csv');

    if (rainbowCsv && rainbowCsv.isActive) {
    console.log('Rainbow CSV is active, disabling decorations.');
    return;
    }
    const activeEditor = window.activeTextEditor;

    if (activeEditor) {
        triggerUpdateDecorations(activeEditor);
    }

    context.subscriptions.push(
        window.onDidChangeActiveTextEditor(editor => {
        if (editor) triggerUpdateDecorations(editor);
        }),
        workspace.onDidChangeTextDocument(event => {
        if (window.activeTextEditor && event.document === window.activeTextEditor.document) {
            triggerUpdateDecorations(window.activeTextEditor);
        }
        })
    );

    function triggerUpdateDecorations(editor: TextEditor) {
        if (!editor || editor.document.languageId !== 'csv') return;

        const text = editor.document.getText();
        const lines = text.split(/\r?\n/);

        let maxColumns = 0;
        const allRows = lines.map(line => line.split(','));
        for (const row of allRows) {
            if (row.length > maxColumns) maxColumns = row.length;
        }

        const baseColor: [number, number, number] = [255, 125, 135];

        const colors = generateHSLColors(maxColumns);

        const decorationTypes = colors.map(color =>
            window.createTextEditorDecorationType({ color })
        );

        const decorationsArray: DecorationOptions[][] = colors.map(() => []);

        for (let lineIdx = 0; lineIdx < allRows.length; lineIdx++) {
            const columns = allRows[lineIdx];
            let colStart = 0;

            for (let colIdx = 0; colIdx < columns.length; colIdx++) {
            const colText = columns[colIdx];
            const startPos = colStart;
            const endPos = colStart + colText.length;

            const range = new Range(lineIdx, startPos, lineIdx, endPos);
            decorationsArray[colIdx].push({ range });

            colStart = endPos + 1; // +1 for the comma
            }
        }

        decorationTypes.forEach((decType, idx) => {
            editor.setDecorations(decType, decorationsArray[idx]);
        });
    }
}

function handleMigration(context){
    migrateConfigToSettings(context)
    migrateAfterDelay(context)
    if (isExtensionUpdated(context)) {
        migrateShowHome(context);
    }
}

export function getCurrentConfigEntry(context: ExtensionContext): any | undefined {
    if (!CONFIG_FILE || !CONFIG_FILE.config) return undefined;
    const selected = workspace.getConfiguration().get("Odoo.selectedProfile") as string;
    const configName = selected && selected !== "" ? selected : "default";
    return CONFIG_FILE.config.find((c: any) => c.name === configName);
}

export function getCurrentConfigFromConfigFile(context: ExtensionContext): { odooPath?: string, addons?: string[] } | undefined {
    const entry = getCurrentConfigEntry(context);
    if (!entry) return undefined;
    const odooPath = entry.odoo_path?.value || entry.odoo_path;
    let addons: string[] = [];
    if (Array.isArray(entry.addons_paths)) {
        addons = entry.addons_paths.map((a: any) => a.value || a).filter(Boolean);
    }
    return { odooPath, addons };
}

export async function activate(context: ExtensionContext): Promise<void> {
    try {
        global.CAN_QUEUE_CONFIG_CHANGE = true;
        global.OUTPUT_CHANNEL = window.createOutputChannel('Odoo', 'python');
        global.LSCLIENT = await initLanguageServerClient(context, global.OUTPUT_CHANNEL);
        // Initialize some settings on the extension's launch if they're missing from the state.
        setMissingStateVariables(context);
        validateState(context);
        handleMigration(context)


        await initStatusBar(context);
        await initializeSubscriptions(context);
        await initializeCSVSemanticTokenProvider(context);

        switch (context.globalState.get('Odoo.displayWelcomeView', null)) {
            case null:
                context.globalState.update('Odoo.displayWelcomeView', true);
                WelcomeWebView.render(context);
                break;
            case true:
                WelcomeWebView.render(context);
                break;
        }

        // Check if the extension was updated since the last time.
        if (isExtensionUpdated(context)) await displayUpdatedNotification(context);

        // We update the last used version on every run.
        updateLastRecordedVersion(context);

        deleteOldFiles(context);
        global.LSCLIENT.info('Starting the extension.');
        setStatusConfig(context);
        global.LSCLIENT.start();
    }
    catch (error) {
        displayCrashMessage(context, error, global.SERVER_PID, 'odoo.activate');
        global.LSCLIENT.error(error);
    }
}

async function waitForClientStop() {
    return new Promise<void>(resolve => {
        clientStopped.event(e => {
            resolve();
        })
    });
}

async function stopClient() {
    if (global.LSCLIENT && !global.CLIENT_IS_STOPPING) {
        global.LSCLIENT.info("Stopping LS Client.");
        global.CLIENT_IS_STOPPING = true;
        await global.LSCLIENT.stop(15000);
        global.CLIENT_IS_STOPPING = false;
        clientStopped.fire(null);
        global.LSCLIENT.info("LS Client stopped.");
    }
}

export async function deactivate(): Promise<void> {
    if (global.LSCLIENT) {
        return global.LSCLIENT.dispose();
    }
}

async function setPythonLSNone() {
    await workspace.getConfiguration('python').update('languageServer', 'None', ConfigurationTarget.Workspace)
        .then(
            () => window.showInformationMessage('Python language server set to None for current workspace for Odoo LS to function properly'),
            (error) => window.showErrorMessage(`Failed to update setting: ${error}`)
        );
}

async function displayDisablePythonLSMessage() {
    if (!global.IS_PYTHON_EXTENSION_READY){
        return
    }
    // if python.languageServer is already None do not show the pop-up
    if (workspace.getConfiguration('python').get("languageServer") == "None"){
        return
    }
    window.showInformationMessage(
        "Disable Python Addon Language server for a better experience",
        "Yes",
        "No",
        "Don't Show again",
    ).then(async selection => {
        switch (selection) {
            case "Yes":
                await setPythonLSNone();
                break;
            case "Don't Show again":
                await workspace.getConfiguration('Odoo').update("disablePythonLanguageServerPopup", true, ConfigurationTarget.Global)
        }
    });
}

async function showConfigProfileQuickPick(context: ExtensionContext) {
  if (!CONFIG_HTML_MAP || Object.keys(CONFIG_HTML_MAP).length === 0) {
    window.showErrorMessage("No configuration profiles available. Please wait for the server to send configurations.");
    return;
  }

  // Remove the special __all__ key for the list, but keep for preview
  let profiles = Object.keys(CONFIG_HTML_MAP).filter(k => k !== "__all__");
  // Filter out profiles that are abstract (abstract === true)
  if (CONFIG_FILE && Array.isArray(CONFIG_FILE.config)) {
    const abstractProfiles = new Set(
      CONFIG_FILE.config.filter((c: any) => c.abstract === true).map((c: any) => c.name)
    );
    profiles = profiles.filter(name => !abstractProfiles.has(name));
  }

  if (profiles.length === 0) {
    window.showErrorMessage("No configuration profiles found.");
    return;
  }

  const allConfigsLabel = "$(list-unordered) Show all configurations";
  const items = [
    {
      label: allConfigsLabel,
      description: "",
      alwaysShow: true,
      buttons: [
        {
          iconPath: new ThemeIcon("eye"),
          tooltip: "Preview all configurations",
        }
      ]
    },
    ...profiles.map((profile) => ({
      label: profile,
      description: "",
      alwaysShow: true,
      buttons: [
        {
          iconPath: new ThemeIcon("eye"),
          tooltip: "Preview configuration",
        }
      ]
    })),
    {
        label: "Disabled",
        alwaysShow: true,
    }
  ];

  const quickPick = window.createQuickPick();
  quickPick.items = items;
  quickPick.title = "Select Odoo Configuration Profile";
  quickPick.matchOnDescription = true;
  quickPick.matchOnDetail = true;
  quickPick.canSelectMany = false;

  quickPick.onDidTriggerItemButton(async (e) => {
    if (e.item.label === allConfigsLabel) {
      showConfigPreview("__all__");
    } else {
      showConfigPreview(e.item.label);
    }
  });

  quickPick.onDidAccept(async () => {
    quickPick.hide();
    const selection = quickPick.selectedItems[0];
    if (selection) {
      if (selection.label === allConfigsLabel) {
        showConfigPreview("__all__");
      } else {
        const ok = await changeSelectedConfig(context, selection.label);
        if (ok && global.LSCLIENT) {
            global.LSCLIENT.restart();
            global.IS_LOADING = false;
            setStatusConfig(context);
        }
      }
    }
  });

  quickPick.show();
}

function showConfigPreview(profileName: string) {
  if (!CONFIG_HTML_MAP || Object.keys(CONFIG_HTML_MAP).length === 0) {
    window.showErrorMessage("No configuration profiles available. Please wait for the server to send configurations.");
    return;
  }
  const html = CONFIG_HTML_MAP[profileName];
  if (!html) {
    window.showErrorMessage("No config HTML found for this profile.");
    return;
  }
  const panel = window.createWebviewPanel(
    'odooConfigPreview',
    'Odoo Config Preview',
    ViewColumn.Active,
    {
      retainContextWhenHidden: true,
    }
  );
  // Replace file:/// links with vscode://file/ links
  panel.webview.html = html.replace(
    /href="file:\/\/\/([^"]+)"/g,
    (match, filePath) => {
      const decodedPath = decodeURIComponent(filePath);
      const vscodeUri = `vscode://file/${decodedPath}`;
      return `href="${vscodeUri}"`;
    }
  );
}
