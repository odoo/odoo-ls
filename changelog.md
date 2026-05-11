# Changelog

## [1.3.2] - 2026/05/11 - release candidate

### Server

- When working with command line (`--parse` option), the parameter `tracked-folders` is now mandatory, as not providing it would lead to unclear results
- dynamic variables (`${workspaceFolder}`, ...) can now be used in the `additional_stubs` and `stdlib` option in toml config files.
- Add OLS OLS01011 indicating that a positional-only argument is passed with keyword arguments
- Add the OLS05069 and OLS05070 diagnostics, related to CSV records parsing.
- Add the operator "access" to domain operators, added in Odoo 19.3

### VsCode

- Fix a crash by preventing the plugin to try to start OdooLS if "disabled" profile is selected
- Update the welcome page links to the wiki

### Fix

- Fix wrong diagnostic OLS01010 indicating that keyword arguments are missing, if `**kwargs` is provided in the call
- Correctly handle files and modules that are opened but for which the creation has not been detected (because of moving from outside the workspace for example)
- Improve the conversion of uri of workspace folders (more specifically tracked-folders)
- Fix the parsing of csv records, especially when quotes are present
- Avoid raising errors during shutdown. This should remove crash notification that can happen when the server is reloading, usually during git branch switches.
- Fix the trimming of .py and .pyi for the paths from config files. Only file extension is now stripped, not any occurence of .py(i)
- Fix GoTo features when used on a xml record stored in a python file (model_xxx ones for example)
- Fix borrow error in some evaluation of `self.env.ref` function
- Fix crash on imports in custom entrypoints with relative folders. A proper implementation will come later.