# Changelog

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