import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';


class Database extends vscode.TreeItem {
    constructor(
        public readonly label: string,
        private version: string,
        public readonly collapsibleState: vscode.TreeItemCollapsibleState
    ) {
        super(label, collapsibleState);
        this.tooltip = `${this.label}-${this.version}`;
        this.description = this.version;
    }

    iconPath = new vscode.ThemeIcon('database');
}

export class TreeDatabasesDataProvider implements vscode.TreeDataProvider<Database> {

    getTreeItem(element: Database): vscode.TreeItem {
        return element;
    }

    getChildren(element?: Database): Thenable<Database[]> {
        if (element && element.label == "Databases") {
            return Promise.resolve(this.get_all_db());
        } else {
            return Promise.resolve([new Database(
                    "Databases",
                    "",
                    vscode.TreeItemCollapsibleState.Collapsed
                )]);


            /*const packageJsonPath = path.join(this.workspaceRoot, 'package.json');
            if (this.pathExists(packageJsonPath)) {
                return Promise.resolve(this.getDepsInPackageJson(packageJsonPath));
            } else {
                vscode.window.showInformationMessage('Workspace has no package.json');
                return Promise.resolve([]);
            }*/
        }
    }

    private get_all_db(): Database[] {
        /*const { Client } = require('pg');
        const client = new Client({
          user: 'sgpostgres',
          host: 'SG-PostgreNoSSL-14-pgsql-master.devservers.scalegrid.io',
          database: 'postgres',
          password: 'password',
          port: 5432,
        })
        client.connect(function(err) {
          if (err) throw err;
          console.log("Connected!");
        });*/
        return [];
    }

    private pathExists(p: string): boolean {
        try {
            fs.accessSync(p);
        } catch (err) {
            return false;
        }
        return true;
    }
}
