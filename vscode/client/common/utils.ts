import { ExtensionContext, Uri, Webview, workspace} from "vscode";
import * as fs from 'fs';
import { URI } from "vscode-languageclient";
import untildify from 'untildify';
import * as readline from 'readline';


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
    return (configs && activeConfig > -1 ? configs[activeConfig] : null);
}

export async function evaluateOdooPath(odooPath){
	const workspaceFolders = workspace.workspaceFolders;

	const fillTemplate = (template, vars = {}) => {
		const handler = new Function('vars', [
			'const tagged = ( ' + Object.keys(vars).join(', ') + ' ) =>',
			'`' + template + '`',
			'return tagged(...Object.values(vars))'
		].join('\n'));
		const res = handler(vars)
		return res;
	};


	for (const i in workspaceFolders){
		const folder = workspaceFolders[i];
		let PATH_VAR_LOCAL = global.PATH_VARIABLES;
		PATH_VAR_LOCAL["workspaceFolder"] = folder.uri.path;
		odooPath = fillTemplate(odooPath,PATH_VAR_LOCAL);
		const version = await getOdooVersion(odooPath);
		if (version){
			global.PATH_VARIABLES["workspaceFolder"] = folder.uri.path; 
			return {"path": odooPath,"version": version};
		}
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
