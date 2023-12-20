"use strict";

import * as net from "net";
import * as path from "path";
import * as fs from "fs";
import * as semver from "semver";
import {
    commands,
    ExtensionContext,
    ExtensionMode,
    QuickPickItem,
    StatusBarAlignment,
    ThemeIcon,
    workspace,
    window,
    QuickPickItemKind,
    TextDocument,
    OutputChannel,
    Uri,
} from "vscode";
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    integer,
} from "vscode-languageclient/node";
import { WelcomeWebView } from "./views/welcome/welcomeWebView";
import { ConfigurationWebView } from './views/configurations/configurationWebView';
import { CrashReportWebView } from './views/crash_report/crashReport';
import { ChangelogWebview } from "./views/changelog/changelogWebview";
import {
    selectedConfigurationChange,
    ConfigurationsChange,
    clientStopped
} from './common/events'
import { 
    IInterpreterDetails, 
    getInterpreterDetails, 
    initializePython, 
    onDidChangePythonInterpreter, 
    onDidChangePythonInterpreterEvent 
} from "./common/python";
import { getCurrentConfig } from "./common/utils";
import { getConfigurationStructure, stateInit } from "./common/validation";
import { execSync } from "child_process";


function getClientOptions(): LanguageClientOptions {
    return {
        // Register the server for plain text documents
        documentSelector: [
            { scheme: "file", language: "python" },
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
                    if (updates) {
                        globalState.update(key, configurations)
                    }
                }

            }
            globalState.update('Odoo.stateVersion', stateInit['Odoo.stateVersion'])
        }

    }
    catch (error) {
        global.LSCLIENT.error(error);
        displayCrashMessage(context, error, 'func.validateState')
    }
}

