const vscode = acquireVsCodeApi();

window.addEventListener("load", main);

function main() {
    const sendReportButton = document.getElementById('send-report-button');

    sendReportButton.addEventListener("click", sendCrashReport);
}

function sendCrashReport() {
    vscode.postMessage({
        command: "send_report",
        additional_info: document.querySelector('#crash-report-form').data["additional_info"]
    });
}