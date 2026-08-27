# Changelog

## [1.5.2] - 2026/08/27 - Fixes

### Server

- Handle special characters ' ', '"', '#', '%', '<', '>', '?', '[', ']', '^', '`', '{', '}', '|', '\\\\' in file URLs
- Semantic tokens can now be disabled in configuration with `disable_semantic_tokens_X = true`, with X in `[python, javascript, xml]`
- Fix and bring back OLS05001 in Xml fields references - `Unknown XML ID`
- Small refactoring of BuildSteps and SymbolKey replacement to improve stability by removing possible invalid states
- Windows builds now include line tables for better tracebacks in case of crash
- Add a warning diagnostic for calls to attributes that are not in dependencies
- GoTo features now select only the headers of classes and functions range. It allows us to bring back the default feature that call gotoreference instead of gotodefinition when clicking on the definition of the symbol

### VsCode

- Notification that suggests disabling the built-in JavaScript plugin will now only be displayed if tsserver is configured and activated

### Fixes

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