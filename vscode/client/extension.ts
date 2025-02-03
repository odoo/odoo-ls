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
    QuickPickItem,
    StatusBarAlignment,
    ThemeIcon,
    workspace,
    window,
    QuickPickItemKind,
    TextDocument,
    OutputChannel,
    Uri,
    ConfigurationTarget,
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
    clientStopped
} from './common/events'
import {
    onDidChangePythonInterpreterEvent
} from "./common/python";
import { areUniquelyEqual, buildFinalPythonPath, evaluateOdooPath, getCurrentConfig, validateAddonPath } from "./common/utils";
import { getConfigurationStructure, stateInit } from "./common/validation";
import { execSync } from "child_process";
import {
    migrateConfigToSettings
} from "./migration/migrateConfig";
import { constants } from "fs/promises";
import { PVSC_EXTENSION_ID } from "@vscode/python-extension";


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
        command,
        args,
        options: { cwd, env: process.env },
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
    let configs = JSON.parse(JSON.stringify(workspace.getConfiguration().get("Odoo.configurations")));

    const newConf = getConfigurationStructure(configId);
    workspace.getConfiguration().update("Odoo.configurations",
    {
        ...configs,
        [configId]: newConf,
    },
    ConfigurationTarget.Global);

    IncrementLastConfigId(context);
    ConfigurationWebView.render(context, newConf);
}

