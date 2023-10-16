# Odoo LSP, how does it work?

You want to read the code of the Odoo language server? It is probably better to start here, as you'll have a small introduction of concepts and objectives of the project !

## A - Symbols

A symbol represents any element coming from the workspace: directory, file, but even variables, classes, functions, etc...
So a Symbol can be of multiple types:
- NAMESPACE: A directory
- PACKAGE: A directory containing the file `__init__.py`
- FILE: A file
- COMPILED: A compiled resource
- CLASS: A class
- VARIABLE: A variable
- FUNCTION: A function
- PRIMITIVE: An evaluation of a primitive variable

Together, symbols are creating a graph, like this:
  - A symbol has one and only one parent.
  - A symbol has three lists of children:
    - `symbols`: list of all exposed subsymbols in a file that you will get if you import the file
    - `modulesSymbols`: List of all subsymbols coming from the disk organization. Example: the content of a directory
    - `localSymbols`: All candidates to the `symbols` list, but that were rejected because erased by another declaration of a symbol with the same name.

Example:

Let's take the following file structure:

```
addons (dir)
    | module (dir)
        | __init__.py (file)
        | file.py (file)
```

`__init__.py` contains

```python
from . import file
```

et `file.py` contains

```python
import os
from odoo import models

variable = 5
print(variable)
variable = 6

class Test(models.Model):

    def func(self):
        return None
```

We will get the following symbols:

- `root`
  - type: ROOT
  - path: [sys.path, odoo_addons_path, config]
  - parent: None
  - symbols: []
  - moduleSymbols: `addons`
  - localSymbols: []
- `addons`
  - type: NAMESPACE
  - path: path_to_project + `addons`
  - parent: `root`
  - symbols: []
  - moduleSymbols: `module`
  - localSymbols: []
- `module`
  - type: PACKAGE
  - path: path_to_project + `addons/module`
  - parent: `addons`
  - symbols: [`file (A)`]
  - moduleSymbols: `file (B)`
  - localSymbols: []
- `file (A)`
  - type: VARIABLE
  - path: path_to_project + `addons/module/__init__.py`
  - parent: `module`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []
- `file (B)`
  - type: FILE
  - path: path_to_project + `addons/module/file.py`
  - parent: `module`
  - symbols: [`os`, `models`, `variable (B)`, `Test`]
  - moduleSymbols: []
  - localSymbols: [`variable (A)`]
- `os`
  - type: VARIABLE
  - path: path_to_project + `addons/module/file.py`
  - parent: `file (B)`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []
- `models`
  - type: VARIABLE
  - path: path_to_project + `addons/module/file.py`
  - parent: `file (B)`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []
- `variable (A)`
  - type: VARIABLE
  - path: path_to_project + `addons/module/file.py`
  - parent: `file (B)`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []
- `variable (B)`
  - type: VARIABLE
  - path: path_to_project + `addons/module/file.py`
  - parent: `file (B)`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []
- `Test`
  - type: CLASS
  - path: path_to_project + `addons/module/file.py`
  - parent: `file (B)`
  - symbols: [`func`]
  - moduleSymbols: []
  - localSymbols: []
- `func`
  - type: FUNCTION
  - path: path_to_project + `addons/module/file.py`
  - parent: `Test`
  - symbols: []
  - moduleSymbols: []
  - localSymbols: []

### Symbol id reference

A node of the graph (a symbol) can be identified by an unique "key", representing the path to follow from the root node to the symbol.
This path is a tuple of two lists. The first one contains all 'generic' elements, and the second one all the elements linked to a specific file.

Example based on the previously given structure:
```python
(["addons", "module", "file"], ["Test", "func"])
```
This path is expressed in two lists to resolve some ambiguïties: files versus variables

In a path with only one list, the symbol
```python
["addons", "module", "file"]
```
would be ambiguous. Indeed, is `"file"` referencing the file `file.py` or the variable `file` in `module/__init__.py`?

The double list is breaking this ambiguity:
```python
(["addons", "module", "file"], [])
```
will represent the file, while
```python
(["addons", "module"], ["file"])
```
will represent the variable.

