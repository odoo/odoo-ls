# Changelog

## [1.2.1] - 2026/03/18 - Fixes

We are addressing some errors in 1.2.0 before releasing the 1.3.0 in the next days

### Server

- Remove OLS05038 (empty function data in XML files), which is a valid use case.
- New warning if 'odoo/addons' is missing.
- Add completion of `inverse_name` if keyword is missing in the relational field.

### Fixes

- Fix crash on unexpected arguments in reading kwarg of self.env.ref function.
- When autocompleting function calls, OdooLS will now follow return values hidden behind multiple references.
- Fix annotation evaluation on functions that was dropped after being processed
- Fix 'with' statement type evaluation
- Fix the anti-duplication algorithm to prevent loading some symbols.
- Fix first argument evalution on static methods and classmethods.
- Fix evaluation of `inverse_name` if keyword is missing in the relational field declaration.
- Remove some useless logs.