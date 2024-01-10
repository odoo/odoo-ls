# Changelog

## 0.2.4 - 2023/01/10

### Fixs

- Fix crash on get_loaded_part_tree if addon path has not been found
- Fix crash on autocompletion if opened file is not found (out of workspace for example)
- Allow path to Odoo community to end with a /
- Fix crash when hovering Relational field declaration
- Fix crash when creating a symbol that was previously missing
- Fix infinite log generation on BrokenPipeError

## 0.2.3 - 2023/12/19

Last update of 2023 ! We wish you all an happy new year !

### VsCode

The rework of the client to work with the Python Extension is delayed to 0.2.4

- New option to choose which missing import should be diagnosed. 3 options are available: none, only odoo imports, all

### Server

- Support a new configuration option to choose which missing import should be diagnosed. The option is called "diagMissingImportLevel" and can take 3 values: "all", "only_odoo" or "none".
- The server can now identify a 'type alias' as it should be. It should now be correctly displayed where it is relevant. Best example of type alias is "AbstractModel", that is a type alias of "BaseModel".
- Server is now able to override `__get__` functions behaviour in its core, and the odoo implementation define all return values for all fields. It means that from now:
`self.name` will be displayed as an str, but `MyModel.name` will be displayed as a fields.Char.
This value is used to for the autocompletion, and so you won't have suggestions from fields class after using a field in a function (like `self.name.???`)

### Fixs

- Prevent BrokenPipeError to log indefinitely if the server is disconnected from client. This fix improve the one of the last version to handle last (hopefully) not catched situation.
- Update int to proper enums in module.py for the 'severity' option of diagnostics


## 0.2.2 - 2023/11/20

### VsCode

- Fix broken image links in readme files.

### Server
- Fix typo in the last patch
- update code to work with cattrs==23.2.1
- Fix diagnostic crash in non-module addon

## 0.2.1 - 2023/11/15

This version contains various fixs based on the reports we got. No new features here.

### Server
- Fix log file hell. No more log file that will fill up your hard disk.
- Fix a crash that can occur if a model is declared ouside of a module (really?)
- Fix crash that can occur if the configuration is wrong. Handle it properly
- Allow creation of full path instead of only a new file. if you have a directory test, you can create test/dummy/file.py in on command instead of 2 (directory + file) without having the extension crashing
- Fix crash on some file edit due to the thread queue that was missing some context
- Fix character index on Hover and Definition feature that created a crash if you hover the last character of the file

## 0.2.0 - 2023/11/07

Update to version numbers: "0.x.0" if x is even, it will be a beta (or pre-release) version, odd numbers will be release version.
There will be no more tag to version (-beta or -alpha)

### VsCode
#### Configurations
- Add a changelog page
- New log level option.
  - Remove the old debug log level by default as it can generate a lot of logs.
- Separate log for each vscode instance and implement a cleanup of old files
- Add a warning if detected odoo in the workspace that's different than the one in the selected configuration
- Add a warning if detected addons paths in the workspace that were not added in the configuration
- New readme and repository refactoring

### Server

- An inheritance that is not in the dependecies of the module will now raised a "not in dependency" error instead of a "not found" one.
- Odoo 16.3+ : raise a DeprecationWarning on all "odoo.tests.common.Form" imports and manually resolve the new symbol

### Fixs

- Autocompletion can no longer propose parameters as items
- Support venv on windows
- Fix odoo version detection for intermediate version (saas)
- Module detection behaviour: empty directory canno't be selected anymore as valid module while searching for a dependency, and non-module files are not added to the tree if not needed.
- New symbols are not built anymore if nothing is importing them
- Python 3.10+: IndentationError is using -1 character index (despite the documentation indicating [1-MAX_INT]), so a special case has been added to handle it.
- The server will not try to release a non-acquired lock anymore

## 0.1.1-alpha.1 - 2023/09/27

