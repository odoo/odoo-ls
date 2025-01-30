import { ExtensionContext, Uri, Webview, window, workspace } from "vscode";
import * as fs from 'fs';
import * as path from "path";
import { URI } from "vscode-languageclient";
import untildify from 'untildify';
import * as readline from 'readline';
import { getInterpreterDetails, IInterpreterDetails, initializePython } from "./python";
import { checkStandalonePythonVersion, getStandalonePythonPath } from "../extension";


/**
 * A helper function which will get the webview URI of a given file or resource.
 *
 * @remarks This URI can be used within a webview's HTML as a link to the
 * given file/resource.
 *
 * @param webview A reference to the extension webview
 * @param extensionUri The URI of the directory containing the extension
 * @param pathList An array of strings representing the path to a file/resource
 * @returns A URI pointing to the file/resource
 */
export function getUri(webview: Webview, extensionUri: Uri, pathList: string[]) {
	return webview.asWebviewUri(Uri.joinPath(extensionUri, ...pathList));
}

export function getNonce() {
	let text = '';
	const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
	for (let i = 0; i < 32; i++) {
		text += possible.charAt(Math.floor(Math.random() * possible.length));
	}
	return text;
}

// Config related utils

export async function getCurrentConfig(context: ExtensionContext) {
	const configs = JSON.parse(JSON.stringify(workspace.getConfiguration().get("Odoo.configurations")));
	const activeConfig: number = Number(workspace.getConfiguration().get('Odoo.selectedConfiguration'));

	// if config is disabled return nothing
	if (activeConfig == -1 || !configs[activeConfig]) {
		return null;
	}
	return (Object.keys(configs[activeConfig]).length !== 0 ? configs[activeConfig] : null);
}

export function isReallyModule(directoryPath: string, moduleName: string): boolean {
	const fullPath = path.join(directoryPath, moduleName, "__manifest__.py");
	return fs.existsSync(fullPath) && fs.lstatSync(fullPath).isFile();
}

export function isAddonPath(directoryPath: string): boolean {
	return fs.existsSync(directoryPath) && fs.statSync(directoryPath).isDirectory() && fs.readdirSync(directoryPath).some((name) =>
		isReallyModule(directoryPath, name)
	);
}

export async function fillTemplate(template, vars = {}) {
	const handler = new Function('vars', [
		'const tagged = ( ' + Object.keys(vars).join(', ') + ' ) =>',
		'`' + template + '`',
		'return tagged(...Object.values(vars))'
	].join('\n'));
	try {
		return handler(vars);
	} catch (error) {
		if (error instanceof ReferenceError) {
			const missingVariableMatch = error.message.match(/(\w+) is not defined/);
			if (missingVariableMatch) {
				const missingVariable = missingVariableMatch[1];
				window.showErrorMessage(`Invalid path template paramater "${missingVariable}". Only "workspaceFolder" and "userHome" are currently supported`)
			}
		}
		throw error;
	}
}

export async function validateAddonPath(addonPath) {
	addonPath = addonPath.replaceAll("\\", "/");
	for (const folder of workspace.workspaceFolders) {
		const PATH_VAR_LOCAL = { ...global.PATH_VARIABLES };
		PATH_VAR_LOCAL["workspaceFolder"] = folder.uri.fsPath.replaceAll("\\", "/");
		let filledPath = path.resolve(await fillTemplate(addonPath, PATH_VAR_LOCAL)).replaceAll("\\", "/").trim();
		if (!filledPath) continue;
		do {
			if (isAddonPath(filledPath)) {
				return filledPath;
			}
			filledPath = path.dirname(filledPath);
		} while (path.parse(filledPath).root != filledPath);
	}
	return null;
}

export async function evaluateOdooPath(odooPath) {
	if (!odooPath) {
		return
	}
	odooPath = odooPath.replaceAll("\\", "/");


	for (const folder of workspace.workspaceFolders) {
		global.PATH_VARIABLES["workspaceFolder"] = folder.uri.fsPath.replaceAll("\\", "/");
		let filledOdooPath = path.resolve(await fillTemplate(odooPath, global.PATH_VARIABLES)).replaceAll("\\", "/").trim();
		do {
			const version = await getOdooVersion(filledOdooPath);
			if (version) {
				return { "path": filledOdooPath, "version": version };
			}
			filledOdooPath = path.dirname(filledOdooPath);
		} while (path.parse(filledOdooPath).root != filledOdooPath);
	}
	return null;
}

export async function getOdooVersion(odooPath: URI) {
	let versionString = null;
	const releasePath = untildify(odooPath) + '/odoo/release.py';
	if (fs.existsSync(releasePath)) {
		const rl = readline.createInterface({
			input: fs.createReadStream(releasePath),
			crlfDelay: Infinity,
		});

		for await (const line of rl) {
			if (line.startsWith('version_info')) {
				versionString = line;
				// Folder is invalid if we don't find any version info
				if (!versionString) {
					return null;
				} else {
					// Folder is valid if a version was found
					const versionRegEx = /\(([^)]+)\)/; // Regex to obtain the info in the parentheses
					const versionArray = versionRegEx.exec(versionString)[1].split(', ');
					const version = `${versionArray[0]}.${versionArray[1]}.${versionArray[2]}` + (versionArray[3] == 'FINAL' ? '' : ` ${versionArray[3]}${versionArray[4]}`);
					return version;
				}
			}
		}
	} else {
		// Folder is invalid if odoo/release.py was never found
		return null;
	}
}

export function areUniquelyEqual<T>(a: Array<T>, b: Array<T>): boolean {
	if (!(Array.isArray(a) && Array.isArray(b))) return false;
	const setA = new Set(a);
	const setB = new Set(b);
	return setA.size === setB.size && [...setA].every(val => setB.has(val));
}

export async function buildFinalPythonPath(context, config_python_path: string, outputLogs: boolean = true): Promise<String> {
	let pythonPath: string = "";
	let interpreter: IInterpreterDetails;
	try {
		interpreter = await getInterpreterDetails();
	} catch {
		interpreter = null;
	}

	//trying to use the VScode python extension
	if (interpreter && global.IS_PYTHON_EXTENSION_READY !== false) {
		pythonPath = interpreter.path[0]
		await initializePython(context.subscriptions);
		global.IS_PYTHON_EXTENSION_READY = true;
	} else {
		global.IS_PYTHON_EXTENSION_READY = false;
		//python extension is not available switch to standalone mode
		if (await checkStandalonePythonVersion(context, config_python_path)) {
			pythonPath = config_python_path;
		}
	}
	if (outputLogs){
		global.OUTPUT_CHANNEL.appendLine("[INFO] Python VS code extension is ".concat(global.IS_PYTHON_EXTENSION_READY ? "ready" : "not ready"));
		global.OUTPUT_CHANNEL.appendLine("[INFO] Using Python at : ".concat(pythonPath));
	}
	return pythonPath
}