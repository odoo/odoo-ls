import { OutputChannel, StatusBarItem } from "vscode";
import {
   LanguageClient,
} from "vscode-languageclient/node";

declare global {
   var LSCLIENT: LanguageClient;
   var STATUS_BAR: StatusBarItem;
   var OUTPUT_CHANNEL: OutputChannel;
   var IS_LOADING: boolean;
   var DEBUG_FILE: string;
   var CLIENT_IS_STOPPING: boolean;
   var CAN_QUEUE_CONFIG_CHANGE: boolean;
   var IS_PYTHON_EXTENSION_READY: boolean;
}