async function changeSelectedConfig(context: ExtensionContext, configId: Number) {
    await workspace.getConfiguration().update("Odoo.selectedConfiguration", configId, ConfigurationTarget.Workspace);
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
    let client : LanguageClient;
    try {
        await updatePythonPath(context);
        if (!workspace.getConfiguration('Odoo').get("disablePythonLanguageServerPopup", false)){
            displayDisablePythonLSMessage();
        }

        global.SERVER_PID = 0;
        let serverPath = "./win_odoo_ls_server.exe";
        if (process.platform === 'darwin') {
            serverPath = "./macos_odoo_ls_server"
        } else if (process.platform !== 'win32') {
            serverPath = "./linux_odoo_ls_server"
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
            client.onNotification("$Odoo/invalid_python_path", async(params) => {
                await window.showErrorMessage(
                    "The Odoo extension is unable to start Python with the path you provided. Verify your configuration"
                );
            }),
            client.onNotification("Odoo/displayCrashNotification", async (params) => {
                await displayCrashMessage(context, params["crashInfo"], params["pid"]);
            }),
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

async function checkAddons(context: ExtensionContext) {
    let currentConfig = await getCurrentConfig(context);
    if (!currentConfig) {
        return
    }
    const validAddons = [];
    const invalidAddons = [];
    for (const addonPath of currentConfig.addons) {
        const validationResult = await validateAddonPath(addonPath);

        if (validationResult !== null) {
            validAddons.push(validationResult);
        } else {
            invalidAddons.push(addonPath);
        }
    }
    if (invalidAddons.length > 0) {
        const invalidPathsMessage = invalidAddons.join(", ");

        window.showWarningMessage(
            `The following addon paths in this configuration seem invalid: (${invalidPathsMessage}). Would you like to change the configuration?`,
            "Update current configuration",
            "Ignore"
        ).then(selection => {
            switch (selection) {
                case "Update current configuration":
                    ConfigurationWebView.render(context, currentConfig);
                    break;
            }
        });
    }
    let configs = JSON.parse(JSON.stringify(workspace.getConfiguration().get("Odoo.configurations")));
    if (areUniquelyEqual(currentConfig.validatedAddonsPaths, validAddons)){
        return
    }
    configs[currentConfig.id]["validatedAddonsPaths"] = validAddons;
    workspace.getConfiguration().update("Odoo.configurations", configs, ConfigurationTarget.Global);

    // Check if workspace folders could also be addons folder
    currentConfig = configs[currentConfig.id];
    let files = await workspace.findFiles('**/__manifest__.py')
    let missingFiles = files.filter(file => {
        return !(
            currentConfig.addons.some((addon) => file.fsPath.replaceAll("\\", "/").startsWith(addon)) ||
            file.fsPath.replaceAll("\\", "/").startsWith(currentConfig.odooPath)
        )
    })
    let missingPaths = [...new Set(missingFiles.map(file => {
        let filePath = file.fsPath.split(path.sep)
        return filePath.slice(0, filePath.length - 2).join(path.sep)
    }))]
    if (missingPaths.length > 0) {
        global.LSCLIENT.warn("Missing addon paths : " + JSON.stringify(missingPaths))
        window.showWarningMessage(
            "We detected addon paths that weren't added in the current configuration. Would you like to add them?",
            "Update current configuration",
            "View Paths",
            "Ignore"
        ).then(selection => {
            switch (selection) {
                case ("Update current configuration"):
                    ConfigurationWebView.render(context, currentConfig);
                    break
                case ("View Paths"):
                    global.LSCLIENT.outputChannel.show();
                    break
            }
        });
    }
}

async function checkOdooPath(context: ExtensionContext) {
    let currentConfig = await getCurrentConfig(context);
    global.OUTPUT_CHANNEL.appendLine("[INFO] checking odoo path ".concat(currentConfig.rawOdooPath))
    const odoo = await evaluateOdooPath(currentConfig.rawOdooPath);
    if (odoo){
        if (currentConfig.odooPath == odoo.path) return;
        let configs = JSON.parse(JSON.stringify(workspace.getConfiguration().get("Odoo.configurations")));
        configs[currentConfig.id]["odooPath"] = odoo.path;
        workspace.getConfiguration().update("Odoo.configurations", configs, ConfigurationTarget.Global);

    }else{
        window.showWarningMessage(
            `The odoo path set in this configuration seems invalid. Would you like to change it?`,
            "Update current configuration",
            "Ignore"
        ).then(selection => {
        switch (selection) {
            case ("Update current configuration"):
                ConfigurationWebView.render(context, currentConfig);
                break
            }
        })
        return
    }


    let odooFound = currentConfig ? workspace.getWorkspaceFolder(Uri.file(odoo.path)) : true
    if (!odooFound) {
        let invalidPath = false
        for (const f of workspace.workspaceFolders) {
            if (fs.existsSync(Uri.joinPath(f.uri, 'odoo-bin').fsPath) ||
                fs.existsSync(Uri.joinPath(Uri.joinPath(f.uri, 'odoo'), 'odoo-bin').fsPath)) {
                global.OUTPUT_CHANNEL.appendLine("invalid Path ".concat(f.uri.toString()))
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
                    ConfigurationWebView.render(context, currentConfig);
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

                const config = await getCurrentConfig(context)
                if (config) {
                    await checkOdooPath(context);
                    await checkAddons(context);
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
                    } else {
                        if (client.diagnostics) client.diagnostics.clear();
                    }
                } else {
                    if (global.LSCLIENT?.isRunning()) await stopClient();
                    global.IS_LOADING = false;
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

        extensions.onDidChange(async (_) => {
            const pyExtWasInstalled = global.IS_PYTHON_EXTENSION_READY === true;
            const pyExtInstalled = extensions.getExtension(PVSC_EXTENSION_ID) !== undefined;
            if (pyExtWasInstalled !== pyExtInstalled){
                await updatePythonPath(context, false);
            }
        }),

        // Listen to changes to Python Interpreter
        onDidChangePythonInterpreterEvent.event(async (_) => {
            await updatePythonPath(context, false);
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
            try {
                const qpick = window.createQuickPick();
                const configs = JSON.parse(JSON.stringify(workspace.getConfiguration().get("Odoo.configurations")));
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
                    if ((currentConfig && configId == currentConfig["id"]) || (Object.keys(configs[configId]).length === 0))
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
                        const buttonConfigId = (buttonEvent.item == currentConfigItem) ? currentConfig["id"] : configMap.get(buttonEvent.item);
                        const config = JSON.parse(JSON.stringify(workspace.getConfiguration().get("Odoo.configurations")))[buttonConfigId]
                        try {
                            ConfigurationWebView.render(context, config);
                        } catch (error) {
                            global.LSCLIENT.error(error);
                            await displayCrashMessage(context, error, global.SERVER_PID, 'render.ConfigurationWebView');
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
                            await displayCrashMessage(context, error, global.SERVER_PID, 'render.ConfigurationWebView')
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
                await displayCrashMessage(context, error, global.SERVER_PID, 'odoo.clickStatusBar')
            }
        }),
        commands.registerCommand(
            "odoo.disablePythonLanguageServerCommand", setPythonLSNone
            ),
        commands.registerCommand(
            "odoo.restartServer", async () => {
                if (global.LSCLIENT) {
                    global.LSCLIENT.restart();
                }
        })
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

function handleMigration(context){
    migrateConfigToSettings(context)
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
            global.LSCLIENT.start();
        }
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

export async function getStandalonePythonPath(context: ExtensionContext) {
    const config = await getCurrentConfig(context);
    const pythonPath = config && config["pythonPath"] ? config["pythonPath"] : "python3";
    return pythonPath
}

export async function getStandalonePythonVersion(python_path_from_config: string): Promise<semver.SemVer> {
    let pythonPath = python_path_from_config
    if (!python_path_from_config) {
        OUTPUT_CHANNEL.appendLine("[INFO] pythonPath is not set, defaulting to python3.");
        pythonPath = "python3"
    }

    try {
        const versionString = execSync(`${pythonPath} --version`).toString().replace("Python ", "")

        return semver.parse(versionString)
    } catch (error) {
        OUTPUT_CHANNEL.appendLine(`[ERROR] Failed to get python version: ${error}`);
        window.showErrorMessage(
            `Path to python executable is invalid. Please update the configuration. Used path: ${pythonPath}`,
        );
        return null
    }
}

/**
 * Check the version of the given path to Python
 * @param context ExtensionContext
 * @param python_path_from_config path to Python, that can differ from the one in the settings (for example because not yet saved)
 * @returns either valid version or invalid
 */
export async function checkStandalonePythonVersion(context, python_path_from_config: string): Promise<boolean>{
    const currentConfig = await getCurrentConfig(context);
    let pythonVersion = await getStandalonePythonVersion(python_path_from_config);
    if (!pythonVersion) {
        return false;
    }
    if (semver.lt(pythonVersion, "3.8.0")) {
        window.showErrorMessage(
            `You must use python 3.8 or newer. Would you like to change it?`,
            "Update current configuration",
            "Ignore"
        ).then(selection => {
            switch (selection) {
                case ("Update current configuration"):
                    ConfigurationWebView.render(context, currentConfig);
                    break
            }
        });
        return false
    }
    return true
}

async function updatePythonPath(context, outputLogs: boolean = true): Promise<boolean>{
	let configs = JSON.parse(JSON.stringify(workspace.getConfiguration().get("Odoo.configurations")));
	const selectedConfig: number = Number(workspace.getConfiguration().get('Odoo.selectedConfiguration'));
    // if config is disabled return nothing
	if (selectedConfig == -1 || !configs[selectedConfig]) {
		return null;
	}
	let config = (Object.keys(configs[selectedConfig]).length !== 0 ? configs[selectedConfig] : null);
    let pythonPath =  await getStandalonePythonPath(context);
    let finalPythonPath = await buildFinalPythonPath(context, pythonPath);
    if (config) {
        if (config["finalPythonPath"]) {
            if (config["finalPythonPath"] === finalPythonPath)
                return false;
        }
        config["finalPythonPath"] = finalPythonPath;
        workspace.getConfiguration().update("Odoo.configurations", configs, ConfigurationTarget.Global);
    }
    return true
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