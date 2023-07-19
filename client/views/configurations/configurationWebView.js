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
  const nameTextfield = document.getElementById('config-name-textfield');
  const pathTextfield = document.getElementById('config-path-textfield');
  const pathButton = document.getElementById('config-path-button');

  addFolderButton.addEventListener("click", addFolderClick);
  nameTextfield.addEventListener("vsc-change", saveConfig);
  pathTextfield.addEventListener("vsc-change", saveConfig);
  pathButton.addEventListener('vsc-click', openOdooFolder);


  // Send a message to notify the extension 
  // that the DOM is loaded and ready.
  vscode.postMessage({
    command: 'view_ready' 
  });
}

function saveConfig() {
  console.log('Trigger saveConfig');
  vscode.postMessage({
      command: "save_config",
      name: document.getElementById("config-name-textfield").value,
      odooPath: document.getElementById("config-path-textfield").value,
      addons: getAddons()
  });
}

function addFolderClick() {
  vscode.postMessage({
    command: "add_addons_folder"
  });
}

function openOdooFolder() {
  vscode.postMessage({
    command: "open_odoo_folder"
  });
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
        saveConfig();
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
