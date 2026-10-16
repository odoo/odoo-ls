# Changelog

## [1.6.1] - 2026/10/19 - Fixes

### Server

- In env.ref calls, take the "raise_if_not_found" parameter into account when raising diagnostics about the ID.
- The server now accepts clients that do not use dynamic registration, such as Claude Code or other AI coding tools.
- Improve the performance of the GoToReference feature by ~70% through various optimizations.
- Improve the performance of rebuilds after an edit.
- Implement a 'rejected configurations' status for invalid configurations, preventing crashes when a non-active configuration is invalid.

### PyCharm

- Avoid sending a duplicate restart notification to the server.
- Disable some PyCharm diagnostics that are irrelevant if OdooLS is installed and running.

### VS Code

- Update markdown-it from 14.3.0 to 14.3.1.

### Zed

- Activate OdooLS on JavaScript/TypeScript files.

### Fixes

- Fix a wrong "missing configuration" message that could be displayed if the user changed the currently loading configuration to "Disabled".
- Add missing AST branches to the search of the GoToReference feature.
- Handle multi-id data in .csv files for many2many fields, removing wrong diagnostics.
- Make the duplicate t-name ID diagnostic reliable, and take t-inherit-mode into account.
- Fix character offsets when publishing diagnostics in XML files around multi-byte characters.
- Fix evaluations that could target the same item multiple times.
- Fix a memory leak when editing files.
- Fix `__getitem__` methods sometimes not being reloaded.
