# Changelog

## 0.10.0 - 2025/08/08 - Better configuration and XML features

Following your feedbacks, we are adding some new features and ways to configure your odools.toml files. If you still think that some changes would be interesting, do not hesitate to contact us !
Alongside these changes, we added some features to XML files, like gotodefinition, hover, and new diagnostics. Everything we want is not there yet, but it's coming soon!
We focused a lot too on improving the exactness of various diagnostics, so you should see less errors in your project!

### VsCode

- Add wiki link to configuration view.
- Update Disabled profile: "Disabled" now shuts down the server
- New setting to provide a generic odools.toml that would be applied to ALL profiles, and that can be at a specific location


### Server

- Add diagnostics filters in configuration files. It allows you to choose diagnostics you want to hide and their severity level
- Add new config url parameter to give a common config file for all your profiles
- Rewrite diagnostics modules, to allow filtering and changes of the level.
- Fix new convention for OLS codes
- Improve the server restart behaviour
- Parse and use `delegate=True` to detect _inherits models
- Improve build tools
- Fix imports with asname value
- gotodefinition on ref to xml_ids
- New hook to global field on IrRule
- New hooks for werkzeug _monkeypatches
- Disable call argument checks for properties function for now.
- Detect and mark function with @classproperty or @lazy_classproperty as such
- Fix all the arguments validation process

#### XML

- menuitem: check that parent is valid
- menuitem: chack validity of action attribute
- menuitem: validate groups attribute
- record: validate model
- record: check that all mandatory fields has been provided
- record, fields, menuitem: hover and gotodefinition to xml_ids and models
- field: basic validation and for specific models (ir.ui.view)
- Support for @language in XML Fields

### Fixes

- Fixed version comparator that was not equaling "18.0" and "18.0.0". It had various side effects on some specific version features.
- Fix crash on delayed thread that occur if odoo is made invalid in the delay
- Fix creation of custom entrypoints on hovering some xml/csv files
- Fix crash on reloading functions created by hooks (only in orm files)
- Various things that would be too hard to explain here. You really read all the changelog?

## 0.8.1 - 2025/09/07 - Quick fix

### Fix

- Fix an issue that prevent OdooLS to correctly detect Odoo Models on some version.

## 0.8.0 - 2025/04/07 - Configuration files and XML Support (part 1)

This update introduces two new big changes in OdooLS.

First, we updated the way OdooLS is configured for a more porwerful, flexible, and IDE-independant way. Unfortunately, it implies that all your existing configurations are lost with this update. We are sorry for that, but we hope you will love way more the new system when you'll have adopted it ! Do not hesitate to give us your feedback, questions or anything you want to say about it on our [github](https://github.com/odoo/odoo-ls/discussions). As it is quite different, you can get lost at the first time. Do not hesitate to read our wiki about [configuration files](https://github.com/odoo/odoo-ls/wiki/Configuration-files).

Secondly, the update introduces all the basic parsing for XML files. This part 1 only includes XML loading, parsing and validation against Odoo RNG file. There is features about XML files here (no hover, gotodefinition, etc...) as it will be released in part 2. This update focus on including XML and CSV files in the server cache and ensure that everything is running fine now that we have different file extensions and language. As always, we would be really happy if you can send us any error or issue you encounter with these new features.

Let's go with this update more in detail now!

### Server

- New configuration system. Configurations are not stored in settings.json anymore, but in configuration files on disk.
- OdooLS can now detect Odoo and run even without any configuration file.
- When loading a manifest, search and load data files (csv/xml). If not found, raise a diagnostic.
- When loading an XML file, display diagnostic on syntax errors
- Validate XML files against RelaxNG file. This validation is not using the file actively but is hardcoded in the server. We did this as
there is no real implementation of RelaxNG in Rust and that we included a lot of hooks and additional checks on the validation based on the python code of Odoo too (more will come in part 2).
- New structure to support dynamic member variable declaration, like in
```python
class cl:
    pass
cl.new_variable = 5
```
This is actually mainly used for new Odoo changes on master, and is not dynamically supported in custom code.
- Support for invalid AST. Now an invalid AST should not prevent the server from providing features as definition, hover, etc...
- Allow hooks to applied on some version of odoo only. Fix most of the issues related to hooks for branches > 18.1 of Odoo.
- Various small optimization updates.

