# OdooLS Error codes

# Nomenclature

Error codes from OdooLS have the format "OLSXZZZZ".
 - "OLS" for OdooLS.
 - "X" indicates if the error code is an INFO(1), WARNING(2), ERROR(3)
 - "Z" is the UID of the error, starting from 0001.

  - 0100 are errors related to modules dependencies
  - 0200 are errors related to manifests

## INFOs

## WARNINGs

### OLS20001

"XXXX not found".
The symbol you are trying to import was not found.
Check your python environment, the effective your sys.path and your addon paths.

### OLS20002

"XXXX not found".
The symbol you used as a base class can not be resolved.
Be sure that the symbol is refering to a valid python class.

### OLS20003

"XXXX not found".
The symbol you used as a base class is not a class, or not evaluated to a class.
Be sure that the symbol is refering to a valid python class.

### OLS20004

"Failed to evaluate XXXX".
The extension failed to evaluate a symbol. This occurs more specificaly when the extension detect a loop in the imports.
If your code is working fine, it can happen if you use too many "import *" that can break the extension flow for now.

### OLS20005

"Multiple definition found for base class".
The extension is unable to handle a base class that has multiple possible definitions. This warning should disappear in the future

### OLS20006
"Deprecation Warning: Since 17.0: odoo.tests.common.Form is deprecated, use odoo.tests.Form"
Form is no longer available on odoo.tests.common, thus it should not be imported from there.

### OLS20201

"The active key is deprecated".
Deprecation warning

## ERRORs

### OLS30001

Unable to parse file. Ruff_python_parser was unable to parse the file content.
See the error message to get the details from Ruff

### OLS30002

"Non-static method should have at least one parameter"

### OLS30101

"This model is not in the dependencies of your module."
With the Environment (often via self.env), you are trying to get a recordset of a model that is not defined in the current module or in the dependencies of the current module.
Even if it could work, this is strongly not recommended, as the model you are refering to could be not available on a live database.
Do not forget that even if your model is in an auto-installed module, it can be uninstalled by a user.

### OLS30102

"Unknown model. Check your addons path"
With the Environment (often via self.env), you are trying to get a recordset of a model that is unknown by OdooLS. It means that if the model exists in the codebase, OdooLS
is not aware of it. Check the addons path you provided to be sure that the module declaring this model is in an addon path.

### OLS30103

"XXXX is not in the dependencies of the module"
The symbol you are importing is in a module that is not in the dependencies of the current module.
You should check the dependencies in the \_\_manifest\_\_.py file of your module.

### OLS30104

"Model is inheriting from a model not declared in the dependencies of the module. Check the manifest."
The declared model is specifying an inheritance to a model that is not declared in the visible modules by the current one.
Consider updating the manifest of your module to include the relevant module.

### OLS30201

"A manifest shoul only contains one dictionnary".
A \_\_manifest\_\_.py file should be evaluated with a literal_eval to a single dictionnary. Do not store any other information in it.

### OLS30302

"Do not use dict unpacking to build your manifest".
Dict unpacking should be avoided. Do not create a dictionnary with values that must be unpacked like in ```{"a";1, **d}```

### OLS30303

"The name of the module should be a string".
String parsing error

### OLS30304

"The depends value should be a list".
list parsing error

### OLS30305

"The depends key should be a list of strings".
list parsing error

### OLS30306

"A module cannot depends on itself".
Do not add the current module name in the depends list.

### OLS30307

"The data value should be a list".
list parsing error

### OLS30308

"The data key should be a list of strings".
list parsing error

### OLS30309

"Manifest keys should be strings".
key parsing error

### OLS30310

"Module XXXX depends on YYYY which is not found. Please review your addons paths".
The module XXXX create a dependency on YYYY, but this module is not found with the current addon path.


