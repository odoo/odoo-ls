/* eslint-disable no-undef */
// file: webview-ui/main.js

const vscode = acquireVsCodeApi();

window.addEventListener("load", main);

function main() {
  const showWelcomeCheckbox = document.getElementById("displayOdooWelcomeOnStart");
  showWelcomeCheckbox.addEventListener("change", changeDisplayWelcomeValue);
}

function changeDisplayWelcomeValue() {
    vscode.postMessage({
        command: "changeWelcomeDisplayValue",
        toggled: document.getElementById("displayOdooWelcomeOnStart").checked
    });
}