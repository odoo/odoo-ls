<h1 align="center">
  <br>
  <a href="https://marketplace.visualstudio.com/items?itemName=Odoo.odoo">
  <img src="https://github.com/odoo/odoo-ls/blob/master/vscode/images/odoo_logo.png?raw=true"></a>
  <br>
  Odoo
  <br>
</h1>

<h4 align="center">Boost your Odoo code development</h4>

## About

This extension integrates the Odoo Language Server, that will help you in the development of your Odoo projects.

**This project is currently under active development. This is a complex project, and you can encounter various issues, incoherent data or crashs. Do not hesitate to report them to help us building the perfect tool !**

## Features

- Autocompletion
- Show definition on hover
- Go to definition
- Diagnostics

## Installation

### Requirements

- Odoo 14+
- Python 3.8+

### Automatic installation

Install the extension from the marketplace
- VsCode: [link]()
- VsCodium: [link]()

### Manually build the .vsix package

#### Requirements

- Python 3.8 or greater
- An active virtual environment (`python3 -m venv venv`)
- nox (`pip install nox`)
- node >= 14.19.0
- npm >= 8.3.0 (`npm` is installed with node, check npm version, use npm install -g npm@8.3.0 to update)

#### How to bundle into .vsix

- Activate the nox venv.
- Install nox if not installed yet.
- Run `build_package.sh
`