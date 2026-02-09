# Changelog

## [1.2.0] - 2026/02/09 - Workspace Symbols / WSL support

*This update pushes the pre-release version 1.1 to the stable branch. There is nothing new if you already uses the pre-release version*

This update improves the QoL on various IDEs and brings some new features:
- Workspace Symbol Lookup allows you to search for any class/function/model/xml_id in the whole project (ctrl-t on vscode). All xml_id are prefixed
by `xmlid.` and model names are quoted.

![workspace symbol lookup image](https://github.com/odoo/odoo-ls/blob/images-changelogs/images/workspace_symbol_lookup.png?raw=true)

- Import statements now have autocompletion, hover and gotodefinition features.
- We now have a better support for WSL paths!

### VsCode

- Better display about the server status.

### PyCharm

- Improve the lifecycle of OdooLS. Server will be always running but idle, and starts only if a configuration is detected. It implies
that we removed the 'start server' button as it is not useful anymore. It should end up in a more clear and usable interface.
- Fix the "Disabled" profile behavior that was preventing any further profile change.
- Starting from 2025.3, PyCharm will be able to display the loading status of the server.
- Attach additional stubs (with lxml) to the build.
- Add configuration wiki link on the settings page.
- Update deprecated API calls to isAarch64 methods.

### Server

- Support for workspace Symbol requests.
- Core structure to support `$/cancelRequest` notification and use it for Workspace Symbol Request. These notifications indicates that a job can be cancelled because no more useful.
- Import statements now have autocompletion, hover feature and gotodefinition.
- Server now supports WSL paths, including `file:////` or `file://wsl.localhost`
- Server now use workDoneProgress to report loading status to client. Client that supports this feature will now display the loading progression at startup.
- Server will now send more information about its status: it can indicate if it is waiting for a git lock to be freed.
- Autocompletion and validation for inverse_name keyword argument.
- Crash reports will now include the latest LSP messages to help the debugging and give us a better overview of what happened before the crash
- It is now possible to autocomplete slices ( `self.env["`) even without closing the brackets.
- Diagnostic filters in configuration files can now accept variables like `${userHome}` or `${workspaceFolder}`
- You can now hover and gotodefinition for module names in `__manifest__.py` files, and in hover you could alse see full list of the module's dependencies.

![manifest hover module image](https://github.com/odoo/odoo-ls/blob/images-changelogs/images/manifest_hover_modules.png?raw=true)

- `filtered` and `filtered_domain` now has a proper return value.
- Remove diagnostics of ImportError in the `except` block of a `try..except ImportError` statement.
- Doing a gotodefinition on a `display_name` will now redirect you to the compute method.
- Update Ruff dependencies to 0.14.3.
- Support for encoding UTF-8, UTF-16 and UTF-32.
- Support for "untitled" files for VsCode.
- Add tests for diagnostics
- Various fixes
- Fix and support for CachedModel introduced in 19.1
- Use a deterministic job queue to avoid random errors caused by different order of symbols
    - For that we replace the current HashSet with a FIFO one, so symbols are processed in the queue order

### Fixes

- Doing a gotodefinition on a value (like a string `"a string"`) will not lead to the value definition (`class str`)
- Doing a cyclic dependency between 'modules depends' will not crash anymore but generate the diagnostic OLS04012.
- Fix the path to additional stubs and so fix the usage of the lxml stubs.
- If odoo_path is ending with `.something` but is pointing to a valid directory, the server should not consider `something` as a file extension, but as a part of the folder name.
- Fix some internal hooks to work with user defined Fields, instead of only the Odoo ones.
- Configuration option "diag_missing_imports" is now really taken into account when generating diagnostics about imports.
- Fix dependencies on comodel and relation fields diagnostics.
- Fix missing ImportError diagnostic on import statement without a 'from' or 'as' part.
- Various small fixes and typos
- Fix crash when a file is importing a .pyd with the same name (avoid self referencing)
- Fix OLS01002 not emitted on valued variables
- FIX OLS01004 that should not be emitted on `classmethod`
- FIX OLS01007 and OLS01010 on evaluation of function calls when keyword-only arguments are used.
- XML Syntax error is now OLS05000
- Fix range for diagnostic OLS05009
- Fix OLS01009 that could be emitted on valid cases.
- Fix detection of `search` and `inverse` keyword on fields declaration
- Fix detection of `inverse_name` on One2Many if the keyword was missing
- Fix deprecation warning OLS03301 that was not emitted
- Fix crash on data not being string in `__manifest__.py`
- Fix validation of `__manifest__.py` files even if the folder does not contain any `__init__.py`
- Functions will not expose their internal function in an autocompletion anymore
- Fix wiki link for configuration on welcome page
- Avoid having empty paths for addons or additional stubs in cli mode
- Avoid adding model dependencies in orm files to avoid rebuilding base files
- Avoid loading Models defined inside functions, e.g. tests.
- Avoid attempting to rebuild `__iter__` on external files, as their file infos are deleted
- Fix fetching symbols in inheritance tree by early stopping when one is found
- Fix failure to detect models when `CachedModel` is missing
- Fix crash on SQL datafiles
- Fix crash on missing python command
- Fix crash on model classes outside a module
- Load modules in the same order Odoo load them
- Ignore invalid `Named` expression on incomplete AST
- Handle models subscripts like `self.search([])[:5]`
- Add missing Self evaluation to `search` method on BaseModel
- Methods `__init_subclass__` and `__class_getitem__` are now automatically detected as `classmethod`s
- Allow `M2OReference` on `inverse_name`
- Fix index calculation in the arguments of functions
- Fix crash on empty config received from client
- Improve borrowing to avoid some borrow errors
- `next_ref` will now rebuild descriptor on-the-fly if needed
- Fix `follow_ref` sometimes dropping evaluations incorrectly
- Remove wrong stop_on_type in features
- Check all relational fields during domain validation
- Correctly identify non-registry model classes
- Update typeshed
- Improve the `env.__getitem__` to handle multiple evaluation
- Improve the `follow_ref` method to handle `typing.Self` evaluations