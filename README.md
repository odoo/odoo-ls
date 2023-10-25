# Odoo Language Server

This repository contains a language server for the Odoo framework that will provide autocompletion, file validation, hover requests, go to definition, and more. This language server is made available for your favorite IDE with the different extensions of this repository.
Actually only vscode is available, but others will come later.
To learn more about language servers, read https://microsoft.github.io/language-server-protocol/
Please consult the readme of each directory to learn more about each project.

## Table of Contents

- [List of projects](#list-of-projects)
- [State of the project](#state-of-the-project)
- [Contributing](#contributing)
- [License](#license)

## List of projects

### Language Server

A generic language server that can be used to provide common IDE features to your IDE: autocompletion, Hovering, go to definition, etc...

### VsCode Extension

An extension that will bundle the Odoo Language Server and give needed settings and some UI improvements to your vscode.

## State of the project

All modules in this repository are actually in development and not released in a stable and valid version. You can face crashs or inconsistent results by using it. Please consult each directory to get a better idea of the state of each project.

## Branches description

`master` contains all new merged content
`alpha` contains all features that are freezed for the next beta version and tested internally
`beta` contains the latest pre-released public version (downloadable packages, available on marketplace that supports pre-release tags)
`release` contains the latest released public version (downloadable packages, available on marketplace)

## Contributing

Do not hesitate to create [issues](https://github.com/odoo/odoo-ls/issues) or to open a [discussion](https://github.com/odoo/odoo-ls/discussions) would you have any problem or remark about the projects. Do not hesitate to browse the [wiki](https://github.com/odoo/odoo-ls/wiki) too.

## License

All the projects of this repository is licensed under the LGPLv3 license. You can consult the LICENSE file to get more information about it.
