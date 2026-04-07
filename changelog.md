# Changelog

## [1.3.1] - 2026/04/07 - Deadlock fix

### Fix

- Fixed a deadlock that could occur during startup.

## [1.3.0] - 2026/04/07 - Go to References

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