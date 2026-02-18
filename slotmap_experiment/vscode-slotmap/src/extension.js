const vscode = require('vscode');

/**
 * After setting a GDB convenience variable, VS Code's Watch panel doesn't
 * know the value changed (no "stop" event occurred). We force a refresh
 * by executing a harmless step-granularity evaluation that cpptools
 * recognizes as state-changing, causing it to invalidate variables.
 */
async function refreshWatchPanel() {
    // customRequest('evaluate') bypasses VS Code's debug console widget,
    // so the Watch panel never refreshes. We need to go through the actual
    // REPL widget: focus it and submit an empty command (= pressing Enter).
    try {
        await vscode.commands.executeCommand('workbench.debug.action.focusRepl');
        await new Promise(r => setTimeout(r, 50));
        await vscode.commands.executeCommand('repl.action.acceptInput');
    } catch (_) {}
}

async function runSlotmapGet(session, mapName, keyName) {
    await session.customRequest('evaluate', {
        expression: `-exec slotmap_get ${mapName} ${keyName}`,
        context: 'repl',
    });
    await refreshWatchPanel();
}

function activate(context) {
    context.subscriptions.push(
        vscode.commands.registerCommand('slotmap.lookup', async (variable) => {
            const session = vscode.debug.activeDebugSession;
            if (!session) {
                vscode.window.showErrorMessage('No active debug session');
                return;
            }

            // Get the key variable name from the context menu target
            let keyName = variable?.variable?.name;
            if (!keyName) {
                keyName = await vscode.window.showInputBox({
                    prompt: 'Key variable name',
                    placeHolder: 'k',
                });
                if (!keyName) return;
            }

            // Get the map variable name from settings
            let mapName = vscode.workspace.getConfiguration('slotmap').get('mapVariable', 'sm');

            // Execute the GDB command
            try {
                await runSlotmapGet(session, mapName, keyName);
            } catch (e) {
                // Map not found — prompt for the name
                const mapNameNew = await vscode.window.showInputBox({
                    prompt: 'SlotMap variable not found. Enter map variable name:',
                    placeHolder: 'sm',
                    value: mapName,
                });
                if (!mapNameNew) return;

                if (mapNameNew !== mapName) {
                    await vscode.workspace.getConfiguration('slotmap').update(
                        'mapVariable', mapNameNew, vscode.ConfigurationTarget.Workspace
                    );
                }

                try {
                    await runSlotmapGet(session, mapNameNew, keyName);
                } catch (e2) {
                    vscode.window.showErrorMessage(`SlotMap lookup failed: ${e2.message}`);
                }
            }
        }),

        vscode.commands.registerCommand('slotmap.setMap', async () => {
            const name = await vscode.window.showInputBox({
                prompt: 'SlotMap variable name',
                placeHolder: 'sm',
                value: vscode.workspace.getConfiguration('slotmap').get('mapVariable', 'sm'),
            });
            if (name) {
                await vscode.workspace.getConfiguration('slotmap').update(
                    'mapVariable', name, vscode.ConfigurationTarget.Workspace
                );
                vscode.window.showInformationMessage(`SlotMap variable set to: ${name}`);
            }
        })
    );
}

function deactivate() {}

module.exports = { activate, deactivate };
