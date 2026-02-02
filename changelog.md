# Changelog

## [1.1.3] - 2026/02/02 - Various fixes

### Fixes

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

## [1.1.2] - 2025/12/10 - CachedModel fixes

### Server

- Various fixes
- Fix and support for CachedModel introduced in 19.1
- Use a deterministic job queue to avoid random errors caused by different order of symbols
    - For that we replace the current HashSet with a FIFO one, so symbols are processed in the queue order

### Fixes

- Fix wiki link for configuration on welcome page
- Avoid having empty paths for addons or additional stubs in cli mode
- Avoid adding model dependencies in orm files to avoid rebuilding base files
- Avoid loading Models defined inside functions, e.g. tests.
- Avoid attempting to rebuild `__iter__` on external files, as their file infos are deleted
- Fix fetching symbols in inheritance tree by early stopping when one is found


## [1.1.1] - 2025/11/24 - Untitled files and Encoding

### Server

- Support for encoding UTF-8, UTF-16 and UTF-32.
- Support for "untitled" files for VsCode.
- Add tests for diagnostics

### Fixs

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


## [1.1.0] - 2025/11/06 - Workspace Symbols / WSL support

This Beta update improves the QoL on various IDEs and brings some new features:
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


## [1.0.4] - 2025/10/31 - Fix Borrow error

### Zed

- Fix the update script to use ".tar.gz" compression instead of ".zip" on linux and darwin computers.

### Fixes

- crash fix: Borrow error on some "on-the-fly" builds.
- Fix the rust version used to compile the server to 1.91 and put this requirement in cargo.toml

## [1.0.3] - 2025/10/26 - Fix out-of-sync issues

We rewrote the thread pool of OdooLS to get rid most of the last known crashes, as they are nearly all linked to out-of-sync issues and the way the thread pool was greedily delaying important tasks.
It should result in a different feeling when using OdooLS, but it should be way more accurate and stable than before, and consistent.
It was not possible to test this new core in all possible situations, so do not hesitate to give any feedback on differences you can see with this new version.
Thank you very much to everyone for your crash reports, they were really helpful (and yes, we read all of them!)
New features will come soon in the pre-release channel, stay tuned!

### Fixes

- New delayed thread and message flow in threads. Symbol creation (ARCH and ARCH-EVAL steps) are now always created on the fly, while validation of files are delayed to inactivity period. It results in more accurate and always in-sync results to requests.

## [1.0.2] - 2025/10/03 - Hotfixes and github actions

### Packaging

- use Github Actions to build all files: OdooLS binaries, Vsix and Pycharm plugin. Removing the dependency on cross-rs to build executables.

### Fixes

- Fix starting file version number for PyCharm, that is starting at 0, while vscode is starting at 1
- Fix crash of package creation on custom tree
- Fix crash on Odoo > 19 that happen if werkzeug is installed and up-to-date
- Remove panic on missing `__init__.py` file for custom entry creation, happening if user removed/renamed the file during the initialization of the server, and warn it instead


## [1.0.1] - 2025/09/17 - Day 1 fixes

### Fixes

- Crash that can occur when doing a gotodefinition in an XML file
- Fix origin of gotodefinition for some links in XML files.
- Change back `<br/>` line breaks that sometimes break PyCharm to escaped `  \\\n`

## [1.0.0] - 2025/09/16 - Release

This project is far from finished, but it has reached a level of maturity where we’re introducing two update channels: **Release** and **Pre-release** (Beta).

If you want early access to new features and to help us improve the tool, enable **Pre-release** updates in your IDE. The pre-release channel will only include features we consider ready, though crashes may still occur due to the wide variety of code we encounter. This helps us catch common issues before pushing to the stable channel.

So, if you prefer a more stable experience, stick with the **Release** channel !

Here is the changelog since the last Beta version (0.12.1)

### Zed

New plugin for Zed. As Zed API is quite poor, the implementation only stick to a basic language server implementation, and will not provide profile selector, profile viewer or crash report view.

### Server

- Change level of "unable to annotate tuple" log from error to debug, as it is indeed a debug information of non implemented statements

### Fixes

- When file cache is invalidated by an incoherent request, do not panic, but reload the cache
- Fix wrong usage of `<br/>` for PyCharm and NeoVim.
- Fix crashes that can occur on some gotodefinition.
- Prevent creation of duplicated addons entrypoint if the directory of an addon path is in PYTHON_PATH
- Prevent creation of custom entrypoint on renaming of directory
- Handle removal of `__init__.py` from packages
- Avoid checking path from FS on DidOpen notification.
- Fix a typo in the hook of `__iter__` function of BaseModel on versions > 18.1
- Force validation of `__iter__` functions if pending on a `for` evaluation