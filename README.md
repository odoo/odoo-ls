# Odoo Language Server

This repository contains a language server for the Odoo framework that will provide autocompletion, file validation, hover requests, go to definition, and more. This language server is made available for your favorite IDE with the different extensions of this repository.
Actually only vscode is available, but others will come later.
To learn more about language servers, read https://microsoft.github.io/language-server-protocol/
Please consult the readme of each directory to learn more about each project.

## Table of Contents

- [List of projects](#list-of-projects)
- [State of the project](#state-of-the-project)
- [Wiki](#wiki)
- [Contributing](#contributing)
- [License](#license)

## List of projects

### Language Server

A generic language server that can be used to provide common IDE features to your IDE as well as a command line tool.
It can provide autocompletion, hovering, go to definition, diagnostics, document symbols, etc...

### VsCode Extension

An extension that will bundle the Odoo Language Server and give needed settings and some UI improvements to your vscode.
The VsCode extension can be found here: https://github.com/odoo/odoo-vscode

### PyCharm plugin

That plugin integrates OdooLS to PyCharm. You can find it here: https://github.com/odoo/odoo-pycharm

### Vim extension

An integration of OdooLS is available for neovim. Check it out here: https://github.com/odoo/odoo-neovim

### Zed extension

A light integration of OdooLS is available for Zed. Check it out here: https://github.com/odoo/odoo-zed

## State of the project

We release this project in two versions: release and pre-release (also called beta).
An even minor version number indicates a release version, while an odd minor version number indicates a pre-release version. For example, 1.4.x is a release version, while 1.5.x is the next pre-release version that will lead to the future 1.6.0 release.
Pre-release versions are more likely to crash or include incomplete or inconsistent features, but they give you early access to upcoming features.
You can switch between both in your IDE, as both versions are published.
In VS Code, you can switch using the corresponding button in the Extensions tab (`Switch to Release` or `Switch to Pre-Release`).
In PyCharm, you have to subscribe to the Beta extension channel.

## Wiki

If you need help to build/install/setup OdooLS, do not forget to check our wiki: https://github.com/odoo/odoo-ls/wiki
If you can't find your answer there, feel free to open an issue or a discussion !

## Branches description

`master` contains all new merged content
`alpha` contains all features that are freezed for the next beta version and tested internally
`beta` contains the latest pre-released public version (downloadable packages, available on marketplace that supports pre-release tags)
`release` contains the latest released public version (downloadable packages, available on marketplace)

## Contributing

Do not hesitate to create [issues](https://github.com/odoo/odoo-ls/issues) or to open a [discussion](https://github.com/odoo/odoo-ls/discussions) would you have any problem or remark about the projects. Do not hesitate to browse the [wiki](https://github.com/odoo/odoo-ls/wiki) too.

## License

All the projects of this repository is licensed under the LGPLv3 license. You can consult the LICENSE file to get more information about it.
