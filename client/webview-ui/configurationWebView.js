/* eslint-disable no-undef */
// file: webview-ui/main.js

const vscode = acquireVsCodeApi();

window.addEventListener("load", main);

function main() {
  const howdyButton = document.getElementById("save_config");
  howdyButton.addEventListener("click", saveConfigClick);
}

function saveConfigClick() {
    vscode.postMessage({
        command: "save_config",
        name: document.getElementById("name").value,
        odooPath: document.getElementById("odooPath").value
    });
}