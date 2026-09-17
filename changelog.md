# Changelog

## [1.6.0] - 2026/09/22 - JavaScript and performance

### Server

- Use OXC and tsserver to provide features for JavaScript files, as well as OWL templates.
- Provide semantic tokens for Python, XML and JS files.
- Store all symbols in an arena, and stop using `Rc<RefCell<>>` to store references.
    - Add GDB scripts to help debug this new memory management
- Switch the internal hasher from SipHash to FxHash for performance.
- Set `codegen-units` to `1` in release builds to improve runtime performance, at the cost of slower compilation.
- On Linux/macOS, switch the memory allocator to jemalloc.
- Support for the new `access` operator in search domains for Odoo >= 19.4
- Refactor the entire config parser to make adding new options easier in the future.
- Report loading progress as a percentage instead of number of items during initial loading.
- Lots of various optimizations.
- Improve unpacking evaluations, resolving code like `for a, b in [(1, 2), (2, 3)]`. `a` and `b` will now properly be evaluated as `int`
- Remove logs that were created on each string occurence of opened files.
- Configuration files that now contains invalid key or syntax errors will be reported in VsCode.
- Update gungraun to 0.19.4
- Various code style enhancements
- Handle special characters ' ', '"', '#', '%', '<', '>', '?', '[', ']', '^', '`', '{', '}', '|', '\\\\' in file URLs
- Semantic tokens can now be disabled in configuration with `disable_semantic_tokens_X = true`, with X in `[python, javascript, xml]`
- Fix and bring back OLS05001 in Xml fields references - `Unknown XML ID`
- Small refactoring of BuildSteps and SymbolKey replacement to improve stability by removing possible invalid states
- Windows builds now include line tables for better tracebacks in case of crash
- Add a warning diagnostic for calls to attributes that are not in dependencies
- GoTo features now select only the headers of classes and functions range. It allows us to bring back the default feature that call gotoreference instead of gotodefinition when clicking on the definition of the symbol
- Add features for `depends` field keyword argument to work in the same way as that for `api.depends` decorator arguments.
- Improve some feature preformances by not cloning the whole AST on requests, by introducing a lightweight enum `ASTKind`.
- More robust and extensive tests for JavaScript features and core.
- Correctly handle Javascript modules (correctly handle `@odoo-module` headers).
- Properly support assets at `.../static/tests` and `.../static/lib`.
- Implement completions for Javascript files.
- Properly store JS Symbols under a module's static dir as assets, and as children of the `ModuleSymbol` they are under.
- Improve and add new diagnostics for Javscript
- Add a new `ComponentManager` to manage the javascript components in a safe localized way
- Add features for newly added `compute_sql` field keyword argument to work like `compute` for odoo versions over `19.1` 


### VsCode

- Display configuration errors to draw user attention to invalid settings.
- Add a popup that strongly suggests disabling the built-in JS plugin in VS Code for your workspace. It will avoid having 2 instances of tsserver running serving the same answers to your requests.
- Notification that suggests disabling the built-in JavaScript plugin will now only be displayed if tsserver is configured and activated

### Fixes

- The server could sometimes get stuck in a state that consumed 100% CPU until the next request (typing, hover, etc.).
- Remove duplicate references found on the same line in XML files.
- Fix some missing references in the GoToReferences features (all references found in functions)
- Fix crash that occur on files that does not contain valid UTF-8
- Fix crash on invalid cycling evaluations.
- Fix Goto location when going on a python package
- Fix default config selection on non-odoo related workspace
- Fix Javascript internal dependency if the file is not part of the project
- Fix issues with compiled Python files.
- Warning about missing tsserver now suggests typescript@6 instead of typescript
- Fix diagnostic OLS05071 that was activating on t-name that contains a dot, which is valid
- Fix various hooks in ORM that could stop working after edits
- Fix cycle prevention in reference evaluation that was incorrectly preventing some valid results due to a different context.
- Fix crash on JavaScript file validation that can happen if AST is not ready or has been dropped
- Ensure that diagnostics are properly cleared when rebuilding an AST
- Fix useless custom entrypoint creation on files that were not opened by the user but modified on disk
- Fix internal file versioning for file info updates of opened files
- Fix range of hook on IrRule to apply only to Odoo < 19.4
- Fix panic on invalid syntax with missing type annotation in tuple assignment, like `a, b: int`
- Fix panic when calling super with an invalid argument like None

### Fixes not included in previous 1.5.2

- Fix an issue where builtins or other Python library files were dropped from index if one of there files were opened and then closed
- Fix a bug where the workspace symbols where duplicated
- Fix the lifecycle of deleted addons, such that they can be recreated easily to avoid having odoo modules disappear forever
- Fix an issue where magic fields ("id", "display_name", ...) where added on every class in the model inheritence tree
- Fix an issue where completion items on class/model number could be duplicated due to multiple defintions
- Filter out magic fields on magic string features, for example trying to go to `"id"` field that just led to the class
- Fix the check for AST `is_built` for Javascript
- Fix the missing label details in completion for Javascript
- Fixes multiple JS missing or incorrect suggestiins in completions
- Fix the logical errors in the lifecycle of Javascript files, now CRUD is handled properly
- Fix a completion issue where `<lambda>` was shown in list of member symbols
- Fix the look up of `check_js_symbol_path_vacant` to use `path` instead of `name`
- Fix a crash that happens when you request references on a relational field that is references in CSV without `/`or`:`
