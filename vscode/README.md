<h1 align="center">
  <br>
  <a href="https://marketplace.visualstudio.com/items?itemName=Odoo.odoo">
  <img src="https://github.com/odoo/odoo-ls/blob/master/vscode/images/odoo_logo.png?raw=true"></a>
  <br>
  Visual Studio Extension
  <br>
</h1>

<h4 align="center">Boost your Odoo code development</h4>

## About

This extension integrates the Odoo Language Server, that will help you in the development of your Odoo projects.

**This project is currently under active development. This is a complex project, and you can encounter various issues, incoherent data or crashes. Do not hesitate to report them to help us build the perfect tool !**

## Features

- Autocompletion
  - Simple Autocompletion
  ![Autocompletion picture](https://raw.githubusercontent.com/odoo/odoo-ls/master/vscode/images/autocomplete.png "Autocompletion")
  - Model fields Completion
  ![Autocompletion in a loop](https://raw.githubusercontent.com/odoo/odoo-ls/master/vscode/images/autocompletion2.png "Autocompletion 2")
  - Smart String completion
  ![Autocompletion in a decorator](https://raw.githubusercontent.com/odoo/odoo-ls/master/vscode/images/autocompletion2.png "Autocompletion 3")
  ![Autocompletion in self.env](https://raw.githubusercontent.com/odoo/odoo-ls/master/vscode/images/autocompletion3.png "Autocompletion 4")
  ![Autocompletion in inverse kwarg](https://raw.githubusercontent.com/odoo/odoo-ls/master/vscode/images/autocompletion4.png "Autocompletion 5")

- Advanced Hover and GoToDefinition
  Even on String values!
  ![Advanced Hover and GoToDefinition](https://raw.githubusercontent.com/odoo/odoo-ls/master/vscode/images/advanced_hover_def.gif "Autocompletion 5")

- Diagnostics

![diagnostics picture](https://raw.githubusercontent.com/odoo/odoo-ls/master/vscode/images/diagnostics.png "Diagnostics")
![diagnostics picture 2](https://raw.githubusercontent.com/odoo/odoo-ls/master/vscode/images/diagnostics2.png "Diagnostics2")

## Installation

### Requirements

- Odoo 14+
- Python 3.8+

### Automatic installation

Install the extension from the marketplace
- VsCode: [link](https://marketplace.visualstudio.com/items?itemName=Odoo.odoo)
- VsCodium: [link](https://open-vsx.org/extension/Odoo/odoo)

### Manually build the .vsix package

#### Requirements

- Python 3.8 or greater
- An active virtual environment (`python3 -m venv venv`)
- nox (`pip install nox`)
- node >= 14.19.0
- npm >= 8.3.0 (`npm` is installed with node, check npm version, use `npm install -g npm@8.3.0` to update)
- @vscode/vsce >= 3.2.1 (`npm i -g @vscode/vsce`)

#### How to bundle into .vsix

- Activate the nox venv.
- Install nox if not installed yet.
- Run `build_package.sh
`