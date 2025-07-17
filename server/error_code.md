# OdooLS Error codes

# Nomenclature

Error codes from OdooLS have the format "OLSXZZZZ".
 - "OLS" for OdooLS.
 - "X" indicates if the error code is an INFO(1), WARNING(2), ERROR(3)
 - "Z" is the UID of the error, starting from 0001.

  - 0100 are errors related to modules dependencies
  - 0200 are errors related to manifests
  - 0400 are errors related to XML

## INFOs

## WARNINGs

### OLS20001

"XXXX not found".
The symbol you are trying to import was not found.
Check your python environment, the effective your sys.path and your addon paths.

### OLS20002

"XXXX not found".
The symbol you used as a base class can not be resolved.
Be sure that the symbol is referring to a valid python class.

### OLS20003

"XXXX not found".
The symbol you used as a base class is not a class, or not evaluated to a class.
Be sure that the symbol is referring to a valid python class.

### OLS20004

"Failed to evaluate XXXX".
The extension failed to evaluate a symbol. This occurs more specifically when the extension detect a loop in the imports.
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
With the Environment (often via self.env), or in @api.returns, you are trying to get a recordset of a model that is not defined in the current module or in the dependencies of the current module.
Even if it could work, this is strongly not recommended, as the model you are referring to could be not available on a live database.
Do not forget that even if your model is in an auto-installed module, it can be uninstalled by a user.

### OLS30102

"Unknown model. Check your addons path"
With the Environment (often via self.env), or in @api.returns, you are trying to get a recordset of a model that is unknown by OdooLS. It means that if the model exists in the codebase, OdooLS
is not aware of it. Check the addons path you provided to be sure that the module declaring this model is in an addon path.

### OLS30103

"XXXX is not in the dependencies of the module"
The symbol you are importing is in a module that is not in the dependencies of the current module.
You should check the dependencies in the \_\_manifest\_\_.py file of your module.

### OLS30104

"Model is inheriting from a model not declared in the dependencies of the module. Check the manifest."
The declared model is specifying an inheritance to a model that is not declared in the visible modules by the current one.
Consider updating the manifest of your module to include the relevant module.

### OLS30105

"This model is inherited, but never declared."

The extension found some classes inheriting this model, but didn't find any class that declare it first, with only a _name.

### OLS30201

"A manifest should contain exactly one dictionary".
A \_\_manifest\_\_.py file should be evaluated with a literal_eval to a single dictionary. Do not store any other information in it.

### OLS30202
"A manifest should not have duplicate keys".
A \_\_manifest\_\_.py dictionary should have at most one definition per key

### OLS30203
"The name of the module should be a string"
The name key on the \_\_manifest\_\_.py should be a string

### OLS30204
"The depends value should be a list"
"depends" value in module manifest should be a list

### OLS30205
"The depends key should be a list of strings"
Values in the manifest's "depends" list should be strings

### OLS30206
"A module cannot depends on itself"
A module cannot have its own name as a dependency in its manifest

### OLS30207
"The data value should be a list"
"data" value in module manifest should be a list

### OLS30208
"The data key should be a list of strings"
Values in the manifest's "data" list should be strings

### OLS30209
"Manifest keys should be strings"
Keys of the dictionary in manifest files have to be string literals

### OLS30210
"Module `{module_name}` depends on `{wrong_dependency}` which is not found. Please review your addons paths"
Module has dependency on a dependency that is either wrong or does not exist. Check that module folder exists, and it contains `__init__.py` and `__manifest__.py`

### OLS30302

"Do not use dict unpacking to build your manifest".
Dict unpacking should be avoided. Do not create a dictionary with values that must be unpacked like in ```{"a";1, **d}```

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

### OLS30311
"First Argument to super must be a class"

### OLS30312
"Super calls outside a class scope must have at least one argument"

### OLS30313

"Domains should be a list of tuples".
The provided domain is not a list of tuples. A domain should be in the form [("field", "operator", "value")]

### OLS30314

"Domain tuple should have 3 elements".
Tuples in a domain should contains 3 elements: ("field", "operator", "value")

