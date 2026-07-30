# Changelog

## [1.5.1] - 2026/07/30 - Day-1 Fixes

### Server

- Improve unpacking evaluations, resolving code like `for a, b in [(1, 2), (2, 3)]`. `a` and `b` will now properly be evaluated as `int`
- Remove logs that were created on each string occurence of opened files.
- Configuration files that now contains invalid key or syntax errors will be reported in VsCode.
- Update gungraun to 0.19.4
- Various code style enhancements

### Fixes

- Fix some missing references in the GoToReferences features (all references found in functions)
- Fix crash that occur on files that does not contain valid UTF-8
- Fix crash on invalid cycling evaluations.
- Fix Goto location when going on a python package
- Fix default config selection on non-odoo related workspace
- Fix Javascript internal dependency if the file is not part of the project