### Fixs
- Language server: Various fixs on asynchronous jobs:
  - prevent EventQueue to process event without valid lock (startup crash)
  - prevent EventQueue to hold the access during update
  - A acquired lock on a queue can not be on a Null queue anymore

## 0.1.1-alpha.0 - 2023/09/23

### VSCode
#### Configurations
- Move the Python version from the settings to the configurations
- Supports virtualenv in pythonPath (venv)
- Add a 'save' and 'delete' button and remove the autosave behaviour
- Add a patchnote page
- Add an option to choose when the server should reload: OnSave, onUpdate, off, as well as the possibility to choose the delay.

### Language Server
#### Core
- Now able to react to external updates for files in workspace, like git updates, or changes done with another tool than current client
- Rewrite job queue to be less aggressive on thread creation
- ~10 % less memory usage by optimizing the cache
- ~10% speedup by optimizing the cache
- ~20% more speedup on Linux (and any non-case-preserving filesystem)
- Refactor the `__manifest__.py` parser to be able to:
  - Raise diagnostics for invalid file (load with ast instead of literal_eval)
  - Update the internal representation on manual changes and trigger dependency updates
- Refactor the listeners to missing symbols, reducing the number of needed rebuild on symbol resolution.
- Make the logger adapted for multi visual studio instances

#### Autocompletion
- Improve item order
  - First will come all public items then private one (with "_" or "__")
  - In each section, items are sorted by the inheritance level: current class first, then inherited classes.
- Improve display - Add the original model of the item on the right aside the type
- Add symbol documentation
- Speedup model autocompletion (by 50x)

#### Hover
- Improve information displayed (more precise class identification)

#### Fixs
- Prevent any completion/hover feature on non-py file


## 0.1.0-alpha.3 - 2023/08/28
Last update to the 0.1.0 that is discontinued now and won't go to beta phase. We will wait at least for the 0.1.1 before testing a beta version (for the venv support)

### VSCode
#### Welcome page
- Remove coma in pip command
#### Crash report
- Add python version and configuration setup to crash report

### Language Server
#### Core
- Improve SyntaxError reporting and make code more robust to invalid files (python 3.10+)


## 0.1.0-alpha.2 - 2023/08/22

### Language Server
#### Fixs
- Fix crash on missing index on remove_symbol
- Fix crash on external dependencies evaluation on references


## 0.1.0-alpha.1 - 2023/08/21

### VSCode
- Prevent the crash reporter to crash if the opened file is not a real file (like config page)
- Prevent the language server to crash when evaluating a non-workspace file, but log it instead (help debugging the configurations)


## 0.1.0-alpha.0 - 2023/08/13

### VSCode
- Configuration page, that allow editing configurations
- Configuration button on the status bar, that will:
  - Display the current availability of the extension with a loading icon (present or not)
  - Display the current chosen configuration
  - open a configuration selector on click
- For Beta/Alpha version, an upload mechanism for crash reports.

### Language Server
#### Core
- Initial loading of python files of an Odoo project, base on given configuration
- Dependencies graph and cache invalidation to be able to keep the internal representation up-to-date with user changes.
  - Note: this is and will be the main focus for the first versions, as we want first to keep enough stability to make the extension useable through a day of development, before adding new features.

#### Autocompletion
- Class attributes
- Model attributes
- _inherit model name

#### Hover
- Display name of symbol under cursor
- Try to guess type
- show description of symbol if exists
- Add a "useful links" section in the description if some interesting symbols could be displayed (like models, or evaluated class)

#### Go To definition
 - Using the same algorithm than the Hover feature, redirect to the symbol requested.

#### Validation
Note: The validation is a big part of the project that could raise a lot of interesting diagnostics from a static analysis of the code. As soon as the stability of the extension will be robust enough, we will add rules here. If you have ideas of things we should check, do not hesitate to contact us.
- Test if an import is valid for the dependency graph of the module. Doesn't raise an error if the import is surrounded by a try...except
- Test that the base class of a class is a valid class symbol
- raise a warning for any not found import.