Effectively, the first list is representing the disk structure (`moduleSymbols`), and the second one any element in a file (`symbols`). If an element of the second list is not present in a file, it will however fallback on the disk structure (`moduleSymbols`).


## B - Evaluation

A symbol contains an Evaluation. If possible, it will contains the evaluated value of the symbol.
Of course FILE, NAMESPACE, PACKAGE,... won't have any Evaluation.
A FUNCTION will have an Evaluation for their return value.


## C - Memory management

Python is good. But memory management can quickly become chaotic, and the structure of the code (graph and reference to symbols in evaluations and other caches) makes updating (deletion, invalidation) of memory very difficult. In particular, removing an element from the symbol tree does not invalidate all references to this element in all other elements of the code (evaluation and caches). To overcome this, it is possible either:
   - to use the notation (see above) allowing you to find an element in the tree and never reference it directly (but this involves a lot of searches in the tree, and a long storage of character strings representing the path in the tree)
   - to use weak pointers. ([weakref](https://docs.python.org/3/library/weakref.html))
   - use two-way references: a reference can be deleted by the object it references

The first solution was rejected by its cumbersome execution and by the difficulty of debugging the application, no reference being resolved during a breakpoint

The second solution, first implemented on a trial basis, quickly showed its limits during regular use. Indeed, a weakref only becomes invalid when the garbage collector has actually destroyed an object.
It is therefore sometimes possible to resurrect a deleted object:
```python
a = 5
ref = weakref.ref(a)
del a
old_a = ref() #if the garbage collector did not collect a already, old_a will be a strong ref to a
```

The third solution was then chosen and implemented in the references.py file to replace weakref. The principle is to offer the same interface as weakref, but not to rely on the garbage collector. When an object is deleted, it calls all references pointing to it and reports its deletion immediately.

The code is therefore organized as follows, and this principle must be a HOLY rule for all development in the project:
All symbols are strongly referenced *ONLY* by their parent symbol. ANY OTHER reference to a symbol from another place in the code, or even between symbols must be done via weak references (RegisteredRef).
Thus the deletion of a symbol is immediate and effective.

### Asynchronous access

This slowly leads to another problem: when code takes a reference to a weakref and therefore a symbol, the reference number increases to 2 (or more), and the deletion of a symbol is no longer immediate.
More generally, we would like to ensure data consistency across several threads.
This is done by 3 methods:
`Odoo.acquire_write()`, `Odoo.acquire_read()` and `Odoo.upgrade_to_write()`

Before any access to LSP data, you should request access corresponding to the need, and stick to it. If a read access needs to be elevated to a write access, `Odoo.upgrade_to_write()` can be used. It is guaranteed that only one write access can take place simultaneously, but there can be as many readers as you want. No write and read access can be simultaneous.
Requesting write access will make the plugin single-threaded for the duration of the lock.

## D - Construction de la base de données

The symbol tree must of course be well constructed and maintained during the utilization of the extension.

- A) Build the architecture
- B) Evaluate the architecture
- C) Initialize Odoo stuff (models, etc...)
  - repeat A and B for base, then for modules
- D) Valider le code

### Build the architecture

This step prepares the basic tree by constructing all the symbols detected in the files. This part loads all the python files following the different imports (+ the `tests` folders in the modules) and builds the corresponding tree.

*Note: External libraries are also parsed if found. however, the code looks first for an existing stubs in the embedded `typeshed` repo. In order to reduce the memory used, the tree of external elements is frozen once parsed, no changes can be made to it subsequently and the caches are frozen. (TODO: generalize to any folder outside of workspace)*

### Architecture Evaluation

This part contains the evaluation of symbols. If possible, the code will try to evaluate and associate the value found with the symbol.
This evaluation concerns all variables at the root of a file or class only. Symbols under a function are not evaluated at this step, as they often need to have the Odoo structure constructed. However, if a doc exists for the function, the return value can already be evaluated (TODO?)

### Odoo object initialization

