const vscode = require('vscode');

let outputChannel;

/**
 * Fix evaluateName paths from the debug adapter to work with GDB.
 *
 * The debug adapter uses enum variant names for tuple variants:
 *   (($slot_l).Function).__0
 * But GDB's expression parser needs numeric field access:
 *   ($slot_l).0
 *
 * Also handles Option variants:
 *   ((expr).parent).Some  →  (expr).parent
 * since slotmap_get handles Option unwrapping internally.
 *
 * Strategy: walk the container chain to rebuild the path using
 * the known type info, replacing variant names with .0 for tuple
 * variants and stripping Option::Some wrappers.
 */
function fixEvaluateName(evaluateName, contextArg) {
    // Collect the chain: variable + container ancestors
    // Each level tells us the field name and the type
    const chain = [];
    if (contextArg?.variable) {
        chain.unshift(contextArg.variable);
    }
    if (contextArg?.container) {
        chain.unshift(contextArg.container);
    }

    // Pattern: ((expr).VariantName).__0  →  (expr).0
    // The debug adapter wraps in parens and uses .VariantName for enum access,
    // followed by .__0 for the tuple field. We replace both with just .0
    let fixed = evaluateName;

    // Replace ).VariantName).__0  with ).0
    // This handles tuple enum variants like Symbol::Function(inner)
    // Pattern: (expr).VariantName).__0  where VariantName is a capitalized identifier
    fixed = fixed.replace(/\)\.([A-Z][a-zA-Z0-9_]*)\)\.__0/g, ').0)');

    // Remove outermost unnecessary parens layer by layer
    // ((($slot_l).0)).parent → ($slot_l).0.parent
    // But be careful to only strip balanced outer parens
    while (fixed.startsWith('(') && fixed.endsWith(')')) {
        // Check if the outer parens are actually balanced wrapping
        const inner = fixed.slice(1, -1);
        let depth = 0;
        let balanced = true;
        for (const ch of inner) {
            if (ch === '(') depth++;
            if (ch === ')') depth--;
            if (depth < 0) { balanced = false; break; }
        }
        if (balanced && depth === 0) {
            fixed = inner;
        } else {
            break;
        }
    }

    // Strip .Some at the end (Option::Some wrapper) — slotmap_get unwraps Options
    fixed = fixed.replace(/\.Some$/, '');

    // Also handle nested .Some.__0 patterns (non-niche Option representation)
    fixed = fixed.replace(/\.Some\.__0/g, '');

    return fixed;
}

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

async function addWatchIfMissing(context, expression) {
    const key = 'slotmap.watched';
    const watched = context.workspaceState.get(key, []);
    if (watched.includes(expression)) return;
    try {
        await vscode.commands.executeCommand('debug.addToWatchExpressions', {
            variable: { evaluateName: expression }
        });
        watched.push(expression);
        await context.workspaceState.update(key, watched);
    } catch (_) {}
}

function activate(context) {
    outputChannel = vscode.window.createOutputChannel('SlotMap');

    context.subscriptions.push(
        vscode.commands.registerCommand('slotmap.lookup', async (variable) => {
            const session = vscode.debug.activeDebugSession;
            if (!session) {
                vscode.window.showErrorMessage('No active debug session');
                return;
            }

            // Log the full variable object so we can see what VS Code gives us
            outputChannel.appendLine(`--- SlotMap Lookup ---`);
            outputChannel.appendLine(`variable arg: ${JSON.stringify(variable, null, 2)}`);

            // Prefer evaluateName (full GDB expression) over name (field name only).
            // The debug adapter generates paths like (($slot_l).Function).__0
            // using enum variant names, but GDB needs numeric access for tuple
            // variants: ($slot_l).0. We rewrite these paths.
            let keyExpr = variable?.variable?.evaluateName || variable?.variable?.name;
            outputChannel.appendLine(`keyExpr (raw): ${keyExpr}`);

            if (keyExpr) {
                keyExpr = fixEvaluateName(keyExpr, variable);
                outputChannel.appendLine(`keyExpr (fixed): ${keyExpr}`);
            }

            if (!keyExpr) {
                keyExpr = await vscode.window.showInputBox({
                    prompt: 'Key expression (variable name or struct.field path)',
                    placeHolder: 'k',
                });
                if (!keyExpr) return;
            }

            const mapName = vscode.workspace.getConfiguration('slotmap').get('mapVariable', 'sm');
            const cmd = `slotmap_get ${mapName} ${keyExpr}`;
            outputChannel.appendLine(`GDB command: ${cmd}`);
            outputChannel.show(true); // show but don't steal focus

            try {
                const result = await session.customRequest('evaluate', {
                    expression: `-exec ${cmd}`,
                    context: 'repl',
                });
                outputChannel.appendLine(`Result: ${JSON.stringify(result)}`);
            } catch (e) {
                outputChannel.appendLine(`Error: ${e.message}`);
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
                        expression: `-exec slotmap_get ${mapNameNew} ${keyExpr}`,
                        context: 'repl',
                    });
                } catch (e2) {
                    vscode.window.showErrorMessage(`SlotMap lookup failed: ${e2.message}`);
                    return;
                }
            }

            // Build a clean suffix for the watch variable name
            // (mirrors the sanitize_var_name logic in slotmap_gdb.py)
            const varSuffix = keyExpr.replace(/^\$/, '').replace(/[^a-zA-Z0-9_]/g, '_').replace(/_+/g, '_').replace(/^_|_$/g, '');

            // Add watch expressions: $slot (latest) and $slot_<suffix> (per-lookup)
            await addWatchIfMissing(context, '$slot');
            await addWatchIfMissing(context, `$slot_${varSuffix}`);
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
        }),

        // Command to reset tracked watch expressions (if you cleaned up manually)
        vscode.commands.registerCommand('slotmap.resetWatched', async () => {
            await context.workspaceState.update('slotmap.watched', []);
            vscode.window.showInformationMessage('SlotMap: watch expression tracking reset');
        })
    );
}

function deactivate() {}

module.exports = { activate, deactivate };