### VsCode

- Update the interface to not show configurations but give a way to change active profile.
- Provide Semantic token for CSV file. If you don't have the RainbowCsv extension installed, OdooLS will colorize your csv files for you.

### Fixs

- Fix import of multi-level elements, as in `import a.b.c`
- Allow name completion in some nested expressions.
- Autocompletion is now better localized, and can not suggest variables declared later in a bloc.
- Fix various borrow errors.

## 0.6.3 - 2025/23/05 - Bugfixs

### Fixs

- Log instead of crash if a file is not in cache anymore as it can happen in some normal situations where cache is invalidated
- Fix various borrow errors on 'go to definition' feature
- Fix various crashes when hovering some part of the code
- Provide PDB alongside exe on windows to get better tracebacks.

## 0.6.2 - 2025/28/04 - Bugfixs

### Fixs

- Fix crash on empty odoo path
- Fix crash while autocomplete on an empty file
- Fix crash on autocompletion in some wase where the needed files are not built already
- Fix crash on module search that could return namespaces instead of modules
- Fix diagnostics range in manifest depends
- Add missing error code in error_code.md
- Fix crash on lazy loading invalid `__all__` variable
- Clean some logs

## 0.6.1 - 2025/24/04 - File cache option and bugfixs

### Key features

- New option that will move file cache from RAM to disk (reduce memory usage by ~30%, but will increase disk access). It is off by
default to not stress your disk, but you can activate it in your settings if you are on a computer with limited RAM amount.
- This patch focus on bugs you reported from the 0.6.0. Thank you for sending us your crash report, it helps us building a stable tool !

### Server

- New file cache option to move file cache from RAM to disk. Off by default

### Fixs

- Fix an issue that was removing all diagnostics on settings change for no reason
- Fix a crash on evaluation refresh
- Fix transformation of path from addon entry to main entry
- Fix creation of entryPoint for namespace symbols
- Fix an issue that prevented the server to mark files as closed, creating a crash on reopening.
- Fix parsing of `__manifest__.py` and `__init__.py` in custom entry point. It fixes crash when opening files outside of the config
- Fix crash on parsing empty `__manifest__.py` file
- Fix reloading of custom entry point
- Fix path comparison. "account_account" will not be considered below "account" because it shares the same start. Now path components are properly used
for the comparison. 

## 0.6.0 - 2025/15/04 - Entrypoints and NOQA Update

### Key features

- OdooLS is now able to run on any python file, even if this python file is not part of your odoo setup. It will then run
like a classical LSP and provide you hover, autocompletion, gotodefinition, etc... but without any odoo knowledge.
- As the core structure should now be in its nearly final form and because of the previous point, OdooLS now has a base test suite to ensure we are keeping every feature stable patch after patch ! These will grow in the future.
- Support for #noqa directive, on file, classes, function or line, with or without error codes
- OdooLS can now handle Walrus operator ⊹╰(⌣ʟ⌣)╯⊹
- OdooLS now has improved inferencer engine and can parse way more expressions and statements
- Various cache and algorithm improvements speed up the server by ~30%, but these ~30% are lost with new features and required parsing...
- OdooLs is now 50% faster on Windows due to disk access improvements. It is nearly not impacting Linux and Macos distribution however.
- Memory usage has been improved by ~6%
- In the end, building time is 10% slower due to new features
- Handle {workspaceFolder:directory} variable in path configurations.

### Server

- Introduce EntryPoints. OdooLS will now provide features for a file depending on its entrypoint: It can be the main entryPoints (usually the odoo project, with odoo/__main__.py), or a single-file entrypoint, the current file. Depending on this context, the server
can act diffently and then work on any python file, even out of the odoo structure. Temporary files are not yet handled however, we
still rely on the disk path to identify a file (will change in next updates)
- Improve Evaluations by handling following Expressions and Statements:
  - Number literals: Float and Complex
  - If blocks
  - unary operators
  - constants (ellipsis and None)
  - basic FString
  - typing.Self
