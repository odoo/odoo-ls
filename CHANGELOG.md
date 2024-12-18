# Changelog

## 0.2.8 - 2024/18/12

### VsCode

- addon paths in configurations can now contains variables: ${workspaceFolder} and ${userHome} are available.
- search for valid addon path in parent folders too.
- New popup windows that will suggest you to disable your actual python language server for your workspace if any is active (only for Python extension).
- Fix hanging if popup window stay opened.
- Fix infinite reload issue

### Server

- Improve autocompletion to take base classes and comodels.
- Add inheritance information in hover for models.
- Adapt the architecture to store function arguments.
- Parse and evaluate function calls according to the function signature. Actually limited to domains and args counts.
- New domain validation: validate structure, operators and fields. Composed fields are not validated for now.
- Autocompletion that contains "." or that complete a string with a "." will not duplicate elements anymore.
- Improve function return type syntax in Hover feature.
- Implement super() evaluation.
- Handle @overload and @classmethod decorator

### Server Fixs

- Autocompletion will not raise an exception if the request is done outside of odoo.
- Gotodefinition will skip evaluation that lead to the same place
- Fix range on GotoDefinition for symbol that has multiple evaluation.
- Prevent parsing docstrings as markdown codeblocks
- Make read thread able to create delayed tasks.
- correctly skip arch step for syntaxically incorrect files.
- Avoid range evaluations on files.
- Allow not imported files to be reloaded
- Remove duplicates in autocompletion results due to diamond inheritance
- Change classes structure to keep inheritance order (HashSet to Vector)
- Incorrect "Base class not found" diagnostic

#### New diagnostics / odoo helpers

- New signature for "browse" on BaseModel.
- New hook for Odoo registry.
- Add "magic" fields to models (id, create_date, etc...)

## 0.2.7 - 2024/31/10

### Server

- Now include macos binary (arm processors)
- Any requests (Hover, autocompletion, ...) is now able to cut any running rebuild, resulting in a way more reactive experience.
- Basic autocompletion implementation. The server should be able to parse ast and understand what you want to autocomplete. However, the results could be incomplete or incorrect at this point, we will improve that in the next versions
- Use hashs to detect if opened files are differents than the disk version to avoid useless computations.
- Prevent file update if the change is leading to syntaxically wrong ast. The index will be rebuilt only if user fix the syntax errors. It avoid useless computations
- Update file cache immediatly, even if reload are delayed by settings. It allows autocompletion to be aware of changes.
- Delay the symbol cleaning to the file reload and not on update, to not drop symbols that could be used by autocompletion or other requests
- Now handle setups where odoo community path or addons path are paths that are in sys.path.
- Fix evaluation of classes having a base class with the same name.
- Fix parsing of empty modules with only a manifest file
- Basic With statement evaluation
- Improve Hover informations for imports expressions (especially for files, packages, namespaces)
- use root_uri as fallback if no workspace_folder is provided (root_uri is deprecated though)
- Implement a profiling setup with iai-callgrind
- various cleaning

#### New diagnostics / odoo helpers

- Add deprecation warning on any use/import of odoo.tests.common.Form after Odoo 17.0
- Autocompletion of Model names in self.env[""] expressions. Autocompleted model names will indicates if a new dependency is required. This comes with a new settings allowing you to choose between 'only available models' or 'all models with an hint'

## 0.2.6 - 2024/01/10

### Server

- Add Function body evaluation. This is the major content of this update. The server has now the required structure to parse function
body and infer the return value of a function. This feature is rudimentary and a lot of function will still have a return value of None,
but the code is ready to support new python expressions!
- fix python path acquisition from vscode settings
- Ignore git file update to avoid useless reload of the index.
- Add various new diagnostics
- fix deadlock that can sometimes occurs in some file update.
- Add support for dynamic symbols. Dynamic symbols are symbols that are added on an object after its declaration
- improve dependency graph to support models
- Server now reacts to WorkspaceDidChangedWatchedFiles, and will restart automatically on Odoo version change
- Better logs for investigations: used settings, build name, etc...


## 0.2.5 - Beta Candidate - 2024/07/10
### Rustpocalypse

This update bring a completely new rewritten version of the extension.
The whole server has been rewritten in Rust for performance reasons, and the vscode extension has been remade to work as a Python Extension.
This version is a first public test, but is not complete yet.
It implies that some features disappeared (temporarily), but there is some new in comparison with the python version.

### Server

You should expect the same features than the python version (diagnostics, hover and gotodefinition), but:
- autocompletion is not available for now, but will come back really soon
- Due the new performances, the extension is now able to parse the content of functions, where Python were only parsing code structure, and is still way faster.
- logs level are editable, and a rotation is set.
- Alongside OnSave and AfterDelay modes to update diagnostics, Adaptive will now refresh the data immediatly or not depending on the size of the task queue.
- New CLI mode, that allow you to generate a JSON with all diagnostics with a given source code

As this is a first version, there is some known issues:
- Windows version is way slower than linux one. This is (probably) due to Windows defender and the way Windows handle small allocations.
- Memory usage is quite the same than the python version, but we should improve that later.

### VsCode

- allowing odooPath to contain ${workspaceFolder} and ${userHome}
- configurations are now editable in settings.json (see https://github.com/odoo/odoo-ls/wiki/Edit-settings.json)
- current configuration is now stored in the workspace settings
- allowing the extension to work with the python VS code extension or in standalone mode
- heavy refactoring, improved stability

### Fixs
- Odoo configuration selected does not match the odoo path popup fixed

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
