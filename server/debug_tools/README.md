## Debug test with Rust Analyzer
If you wish to use the Rust Analyzer debugger in VS Code, add this to your settings.json:

```json
 "rust-analyzer.debug.engine": "ms-vscode.cpptools",
    "rust-analyzer.debug.engineSettings": {
        "cppdbg": {
            "miDebuggerPath": "/usr/bin/rust-gdb",
            "setupCommands": [
              {
                  "description": "Enable pretty-printing for gdb",
                  "text": "-enable-pretty-printing",
                  "ignoreFailures": false
              },
              {
                  "description": "Load SymbolTable GDB script",
                  "text": "source ${workspaceFolder}/server/debug_tools/symbol_table_gdb_script.py",
                  "ignoreFailures": false
              },
              {
                  "description": "Load Reference printer script",
                  "text": "source ${workspaceFolder}/server/debug_tools/rust_ref_printers.py",
                  "ignoreFailures": false
              },
              {
                  "description": "Set print depth limit to prevent infinite recursion",
                  "text": "set print max-depth 3",
                  "ignoreFailures": false
              },
          ]
        },
    },
```