- Make results unique in model name hover
- Add a cache to import resolver, speeding up the process.
- Add traceback to error info in crash report
- Use Yarn instead of String to store small names of symbols to speed up and improve memory usage
- Add hover and gotodef feature to decorators (@api.depends,...), to related fields, comodel_name and model strings before arguments.
- Update Ruff Parser to 0.11.4
- Improve reactivity of server on typing in 'adaptive' mode
- Support for NOQA
- Odoo step has been merged with Arch Eval step, resulting in a process in 3 steps instead of 4.

### Server Fixs

- A module is now automatically (re)imported if reloaded or created if it is in addons path.
- Fix dependency graph on inheritance and imports.
- Fix TestCursor hook behaviour to show right Cursor class in tests directories
- Fix BorrowError on FileManager clear method
- Hover and GotoDefinition features are now working in .pyi files
- Evaluation should correctly take into account all base classes of an object/model
- Fix this changelog filename to be able to publish on VsCodium
- Fix infinite loop on variable evaluation

### Vscode Fixs

- Prevent throwing an error notification when the client is stopping
- Improve reactivity of the server if an interruption is coming during processing or shutdown event

#### New diagnostics / odoo helpers

- Check that the manifest doesn't contain the same key twice
- In a compute function, check that you don't assign another variable that the one you are computing
- Check that comodel_name on related fields is valid
- Check that related field is the same type
- New errors to express the invalid dot notation in strings used for related, domains...

## 0.4.1 - 2025/12/02

Small patch that address crashes we got from your reports

### Vscode Fixs

- validate paths on config view opening

### Server

- Server will now answer to DocumentSymbol requests and give you a tree os symbols that you can find in a file.
- Add validation and diagnostics on some missing Python statements (match, ...)
- Better error message if missing typeshed

### Server Fixs

- crash fix: Handle new odoo structure available on master
- crash fix: Do not evaluate documents that are not saved on disk and has an invalid path (will be improved later)
- crash fix: Do not assume that base class is always valid, and silently ignore an invalid base class
- crash fix: Do not evaluate architecture of a file if the hash of file has changed since the first building
- crash fix: url encode paths to handle spaces or invalid characters in uris
- crash fix: add a guard against empty contexts while getting symbol
- crash fix: Fix crash on cyclic references involving only functions (temp fix before a proper implementation)
- Fix multiple inference instance evaluation
- add context information to be able to resolve ": Self" return value for functions
- Test if odoo package is found or not and log it if not
- Fix hook that transform Cursor into TestCursor in tests directories

## 0.4.0 - 2025/05/02

0.4.0 is the first Rust version of the tool that is coming to Beta. It means that if you didn't update to alpha version manually, this changelog is new for you since the 0.2.4 version (last published Python version of the tool, not maintained anymore)
Some configuration migrations could fail while upgrading from the Python version. We apologize in advance if you have to set up them again !

### VsCode

- Add a commmand to restart the server manually
- Updated welcome page to reflect new changes
- Remove deprecated views
- Remove "afterDelay" option, in favor of "adaptive" option. Threads are way more reactives than before and "adaptive" should
be enough in all cases.
- handle installation of python extension while Odoo is running

### VsCode Fix

- Fix the extension hanging while the server starts

### Server

- improve odoo detection to handle nightly builds of Odoo.
- Return Class location on definition request of model name (strings)
- Server will auto reset if too many changes occur in the workspace (git checkout detection purpose)
- improve the rebuild queue, by putting functions in it with a module dependency, instead of the whole file. It lowers the needed
computation on each change.
- onSave settings will not trigger a rebuild anymore if ast in the file is invalid
- Improve range of link given by GoToDefinition on packages
- precompute model dependencies to improve performances
- Add various odoo api method signatures (with_context...)
- Add search domains diagnostics
- Add search domains autocompletion
- Add search domains GotoDefinition
- Various hover display improvements: syntax, values and infered types on functions
- Implement _inherits logic
- Improve internal context usage to correctly reflect what contains the current parsing
- Remove usage of the custom route Odoo/getPythonPath, and now using lsp default configuration
- Improve message managements to make threads more reactive, and so the extension
- use start of expr range to avoid some out of scope issues in autocompletion
- Server do not restart anymore but reset on python path update

### Server fixs

- fix crash on importation of compiled files
- Remove autocompletion items that are not in module dependencies

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