### OLS30315

"XXX takes Y positional arguments but Z was given".
Number of positional arguments given as parameter to the function is wrong.

### OLS30316

"XXX got an unexpected keyword argument 'YYY'".
You gave a named parameter that is not present in the function definition.

### OLS30317

"A String value in tuple should contains '&', '|' or '!'".
You gave a named parameter that is not present in the function definition.

### OLS30318

"Invalid comparison operator".
Tuples in search domains should be of one of these values:
"=", "!=", ">", ">=", "<", "<=", "=?", "=like", "like", "not like", "ilike", "not ilike", "=ilike", "in", "not in", "child_of", "parent_of", "any", "not any"

### OLS30319

"Missing tuple after a search domain operator".
If you use a search domain operator (&, ! or |), they should be followed by tuples or lists.

### OLS30320

"Invalid search domain field: XXX is not a member of YYY".
In a search domain, the first element of a tuple must be a member of the model, or of any model in a relation if expression contains "." (see documentation)

### OLS30321

"Invalid search domain field: Unknown date granularity".
In a search domain, when using a dot separator on a Date field, you can use the following granularities to access part of the date:
"year_number", "quarter_number", "month_number", "iso_week_number", "day_of_week", "day_of_month", "day_of_year", "hour_number", "minute_number", "second_number"

### OLS30322

"Invalid search domain field: Invalid dot notation".
In a search domain, when using a dot separator, it should be used either on a Date or Relational field.
If you used a relational field and get this error, check that the comodel of this field is valid.

### OLS30323
"Field does not exist on model or not in dependencies"
In related keyword argument or decorators api.onchange/depends/constrains, the field provided
should exist and be able to be resolved from current module

### OLS30324
"Field comodel_name not in dependencies"
In relational fields, comodel_name supplied exists but not in dependencies

### OLS30325
"Field comodel_name does not exist"
In relational fields, comodel_name does not exist in current configuration

### OLS30326
"Related field is not of the same type"
Type of references field in related keyword argument does not match the current field

### OLS30327
"Method does not exist on current model"
For compute, search, inverse arguments, this error is shown when the method is not found on the current model

### OLS30328
"Compute method not set to modify this field"
The compute method is set to modify a certain field(s).
Consider marking the modified field with the compute method

### OLS30329
"Unknown XML ID"
The XML ID you referenced has not been found in any XML in this module or its dependencies

### OLS30330
"Unspecified module. Add the module name before the XML ID: 'module.xml_id'"
You provided an XML ID that has no module specified. Specify the module which XML_ID belong to with 'module.xml_id'

### OLS30331
"Unknown module"
The given module is unknown

### OLS30400
"Invalid attribute"
odoo, openerp and data nodes can not contain this attribute.

### OLS30401
"Invalid node tag"
This tag is invalid

### OLS30402
"menuitem node must contains an id attribute"

### OLS30403
"Invalid attribute in menuitem node"
This attribute is not valid in a menuitem node

### OLS30404
"Sequence attribute must be a string representing a number"

### OLS30405
"SubmenuItem is not allowed when action and parent attributes are defined on a menuitem"

### OLS30406
"web_icon attribute is not allowed when parent is specified"

### OLS30407
"Invalid child node in menuitem"

### OLS30408
"parent attribute is not allowed in submenuitems"

### OLS30409 - OLS30443
"Various errors of RNG validation of XML files"

### OLS30444
"Data file not found in the module"

### OLS30445
"Data file should be an XML or a CSV file"

### OLS30446
"XML ID should not contain more than one dot"
An XML_ID should be in the format 'xml_id' or 'module.xml_id', but can't contains more dots

### OLS30447
"Parent menuitem with id XXXX does not exist"
A menuitem is specifying a parent that has not been declared before itself.

### OLS30448
"Action menuitem with id XXXX does not exist"
A menuitem is specifying an action that has not been declared before the menuitem.

### OLS30449
"Group with id XXXX does not exist"
A menuitem is specifying a group that has not been declared before the menuitem

### OLS30450
"model no in"