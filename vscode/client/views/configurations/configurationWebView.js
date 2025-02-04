/* eslint-disable no-undef */
const vscode = acquireVsCodeApi();

window.addEventListener("load", main);
window.addEventListener("message", event => {
  const message = event.data;
  switch (message.command) {
    case "render_addons":
      renderAddonsTree(message.addons);
      break;
    case "update_path":
      const pathField = document.getElementById('config-path-textfield');
      pathField.focus();
      pathField.setAttribute("value", message.path);
      break
    case "update_python_path":
      const pythonPathField = document.getElementById('config-python-path-textfield');
      pythonPathField.focus();
      pythonPathField.setAttribute("value", message.pythonPath);
      break
    case "update_config_folder_validity":
      const pathHelper = document.getElementById('config-path-helper');
      if (message.version) {
        pathHelper.innerHTML = `<p id="path-helper-valid">Valid Odoo installation detected (${message.version}).</p>`;
      }
      else {
        pathHelper.innerHTML = '<p id="path-helper-invalid">No Odoo installation detected.</p>';
      }
      break
    case "read_addons_folder":
      const addonInput = document.getElementById('addons-folder-input');
      addonInput.focus();
      addonInput.setAttribute("value", message.addonPath);
      break
    case "clear_addons_folder":
      const addonInputField = document.getElementById('addons-folder-input');
      addonInputField.focus();
      addonInputField.setAttribute("value", '');
      break
    case "update_python_path_validity":
      const pythonPathHelper = document.getElementById('config-python-path-helper');
      if (message.valid === true) {
        pythonPathHelper.innerHTML = `<p id="path-helper-valid">Valid Python path.</p>`;
      }
      else {
        pythonPathHelper.innerHTML = '<p id="path-helper-invalid">Invalid Python path.</p>';
      }
      break
  }
});

function main() {
  const addFolderButton = document.getElementById('config-addons-path-button');
  const addAddonPathButton = document.getElementById('add-addon-path-button');
  const pathTextfield = document.getElementById('config-path-textfield');
  const pathButton = document.getElementById('config-path-button');
  const pythonPathButton = document.getElementById('config-python-path-button');
  const pythonPathField = document.getElementById('config-python-path-textfield');
  const saveButton = document.getElementById('save-button');
  const deleteButton = document.getElementById('delete-button');

  addFolderButton.addEventListener("vsc-click", addFolderClick);
  addAddonPathButton.addEventListener("click", addAddonPathClick);
  pathTextfield.addEventListener("vsc-change", updateVersion);
  pathButton.addEventListener('vsc-click', openOdooFolder);
  if (pythonPathButton){
    pythonPathButton.addEventListener('vsc-click', openPythonPath);
    pythonPathField.addEventListener('vsc-input', changePythonPath);
  }
  saveButton.addEventListener('click', saveConfig);
  deleteButton.addEventListener('click', deleteConfig);

  // Send a message to notify the extension
  // that the DOM is loaded and ready.
  vscode.postMessage({
    command: 'view_ready'
  });
}

function saveConfig() {
  let pythonPath = document.getElementById("config-python-path-textfield");
  if (!pythonPath){
    pythonPath=undefined
  }else{
    pythonPath=pythonPath.value
  }

  vscode.postMessage({
      command: "save_config",
      name: document.getElementById("config-name-textfield").value,
      rawOdooPath: document.getElementById("config-path-textfield").value,
      addons: getAddons(),
      pythonPath: pythonPath,
  });
}

function openPythonPath() {
  vscode.postMessage({
    command: "open_python_path"
  });
}

function changePythonPath() {
  vscode.postMessage({
    command: "change_python_path",
    pythonPath: document.getElementById("config-python-path-textfield").value,
  });
}

function addFolderClick() {
  vscode.postMessage({
    command: "add_addons_folder"
  });
}

function addAddonPathClick() {
  vscode.postMessage({
    command: "add_addons_path",
    addonPath: document.getElementById("addons-folder-input").value,
  });
}

function deleteAddon(addons){
  vscode.postMessage({
    command: "delete_addons_folder",
    addons: addons,
  });
}

function openOdooFolder() {
  vscode.postMessage({
    command: "open_odoo_folder"
  });
}

function deleteConfig() {
  vscode.postMessage({
    command: "delete_config"
  });
}

function updateVersion(){
  vscode.postMessage({
    command: "update_version",
    rawOdooPath: document.getElementById("config-path-textfield").value,
  })
}
function renderAddonsTree(addons) {
  const tree = document.getElementById('addons-tree');
  const icons = {
    leaf: 'folder'
  };

  const actions = [
    {
      icon: 'trash',
      actionId: 'delete',
      tooltip: 'Delete',
    },
  ];

  let data = [];
  for (let i = 0; i < addons.length; i++) {
    data.push({icons, actions, label: addons[i]});
  }
  tree.data = data;

  tree.addEventListener("vsc-run-action", event => {
    const action = event.detail;
    switch (action.actionId) {
      case "delete":
        data.splice(action.item.path[0], 1);
        tree.data = data;
        deleteAddon(getAddons(tree.data));
        break;
    }
  });
}

function getAddons() {
  const tree = document.getElementById('addons-tree');
  let addons = [];

  tree.data.forEach(element => {
    addons.push(element.label);
  });
  return addons;
}
