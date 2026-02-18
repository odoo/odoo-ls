const vscode = require('vscode');

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
                const result = await session.customRequest('evaluate', {
                    expression: `-exec slotmap_get ${mapName} ${keyName}`,
                    context: 'repl',
                });

                // If it failed (map not found), prompt for the map name
                if (result.result && result.result.includes('No symbol')) {
                    mapName = await vscode.window.showInputBox({
                        prompt: 'SlotMap variable name',
                        placeHolder: 'sm',
                        value: mapName,
                    });
                    if (!mapName) return;

                    // Save the new default
                    await vscode.workspace.getConfiguration('slotmap').update(
                        'mapVariable', mapName, vscode.ConfigurationTarget.Workspace
                    );

                    // Retry
                    await session.customRequest('evaluate', {
                        expression: `-exec slotmap_get ${mapName} ${keyName}`,
                        context: 'repl',
                    });
                }
            } catch (e) {
                // cpptools returns errors as exceptions; prompt for map name
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