This step goes over the symbols previously created to reconstruct the structure of Odoo: models, modules, etc... are grouped and hidden for faster access later.
The storage of original data REMAINS the symbol tree.
However, the Odoo class contains objects with references to this tree:
- `models`: dictionary, which for each model name (ex: `"mail.thread"`), associates a Model object. This object collects all the symbols representing a model, and makes it possible to respond to queries on a model to which it will respond using these symbols.
- `modules`: brings together all the symbols representing an Odoo module, as well as the information from the manifest linked to it.

This step also allows too the creation of symbols that escape the first step due to their dynamic nature: `self.env`, `env.cr`, etc...

### Validation

This last step does not build the database, but makes a final pass over it in order to be able to generate diagnostics now that the database is complete.
This is the step that adds all the information/warnings/errors to the project.

## E - Updates and dependencies

Once the database is built, it is obviously a matter of keeping it up to date.
For each code update, it is therefore necessary to reflect the changes in the symbol structures.

A first possibility would be to analyze the change made and to locally change the symbols concerned. However, this solution involves a lot of code and complexity to successfully isolate these changes and maintain consistency in the final tree.

The other solution is to rebuild the entire file during any modification. In reality, when a file is loaded into memory, analyzing it is very fast, and the cost of reconstructing the entire file instead of the symbol concerned is far too low to justify much heavier code.

Once the file is rebuilt, it is necessary to ensure that all elements other than these symbols are also updated. Whether it was the Evaluations pointing to these symbols or other parts of the tree whose construction depended on them.
For this, during the construction/evaluation/validation of a symbol, if another enters the process, the first symbol is noted as "dependent on" on the second symbol.
When rebuilding a file, all deleted symbols add their (inverted) dependencies into a list of symbols to rebuild/evaluate/validate. These will potentially do the same with their dependencies.

Note: In the case of an incorrect code, it may happen that an evaluation fails because the symbol which is supposed to be pointed to does not exist. In this case the dependency cannot be created and no error resolution can be done if the symbol were to appear. From then on, all missing symbols are saved in the Odoo class, and any new symbol checks this list if it does not resolve an existing error.

### Dependency types

When a symbol registers its dependency on another symbol, it indicates the type of dependency:
- __ARCH__: the construction of the symbol depends on the second symbol. A typical example would be `import *`. this instruction will create a whole series of symbols which depend on the imported file. If the imported file is modified, then the symbols added by `import *` must be relisted
- __ARCH_EVAL__: The architecture of the symbols does not depend on the second symbol, but its evaluation does. This dependency is different from the first, because the symbol list is not modified. It makes it possible to not propagate an architectural modification too far.
- __ODOO__: elements specific to Odoo have been modified (_inherit, _name, base class, etc...)
- __VALIDATION__: Validation indicates that an error could occur if the second symbol is modified.

## F - as Features

The language server, with its database and notifications, can provide the following functionalities:

- __Diagnostics__: Gives a list of indications on the analyzed code. These diagnostics can be found in the "problems" tab on vscode, and usually visible in the code via a blue/yellow/red underline.
- __Hover__: Gives an information bubble to display concerning the element currently under the mouse cursor
- __Go To Definition__: Allows you to go directly to the location of the symbol declaration under the cursor.
- __Autocompletion__: provides a list of candidates to complete the code under the cursor
- __Refactoring (Coming soon)__: Help with refactoring with "replace all".

## G - Evaluation requests

In order to respond to autocompletion queries, hovers and goto definitions, parsoUtils.evaluateType is a function capable of evaluating the type of a piece of code given as a parameter.
This is the main interface for interacting with the knowledge base.

This function uses a context to transfer important information from step to step. This context contains three keys: args, parent and module.

In the case of `self.env["test"].func(a)`
- context when evaluating self will be `{module: currentModule}`
- context when evaluating env will be `{args: None, parent: self, module: currentModule}`
- context when evaluating env.__getitem__ will be `{args: "test", parent: env, module: currentModule}`
- the context when evaluating func() will be `{args: {a}, parent: TestModel, module: currentModule}`
