const vscode = require('vscode');

/**
 * Force the Watch panel to refresh by submitting an empty command
 * through the debug console REPL (equivalent to pressing Enter).
 */
async function refreshWatchPanel() {
    try {
        await vscode.commands.executeCommand('workbench.debug.action.focusRepl');
        await new Promise(r => setTimeout(r, 50));
        await vscode.commands.executeCommand('repl.action.acceptInput');
    } catch (_) {}
}

const watchedExpressions = new Set();

async function addWatchIfMissing(expression) {
    if (watchedExpressions.has(expression)) return;
    try {
        await vscode.commands.executeCommand('debug.addToWatchExpressions', {
            variable: { evaluateName: expression }
        });
        watchedExpressions.add(expression);
    } catch (_) {}
}

function activate(context) {
    context.subscriptions.push(
        vscode.commands.registerCommand('slotmap.lookup', async (variable) => {
            const session = vscode.debug.activeDebugSession;
            if (!session) {
                vscode.window.showErrorMessage('No active debug session');
                return;
            }

            let keyName = variable?.variable?.name;
            if (!keyName) {
                keyName = await vscode.window.showInputBox({
                    prompt: 'Key variable name',
                    placeHolder: 'k',
                });
                if (!keyName) return;
            }

            const mapName = vscode.workspace.getConfiguration('slotmap').get('mapVariable', 'sm');

            try {
                await session.customRequest('evaluate', {
                    expression: `-exec slotmap_get ${mapName} ${keyName}`,
                    context: 'repl',
                });
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
                    await session.customRequest('evaluate', {
                        expression: `-exec slotmap_get ${mapNameNew} ${keyName}`,
                        context: 'repl',
                    });
                } catch (e2) {
                    vscode.window.showErrorMessage(`SlotMap lookup failed: ${e2.message}`);
                    return;
                }
            }

            // Add $slot to Watch panel if not already there
            await addWatchIfMissing('$slot');
            await refreshWatchPanel();
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
