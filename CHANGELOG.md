# Changelog

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
