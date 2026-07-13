# Changelog

## [1.4.0] - 2026/07/22 - Go to References

This update is refactoring the way "Goto" features are working, as well as adding the new Go to References feature.

### Server

- Change gotoDefinition to pass through imports until the true definition of the symbol
- Add GoToDeclaration that goes to the first declaration or assignation found for a symbol
- Add GoToReferences that will search for all usage of a symbol. Available in python, xml, csv and `__manifest__.py` files.
- Implement all these gotos features in CSV files.
- Load and validate `asset` nodes in XML files.
- Validation of language codes used in XML files.
- Add a new option in configuration files: "additional_languages", allowing you to add languages that would not be added in data files.
- Server will not close anymore if multiple workspace folders has the same name. However, it will still be impossible to reference one of them in a configuration.
- Handle lambda expressions.
- Add evaluation for `request` and `request.env` in controllers.
- Validation of `assets` values in `__manifest__.py`
- Improve the `self` evaluation to be able to propagate it to class children or overrides.
- Add the list of folders to the documentation when hovering a symbol representing a python namespace
- CLI mode is now loading configurations like the normal process is doing, making the profiles available in this mode.
- Add a new argument to the command line: `selected_config` allowing you to manually select a profile when running in CLI mode.
- Various performances update (HashMap without hashing function for integer keys, better filesystem access on windows)
- Upgrade Rust to 1.94, Ruff to 0.15.0
- When working with command line (`--parse` option), the parameter `tracked-folders` is now mandatory, as not providing it would lead to unclear results
- dynamic variables (`${workspaceFolder}`, ...) can now be used in the `additional_stubs` and `stdlib` option in toml config files.
- Add OLS OLS01011 indicating that a positional-only argument is passed with keyword arguments
- Add the OLS05069 and OLS05070 diagnostics, related to CSV records parsing.
- Add the operator "access" to domain operators, added in Odoo 19.3

### VsCode

- Fix a crash by preventing the plugin to try to start OdooLS if "disabled" profile is selected
- Update the welcome page links to the wiki
- [packaging] Improve package dependencies management
- [packaging] remove axios and untildify dependency and update other packages

### Fixes

- Fixed a deadlock that could occur during startup.
- Fix wrong diagnostic OLS01010 indicating that keyword arguments are missing, if `**kwargs` is provided in the call
- Correctly handle files and modules that are opened but for which the creation has not been detected (because of moving from outside the workspace for example)
- Improve the conversion of uri of workspace folders (more specifically tracked-folders)
- Fix the parsing of csv records, especially when quotes are present
- Avoid raising errors during shutdown. This should remove crash notification that can happen when the server is reloading, usually during git branch switches.
- Fix the trimming of .py and .pyi for the paths from config files. Only file extension is now stripped, not any occurence of .py(i)
- Fix GoTo features when used on a xml record stored in a python file (model_xxx ones for example)
- Fix borrow error in some evaluation of `self.env.ref` function
- Fix crash on imports in custom entrypoints with relative folders. A proper implementation will come later.
- Fix wrong diagnostics about positional-only arguments to functions
- Fix crash with cache poisoning when file is edited during validation
- Fix duplication of file cache update
- Fix some relative import errors in no-odoo mode

### Fixes not included in previous 1.3.3

- Fix crash on some cyclic rebuild jobs
- Fix progression indicator never ending on PyCharm
- OLS03023 was wrongly raised on fields that inherit an abstract model