function setMissingStateVariables(context: ExtensionContext) {
    const globalStateKeys = context.globalState.keys();
    const workspaceStateKeys = context.workspaceState.keys();
    let globalVariables = new Map<string, any>([
        ["Odoo.configurations", stateInit["Odoo.configuration"]],
        ["Odoo.nextConfigId", stateInit["Odoo.nextConfigId"]],
        ["Odoo.stateVersion", stateInit["Odoo.stateVersion"]],
        ["Odoo.lastRecordedVersion", context.extension.packageJSON.version], 
    ]);
    const workspaceVariables = new Map([["Odoo.selectedConfiguration", [-1]]]);

    for (let key of globalVariables.keys()) {
        if (!globalStateKeys.includes(key)) {
            global.LSCLIENT.info(`${key} was missing in global state. Setting up the variable.`);
            context.globalState.update(key, globalVariables.get(key));
        }
    }

    for (let key of workspaceVariables.keys()) {
        if (!workspaceStateKeys.includes(key)) {
            global.LSCLIENT.info(`${key} was missing in workspace state. Setting up the variable.`);
            context.workspaceState.update(key, workspaceVariables.get(key));
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
    const selection = await window.showInformationMessage(
        "The Odoo extension has been updated.",
        "Show changelog",
        "Dismiss"
    )
    switch (selection) {
        case "Show changelog":
            ChangelogWebview.render(context);
            break;
    }
}

function updateLastRecordedVersion(context: ExtensionContext) {
    context.globalState.update("Odoo.lastRecordedVersion", context.extension.packageJSON.version);
}

function startLangServerTCP(addr: number, outputChannel: OutputChannel): LanguageClient {
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

    return new LanguageClient(
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
): LanguageClient {
    const serverOptions: ServerOptions = {
        args,
        command,
        options: { cwd },
    };
    const clientOptions: LanguageClientOptions = getClientOptions();
    clientOptions.outputChannel = outputChannel;

    return new LanguageClient('odooServer', 'Odoo Server', serverOptions, clientOptions);
}

async function setStatusConfig(context: ExtensionContext) {
    const config = await getCurrentConfig(context);
    let text = (config ? `Odoo (${config["name"]})` : `Odoo (Disabled)`);
    global.STATUS_BAR.text = (global.IS_LOADING) ? "$(loading~spin) " + text : text;
}

function getLastConfigId(context: ExtensionContext): number | undefined {
    return context.globalState.get("Odoo.nextConfigId");
}

function IncrementLastConfigId(context: ExtensionContext) {
    const lastId: number = context.globalState.get("Odoo.nextConfigId");
    context.globalState.update("Odoo.nextConfigId", lastId + 1);
}

async function addNewConfiguration(context: ExtensionContext) {
    const configId = getLastConfigId(context);
    let configs: Map<number, object> = context.globalState.get("Odoo.configurations");
    await context.globalState.update(
        "Odoo.configurations",
        {
            ...configs,
            [configId]: getConfigurationStructure(configId),
        }
    );
    ConfigurationsChange.fire(null);
    IncrementLastConfigId(context);
    ConfigurationWebView.render(context, configId);
}

async function changeSelectedConfig(context: ExtensionContext, configId: Number) {
    const oldConfig = await getCurrentConfig(context)
    await context.workspaceState.update("Odoo.selectedConfiguration", configId);
    selectedConfigurationChange.fire(oldConfig);
}

async function displayCrashMessage(context: ExtensionContext, crashInfo: string, command: string = null, outputChannel = global.LSCLIENT.outputChannel) {
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

    switch (selection) {
        case ("Send crash report"):
            CrashReportWebView.render(context, activeFile, crashInfo, command, global.DEBUG_FILE);
            break
        case ("Open logs"):
            outputChannel.show();
            break
    }
}

async function initLanguageServerClient(context: ExtensionContext, outputChannel: OutputChannel, autoStart = false) {
    let client = global.LSCLIENT;
    try {
        let pythonPath: string;

        try{
            //trying to use the VScode python extension
            const interpreter = await getInterpreterDetails();
            pythonPath = interpreter.path[0];
            global.IS_PYTHON_EXTENSION_READY = true;

        }catch{
            global.IS_PYTHON_EXTENSION_READY = false;
            //python extension is not available switch to standalone mode
            pythonPath =  await getStandalonePythonPath(context);
            await checkStandalonePythonVersion(context)
        }
        outputChannel.appendLine("[INFO] Python VS code extension is ".concat(global.IS_PYTHON_EXTENSION_READY ? "ready" : "not ready"));

        

        if (context.extensionMode === ExtensionMode.Development) {
            // Development - Run the server manually
            await commands.executeCommand('setContext', 'odoo.showCrashNotificationCommand', true);
            client = startLangServerTCP(2087, outputChannel);
            global.DEBUG_FILE = 'pygls.log';
        } else {
            // Production - Client is going to run the server (for use within `.vsix` package)
            const cwd = path.join(__dirname, "..", "..");
            client = startLangServer(pythonPath, ["-m", "server", "--log", global.DEBUG_FILE, "--id", "clean-odoo-lsp"], cwd, outputChannel);
        }

        context.subscriptions.push(
            client.onNotification("Odoo/loadingStatusUpdate", async (state: String) => {
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
            client.onRequest("Odoo/getConfiguration", async (params) => {
                return await getCurrentConfig(context);
            }),
            client.onNotification("Odoo/displayCrashNotification", async (params) => {
                await displayCrashMessage(context, params["crashInfo"]);
            })
        );
        if (autoStart) {
            await client.start();
            await client.sendNotification("Odoo/clientReady");
        }
        return client;
    } catch (error) {
        outputChannel.appendLine("Couldn't Start Language server.");
        outputChannel.appendLine(error);
        await displayCrashMessage(context, error, 'initLanguageServer' , outputChannel);
    }
}

function deleteOldFiles(context: ExtensionContext) {
    const files = fs.readdirSync(context.extensionUri.fsPath).filter(fn => fn.startsWith('pygls-') && fn.endsWith('.log'));
    for (const file of files) {
        let dateLimit = new Date();
        dateLimit.setDate(dateLimit.getDate() - 2);
        let date = new Date(file.slice(6, -4).replaceAll("_",":"));
        if (date < dateLimit) {
            fs.unlinkSync(Uri.joinPath(context.extensionUri, file).fsPath);
        }
    }
}

async function checkAddons(context: ExtensionContext) {
    let files = await workspace.findFiles('**/__manifest__.py')
    let currentConfig = await getCurrentConfig(context);
    if (currentConfig) {
        let missingFiles = files.filter(file => {
            return !(
                currentConfig.addons.some((addon) => file.fsPath.startsWith(addon)) ||
                file.fsPath.startsWith(currentConfig.odooPath)
            )
        })
        let missingPaths = [...new Set(missingFiles.map(file => {
            let filePath = file.fsPath.split(path.sep)
            return filePath.slice(0, filePath.length - 2).join(path.sep)
        }))]
        if (missingPaths.length > 0) {
            global.LSCLIENT.warn("Missing addon paths : " + JSON.stringify(missingPaths))
            window.showWarningMessage(
                `We detected addon paths that weren't added in the current configuration. Would you like to add them?`,
                "Update current configuration",
                "View Paths",
                "Ignore"
            ).then(selection => {
                switch (selection) {
                    case ("Update current configuration"):
                        ConfigurationWebView.render(context, currentConfig.id);
                        break
                    case ("View Paths"):
                        global.LSCLIENT.outputChannel.show();
                        break
                }
            });
        }
    }
}

async function checkOdooPath(context: ExtensionContext) {
    let currentConfig = await getCurrentConfig(context);
    let odooFound = currentConfig ? workspace.getWorkspaceFolder(Uri.parse(currentConfig.odooPath)) : true
    if (!odooFound) {
        let invalidPath = false
        for (const f of workspace.workspaceFolders) {
            if (fs.existsSync(Uri.joinPath(f.uri, 'odoo-bin').fsPath) ||
                fs.existsSync(Uri.joinPath(Uri.joinPath(f.uri, 'odoo'), 'odoo-bin').fsPath)) {
                invalidPath = true;
                break;
            }
        }
        if (invalidPath) {
            window.showWarningMessage(
                `The Odoo configuration selected does not match the odoo path in the workspace. Would you like to change it?`,
                "Update current configuration",
                "Ignore"
            ).then(selection => {
            switch (selection) {
                case ("Update current configuration"):
                    ConfigurationWebView.render(context, currentConfig.id);
                    break
                }
            })
        }
    }
}

async function initStatusBar(context: ExtensionContext): Promise<void> {
    global.STATUS_BAR = window.createStatusBarItem(StatusBarAlignment.Left, 100);
    global.STATUS_BAR.command = "odoo.clickStatusBar"
    context.subscriptions.push(global.STATUS_BAR);
    await setStatusConfig(context);
    global.STATUS_BAR.show();
}


async function initializeSubscriptions(context: ExtensionContext): Promise<void> {
    let terminal = window.terminals.find(t => t.name === 'close-odoo-client')
    if (!terminal){
        window.createTerminal({ name: `close-odoo-client`, hideFromUser:true})
    }

    context.subscriptions.push(window.onDidCloseTerminal(async (terminal) => {
        if (terminal.name === 'close-odoo-client') await stopClient();
    }))

    // Listen to changes to Configurations
    context.subscriptions.push(
        ConfigurationsChange.event(async (changes: Array<string> | null) => {
            try {
                let client = global.LSCLIENT;
                await setStatusConfig(context);
                const RELOAD_ON_CHANGE = ["odooPath","addons","pythonPath"];
                if (changes && (changes.some(r=> RELOAD_ON_CHANGE.includes(r)))) {
                    await checkOdooPath(context);
                    await checkAddons(context);
                    if (client.diagnostics) client.diagnostics.clear();

                    if (changes.includes('pythonPath')){
                        await checkStandalonePythonVersion(context);
                        onDidChangePythonInterpreterEvent.fire(changes["pythonPath"]);
                        return
                    }
                    await client.sendNotification("Odoo/configurationChanged");
                }
            }
            catch (error) {
                global.LSCLIENT.error(error)
                await displayCrashMessage(context, 'event.ConfigurationsChange')
            }
        })
    );

    // Listen to changes to the selected Configuration
    context.subscriptions.push(
        selectedConfigurationChange.event(async (oldConfig) => {
            try {
                if (!global.CAN_QUEUE_CONFIG_CHANGE) return;

                if (global.CLIENT_IS_STOPPING) {
                    global.CAN_QUEUE_CONFIG_CHANGE = false;
                    await waitForClientStop();
                    global.CAN_QUEUE_CONFIG_CHANGE = true;
                }

                let client = global.LSCLIENT;
                const config = await getCurrentConfig(context)
                if (config) {
                    await checkOdooPath(context);
                    await checkAddons(context);
                    if (!global.IS_PYTHON_EXTENSION_READY){
                        await checkStandalonePythonVersion(context);
                        if (!oldConfig || config["pythonPath"] != oldConfig["pythonPath"]){
                            onDidChangePythonInterpreterEvent.fire(config["pythonPath"]);
                            await setStatusConfig(context);
                            return
                        }
                    }
                    if (!client) {
                        global.LSCLIENT = await initLanguageServerClient(context, global.OUTPUT_CHANNEL);
                        client = global.LSCLIENT;
                    }
                    if (client.needsStart()) {
                        await client.start();
                        await client.sendNotification(
                            "Odoo/clientReady",
                        );
                    } else {
                        if (client.diagnostics) client.diagnostics.clear();
                        await client.sendNotification("Odoo/configurationChanged");
                    }
                } else {
                    if (client?.isRunning()) await stopClient();
                    global.IS_LOADING = false;
                }
                await setStatusConfig(context);
            }
            catch (error) {
                global.LSCLIENT.error(error);
                await displayCrashMessage(context, error, 'event.selectedConfigurationChange');
            }
        })
    );
    
    // Listen to changes to Python Interpreter
    context.subscriptions.push(
        onDidChangePythonInterpreter(async (e: IInterpreterDetails) => {
            let startClient = false;
            global.CAN_QUEUE_CONFIG_CHANGE = false;
            if (global.LSCLIENT) {
                if (global.CLIENT_IS_STOPPING) {
                    await waitForClientStop();
                }
                if (global.LSCLIENT?.isRunning()) {
                    await stopClient();
                }
                await global.LSCLIENT.dispose();
            }
            if (await getCurrentConfig(context)) {
                startClient = true;
            }
            global.LSCLIENT = await initLanguageServerClient(context, global.OUTPUT_CHANNEL, startClient);
            global.CAN_QUEUE_CONFIG_CHANGE = true;
        })
    );

    // COMMANDS
    context.subscriptions.push(
        commands.registerCommand("odoo.openWelcomeView", async () => {
            try {
                WelcomeWebView.render(context);
            }
            catch (error) {
                global.LSCLIENT.error(error)
                await displayCrashMessage(context, error, 'odoo.openWelcomeView')
            }
        })
    );

    context.subscriptions.push(
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
                await displayCrashMessage(context, error, 'odoo.clearState');
            }
        }));

    context.subscriptions.push(commands.registerCommand("odoo.openChangelogView", () => {
        ChangelogWebview.render(context);
    }));

    context.subscriptions.push(
        commands.registerCommand('odoo.clickStatusBar', async () => {
            try {
                const qpick = window.createQuickPick();
                const configs: Map<integer, object> = context.globalState.get("Odoo.configurations");
                let selectedConfiguration = null;
                const currentConfig = await getCurrentConfig(context);
                let currentConfigItem: QuickPickItem;
                const configMap = new Map();
                const separator = { kind: QuickPickItemKind.Separator };
                const addConfigItem = {
                    label: "$(add) Add new configuration"
                };
                const disabledItem = {
                    label: "Disabled"
                }
                const gearIcon = new ThemeIcon("gear");

                for (const configId in configs) {
                    if (currentConfig && configId == currentConfig["id"])
                        continue;
                    configMap.set({ "label": configs[configId]["name"], "buttons": [{ iconPath: gearIcon }] }, configId)
                }

                let picks = [disabledItem, ...Array.from(configMap.keys())];
                if (picks.length)
                    picks.push(separator);

                if (currentConfig) {
                    currentConfigItem = { "label": currentConfig["name"], "description": "(current)", "buttons": [{ iconPath: gearIcon }] };
                    picks.splice(currentConfig["id"] + 1, 0, currentConfigItem);
                }

                picks.push(addConfigItem);
                qpick.title = "Select a configuration";
                qpick.items = picks;
                qpick.activeItems = currentConfig ? [picks[currentConfig["id"] + 1]] : [picks[0]];

                qpick.onDidChangeSelection(selection => {
                    selectedConfiguration = selection[0];
                });

                qpick.onDidTriggerItemButton(async (buttonEvent) => {
                    if (buttonEvent.button.iconPath == gearIcon) {
                        let buttonConfigId = (buttonEvent.item == currentConfigItem) ? currentConfig["id"] : configMap.get(buttonEvent.item);
                        try {
                            ConfigurationWebView.render(context, Number(buttonConfigId));
                        } catch (error) {
                            global.LSCLIENT.error(error);
                            await displayCrashMessage(context, error, 'render.ConfigurationWebView');
                        }
                    }
                });

                qpick.onDidAccept(async () => {
                    if (selectedConfiguration == addConfigItem) {
                        try {
                            await addNewConfiguration(context);
                        }
                        catch (error) {
                            global.LSCLIENT.error(error)
                            await displayCrashMessage(context, error, 'render.ConfigurationWebView')
                        }
                    }
                    else if (selectedConfiguration == disabledItem) {
                        await changeSelectedConfig(context, -1);
                    }
                    else if (selectedConfiguration && selectedConfiguration != currentConfigItem) {
                        await changeSelectedConfig(context, configMap.get(selectedConfiguration));
                    }
                    qpick.hide();
                });
                qpick.onDidHide(() => qpick.dispose());
                qpick.show();
            }
            catch (error) {
                global.LSCLIENT.error(error)
                await displayCrashMessage(context, error, 'odoo.clickStatusBar')
            }
        })
    );

    if (context.extensionMode === ExtensionMode.Development) {
        context.subscriptions.push(
            commands.registerCommand(
                "odoo.testCrashMessage", async () => {
                    await displayCrashMessage(context, "Test crash message");
                }
            )
        );
    }
}


export async function activate(context: ExtensionContext): Promise<void> {
    try {
        global.CAN_QUEUE_CONFIG_CHANGE = true;
        global.DEBUG_FILE = `pygls-${new Date().toISOString().replaceAll(":","_")}.log`;
        global.OUTPUT_CHANNEL = window.createOutputChannel('Odoo', 'python');
        global.LSCLIENT = await initLanguageServerClient(context, global.OUTPUT_CHANNEL);
        // Initialize some settings on the extension's launch if they're missing from the state.
        setMissingStateVariables(context);
        validateState(context);

        if (global.IS_PYTHON_EXTENSION_READY){
            await initializePython(context.subscriptions);
        }
        await initStatusBar(context);
        await initializeSubscriptions(context);

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

        const config = await getCurrentConfig(context);
        if (config) {
            deleteOldFiles(context)
            global.LSCLIENT.info('Starting the extension.');

            await checkOdooPath(context);
            await checkAddons(context);

            global.STATUS_BAR.text = `Odoo (${config["name"]})`
            await global.LSCLIENT.start();
            await global.LSCLIENT.sendNotification(
                "Odoo/clientReady",
            );
        }
    }
    catch (error) {
        global.LSCLIENT.error(error);
        displayCrashMessage(context, error, 'odoo.activate');
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

async function getStandalonePythonPath(context: ExtensionContext) {
    const config = await getCurrentConfig(context);
    const pythonPath = config && config["pythonPath"] ? config["pythonPath"] : "python3";
    return pythonPath
}

async function checkStandalonePythonVersion(context: ExtensionContext): Promise<boolean>{
    const currentConfig = await getCurrentConfig(context);
    if (!currentConfig){
        return
    }
    
    const pythonPath = currentConfig["pythonPath"]
    if (!pythonPath) {
        OUTPUT_CHANNEL.appendLine("[INFO] pythonPath is not set, defaulting to python3.");
    }

    const versionString = execSync(`${pythonPath} --version`).toString().replace("Python ", "")

    const pythonVersion = semver.parse(versionString)  
    if (!pythonVersion || semver.lt(pythonVersion, "3.8.0")) {
        window.showErrorMessage(
            `You must use python 3.8 or newer. Would you like to change it?`,
            "Update current configuration",
            "Ignore"
        ).then(selection => {
            switch (selection) {
                case ("Update current configuration"):
                    ConfigurationWebView.render(context, currentConfig.id);
                    break
            }
        });
        return false
    }
    return true
}
