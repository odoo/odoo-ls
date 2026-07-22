# Changelog

## [1.5.0] - 2026/07/22 - JavaScript and performance

This new update lays the foundation for support for JavaScript, OWL, and templates. OdooLS now uses OXC internally to parse and read JavaScript files, and requires you to install tsserver to provide all other features. It works by creating an internal project with a dynamic `tsconfig.json` that is served to tsserver, depending on your project configuration.

This update also introduces a major refactor of the project, especially its memory management, leading to more than a 60 % performance improvement (mostly on loading time) and additional 25 % memory savings. We moved from storing references in `Rc<RefCell<>>` to an arena. This provides better performance, a better memory layout, and better use of the Rust borrow checker. With these checks done at compile time, there will be fewer runtime crashes.

### JavaScript key information

- Introduced features: 
  - Javascript files: GotoDefinition, hover, completion, references, semantic tokens, document symbols, workspace symbols. All these features are able to resolve @module imports.
  - Templates and components are linked, and you can naviguate from one to the other by doing a gotodefinition on the template name or the component `template` attribute.
  - Javascript code in XML templates files are now evaluated, activating all features for these pieces of code.
- To use tsserver, you must install it first. It is not bundled with the extension. You can install it globally (`npm install -g typescript6`) or install it another way and provide the path/command OdooLS should use through the `tsserver_command` option in your config file.
- You can disable the JavaScript feature with the `disable_javascript` option in your config file.
- You can enable TypeScript diagnostics in JavaScript files by setting `ts_check: true` in your config file (equivalent to `checkJS` in a `jsconfig.json` or `tsconfig.json`). It is also equivalent to a `@ts-check` at the top of a .js file.
- Autocompletion in XML files is currently not working in PyCharm, as it seems to prevent the LSP from working in these files. We are trying to find a solution.
- Please note that this is a pre-release version. Feel free to send us any feedback. This version is still based on TypeScript 6, but Microsoft has just released TypeScript 7 (written in Go). Be sure to install TypeScript 6 for now (we will move to TypeScript 7 in the future).

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

### VsCode

- Display configuration errors to draw user attention to invalid settings.
- Add a popup that strongly suggests disabling the built-in JS plugin in VS Code for your workspace. It will avoid having 2 instances of tsserver running serving the same answers to your requests.

### Fixes

- The server could sometimes get stuck in a state that consumed 100% CPU until the next request (typing, hover, etc.).
- Remove duplicate references found on the same line in XML files.
