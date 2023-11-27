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
  }
});

function main() {
  const addFolderButton = document.getElementById('add-folder-button');
  const pathTextfield = document.getElementById('config-path-textfield');
  const pathButton = document.getElementById('config-path-button');
  const pythonPathButton = document.getElementById('config-python-path-button');
  const saveButton = document.getElementById('save-button');
  const deleteButton = document.getElementById('delete-button');

  addFolderButton.addEventListener("click", addFolderClick);
  pathTextfield.addEventListener("vsc-change", updateVersion);
  pathButton.addEventListener('vsc-click', openOdooFolder);
  pythonPathButton.addEventListener('vsc-click', openPythonPath);
  saveButton.addEventListener('click', saveConfig);
  deleteButton.addEventListener('click', deleteConfig);

  // Send a message to notify the extension 
  // that the DOM is loaded and ready.
  vscode.postMessage({
    command: 'view_ready' 
  });
}

function saveConfig() {
  vscode.postMessage({
      command: "save_config",
      name: document.getElementById("config-name-textfield").value,
      odooPath: document.getElementById("config-path-textfield").value,
      addons: getAddons(),
      pythonPath: document.getElementById("config-python-path-textfield").value,
  });
}

function openPythonPath() {
  vscode.postMessage({
    command: "open_python_path"
  });
}

function addFolderClick() {
  vscode.postMessage({
    command: "add_addons_folder"
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
    odooPath: document.getElementById("config-path-textfield").value,
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
