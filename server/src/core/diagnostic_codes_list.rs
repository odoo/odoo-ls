// This file contains only the list of diagnostic codes and their documentation for OdooLS diagnostics.
// Each entry is a doc comment and a code, followed by its default severity and message template.
//
// To add a new code, add it here in the same format.

use serde::{Deserialize, Serialize};
use super::{DiagnosticInfo, DiagnosticSetting};

/*
Sections:
- Import resolution
- Manifest validation
- Python validation
- XML validation
 */

diagnostic_codes! {
    /** "{0} not found".
    * The symbol you are trying to import was not found.
    * Check your python environment, the effective your sys.path and your addon paths.
    */
    OLS20001, (DiagnosticSetting::Warning, "{0} not found"),
    /** "{0} not found".
    * The symbol you used as a base class can not be resolved.
    * Be sure that the symbol is referring to a valid python class.
    */
    OLS20002, (DiagnosticSetting::Warning, "{0} not found"),
    /** "{0} not found".
    * The symbol you used as a base class is not a class, or not evaluated to a class.
    * Be sure that the symbol is referring to a valid python class.
    */
    OLS20003, (DiagnosticSetting::Warning, "{0} not found"),
    /** "Failed to evaluate {0}".
    * The extension failed to evaluate a symbol. This occurs more specifically when the extension detect a loop in the imports.
    * If your code is working fine, it can happen if you use too many "import *" that can break the extension flow for now.
    */
    OLS20004, (DiagnosticSetting::Warning, "Failed to evaluate {0}"),
    /** "Multiple definition found for base class".
    * The extension is unable to handle a base class that has multiple possible definitions. This Warning should disappear in the future
    */
    OLS20005, (DiagnosticSetting::Warning, "Multiple definition found for base class"),
    /** "Deprecation Warning: Since 17.0: odoo.tests.common.Form is deprecated, use odoo.tests.Form".
    * Form is no longer available on odoo.tests.common, thus it should not be imported from there.
    */
    OLS20006, (DiagnosticSetting::Warning, "Deprecation Warning: Since 17.0: odoo.tests.common.Form is deprecated, use odoo.tests.Form"),
    /** "The active key is deprecated".
    * Deprecation Warning
    */
    OLS20201, (DiagnosticSetting::Warning, "The active key is deprecated"),
    /** Unable to parse file. Ruff_python_parser was unable to parse the file content.
    * See the error message to get the details from Ruff
    */
    OLS30001, (DiagnosticSetting::Error, "Unable to parse file. Ruff_python_parser was unable to parse the file content. See the error message to get the details from Ruff"),
    /** "Non-static method should have at least one parameter"
    */
    OLS30002, (DiagnosticSetting::Error, "Non-static method should have at least one parameter"),
    /** "This model is not in the dependencies of your module."
    * With the Environment (often via self.env), or in @api.returns, you are trying to get a recordset of a model that is not defined in the current module or in the dependencies of the current module.
    * Even if it could work, this is strongly not recommended, as the model you are referring to could be not available on a live database.
    * Do not forget that even if your model is in an auto-installed module, it can be uninstalled by a user.
    */
    OLS30101, (DiagnosticSetting::Error, "This model is not in the dependencies of your module."),
    /** "Unknown model. Check your addons path"
    * With the Environment (often via self.env), or in @api.returns, you are trying to get a recordset of a model that is unknown by OdooLS.
    * It means that if the model exists in the codebase, OdooLS is not aware of it.
    * Check the addons path you provided to be sure that the module declaring this model is in an addon path.
    */
    OLS30102, (DiagnosticSetting::Error, "Unknown model. Check your addons path"),
    /** "{0} is not in the dependencies of the module"
    * The symbol you are importing is in a module that is not in the dependencies of the current module.
    * You should check the dependencies in the __manifest__.py file of your module.
    */
    OLS30103, (DiagnosticSetting::Error, "{0} is not in the dependencies of the module"),
    /** "Model is inheriting from a model not declared in the dependencies of the module. Check the manifest."
    * The declared model is specifying an inheritance to a model that is not declared in the visible modules by the current one.
    * Consider updating the manifest of your module to include the relevant module.
    */
    OLS30104, (DiagnosticSetting::Error, "Model is inheriting from a model not declared in the dependencies of the module. Check the manifest."),
    /** "This model is inherited, but never declared."
    * The extension found some classes inheriting this model, but didn't find any class that declare it first, with only a _name.
    */
    OLS30105, (DiagnosticSetting::Error, "This model is inherited, but never declared."),
    /** "A manifest should contain exactly one dictionary".
    * A __manifest__.py file should be evaluated with a literal_eval to a single dictionary.
    * Do not store any other information in it.
    */
    OLS30201, (DiagnosticSetting::Error, "A manifest should contain exactly one dictionary"),
    /** "A manifest should not have duplicate keys".
    * A __manifest__.py dictionary should have at most one definition per key
    */
    OLS30202, (DiagnosticSetting::Error, "A manifest should not have duplicate keys"),
    /** "The name of the module should be a string".
    * The name key on the __manifest__.py should be a string
    */
    OLS30203, (DiagnosticSetting::Error, "The name of the module should be a string"),
    /** "The depends value should be a list".
    * "depends" value in module manifest should be a list
    */
    OLS30204, (DiagnosticSetting::Error, "The depends value should be a list"),
    /** "The depends key should be a list of strings".
    * Values in the manifest's "depends" list should be strings
    */
    OLS30205, (DiagnosticSetting::Error, "The depends key should be a list of strings"),
    /** "A module cannot depends on itself".
    * A module cannot have its own name as a dependency in its manifest
    */
    OLS30206, (DiagnosticSetting::Error, "A module cannot depends on itself"),
    /** "The data value should be a list".
    * "data" value in module manifest should be a list
    */
    OLS30207, (DiagnosticSetting::Error, "The data value should be a list"),
    /** "The data key should be a list of strings".
    * Values in the manifest's "data" list should be strings
    */
    OLS30208, (DiagnosticSetting::Error, "The data key should be a list of strings"),
    /** "Manifest keys should be strings".
    * Keys of the dictionary in manifest files have to be string literals
    */
    OLS30209, (DiagnosticSetting::Error, "Manifest keys should be strings"),
    /** "Module {0} depends on {1} which is not found. Please review your addons paths".
    * Module has dependency on a dependency that is either wrong or does not exist.
    * Check that module folder exists, and it contains __init__.py and __manifest__.py
    */
    OLS30210, (DiagnosticSetting::Error, "Module {0} depends on {1} which is not found. Please review your addons paths"),
    /** "Do not use dict unpacking to build your manifest".
    * Dict unpacking should be avoided. Do not create a dictionary with values that must be unpacked like in {"a":1, **d}
    */
    OLS30302, (DiagnosticSetting::Error, "Do not use dict unpacking to build your manifest"),
    /** "The name of the module should be a string".
    * String parsing error
    */
    OLS30303, (DiagnosticSetting::Error, "The name of the module should be a string"),
    /** "The depends value should be a list".
    * list parsing error
    */
    OLS30304, (DiagnosticSetting::Error, "The depends value should be a list"),
    /** "The depends key should be a list of strings".
    * list parsing error
    */
    OLS30305, (DiagnosticSetting::Error, "The depends key should be a list of strings"),
    /** "A module cannot depends on itself".
    * Do not add the current module name in the depends list.
    */
    OLS30306, (DiagnosticSetting::Error, "A module cannot depends on itself"),
    /** "The data value should be a list".
    * list parsing error
    */
    OLS30307, (DiagnosticSetting::Error, "The data value should be a list"),
    /** "The data key should be a list of strings".
    * list parsing error
    */
    OLS30308, (DiagnosticSetting::Error, "The data key should be a list of strings"),
    /** "Manifest keys should be strings".
    * key parsing error
    */
    OLS30309, (DiagnosticSetting::Error, "Manifest keys should be strings"),
    /** "Module {0} depends on {1} which is not found. Please review your addons paths".
    * The module {0} create a dependency on {1}, but this module is not found with the current addon path.
    */
    OLS30310, (DiagnosticSetting::Error, "Module {0} depends on {1} which is not found. Please review your addons paths"),
    /** "First Argument to super must be a class"
    */
    OLS30311, (DiagnosticSetting::Error, "First Argument to super must be a class"),
    /** "Super calls outside a class scope must have at least one argument"
    */
    OLS30312, (DiagnosticSetting::Error, "Super calls outside a class scope must have at least one argument"),
    /** "Domains should be a list of tuples".
    * The provided domain is not a list of tuples. A domain should be in the form [("field", "operator", "value")]
    */
    OLS30313, (DiagnosticSetting::Error, "Domains should be a list of tuples"),
    /** "Domain tuple should have 3 elements".
    * Tuples in a domain should contains 3 elements: ("field", "operator", "value")
    */
    OLS30314, (DiagnosticSetting::Error, "Domain tuple should have 3 elements"),
    /** "{0} takes {1} positional arguments but {2} was given".
    * Number of positional arguments given as parameter to the function is wrong.
    */
    OLS30315, (DiagnosticSetting::Error, "{0} takes {1} positional arguments but {2} was given"),
    /** "{0} got an unexpected keyword argument '{1}'".
    * You gave a named parameter that is not present in the function definition.
    */
    OLS30316, (DiagnosticSetting::Error, "{0} got an unexpected keyword argument '{1}'"),
    /** "A String value in tuple should contains '&', '|' or '!'".
    * You gave a named parameter that is not present in the function definition.
    */
    OLS30317, (DiagnosticSetting::Error, "A String value in tuple should contains '&', '|' or '!'"),
    /** "Invalid comparison operator".
    * Tuples in search domains should be of one of these values:
    * "=", "!=", ">", ">=", "<", "<=", "=?", "=like", "like", "not like", "ilike", "not ilike", "=ilike", "in", "not in", "child_of", "parent_of", "any", "not any"
    */
    OLS30318, (DiagnosticSetting::Error, "Invalid comparison operator"),
    /** "Missing tuple after a search domain operator".
    * If you use a search domain operator (&, ! or |), they should be followed by tuples or lists.
    */
    OLS30319, (DiagnosticSetting::Error, "Missing tuple after a search domain operator"),
    /** "Invalid search domain field: {0} is not a member of {1}".
    * In a search domain, the first element of a tuple must be a member of the model, or of any model in a relation if expression contains "." (see documentation)
    */
    OLS30320, (DiagnosticSetting::Error, "Invalid search domain field: {0} is not a member of {1}"),
    /** "Invalid search domain field: Unknown date granularity".
    * In a search domain, when using a dot separator on a Date field, you can use the following granularities to access part of the date:
    * "year_number", "quarter_number", "month_number", "iso_week_number", "day_of_week", "day_of_month", "day_of_year", "hour_number", "minute_number", "second_number"
    */
    OLS30321, (DiagnosticSetting::Error, "Invalid search domain field: Unknown date granularity"),
    /** "Invalid search domain field: Invalid dot notation".
    * In a search domain, when using a dot separator, it should be used either on a Date or Relational field.
    * If you used a relational field and get this error, check that the comodel of this field is valid.
    */
    OLS30322, (DiagnosticSetting::Error, "Invalid search domain field: Invalid dot notation"),
    /** "Field does not exist on model or not in dependencies".
    * In related keyword argument or decorators api.onchange/depends/constrains, the field provided
    * should exist and be able to be resolved from current module
    */
    OLS30323, (DiagnosticSetting::Error, "Field does not exist on model or not in dependencies"),
    /** "Field comodel_name not in dependencies".
    * In relational fields, comodel_name supplied exists but not in dependencies
    */
    OLS30324, (DiagnosticSetting::Error, "Field comodel_name not in dependencies"),
    /** "Field comodel_name does not exist".
    * In relational fields, comodel_name does not exist in current configuration
    */
    OLS30325, (DiagnosticSetting::Error, "Field comodel_name does not exist"),
    /** "Related field is not of the same type".
    * Type of references field in related keyword argument does not match the current field
    */
    OLS30326, (DiagnosticSetting::Error, "Related field is not of the same type"),
    /** "Method does not exist on current model".
    * For compute, search, inverse arguments, this error is shown when the method is not found on the current model
    */
    OLS30327, (DiagnosticSetting::Error, "Method does not exist on current model"),
    /** "Compute method not set to modify this field".
    * The compute method is set to modify a certain field(s).
    * Consider marking the modified field with the compute method
    */
    OLS30328, (DiagnosticSetting::Error, "Compute method not set to modify this field"),
    /** "Unknown XML ID".
    * The XML ID you referenced has not been found in any XML in this module or its dependencies
    */
    OLS30329, (DiagnosticSetting::Error, "Unknown XML ID"),
    /** "Unspecified module. Add the module name before the XML ID: 'module.xml_id'".
    * You provided an XML ID that has no module specified. Specify the module which XML_ID belong to with 'module.xml_id'
    */
    OLS30330, (DiagnosticSetting::Error, "Unspecified module. Add the module name before the XML ID: 'module.xml_id'"),
    /** "Unknown module".
    * The given module is unknown
    */
    OLS30331, (DiagnosticSetting::Error, "Unknown module"),
    /** "Invalid attribute".
    * odoo, openerp and data nodes can not contain this attribute.
    */
    OLS30400, (DiagnosticSetting::Error, "Invalid attribute"),
    /** "Invalid node tag".
    * This tag is invalid
    */
    OLS30401, (DiagnosticSetting::Error, "Invalid node tag"),
    /** "menuitem node must contains an id attribute"
    */
    OLS30402, (DiagnosticSetting::Error, "menuitem node must contains an id attribute"),
    /** "Invalid attribute in menuitem node".
    * This attribute is not valid in a menuitem node
    */
    OLS30403, (DiagnosticSetting::Error, "Invalid attribute in menuitem node"),
    /** "Sequence attribute must be a string representing a number"
    */
    OLS30404, (DiagnosticSetting::Error, "Sequence attribute must be a string representing a number"),
    /** "SubmenuItem is not allowed when action and parent attributes are defined on a menuitem"
    */
    OLS30405, (DiagnosticSetting::Error, "SubmenuItem is not allowed when action and parent attributes are defined on a menuitem"),
    /** "web_icon attribute is not allowed when parent is specified"
    */
    OLS30406, (DiagnosticSetting::Error, "web_icon attribute is not allowed when parent is specified"),
    /** "Invalid child node in menuitem"
    */
    OLS30407, (DiagnosticSetting::Error, "Invalid child node in menuitem"),
    /** "parent attribute is not allowed in submenuitems"
    */
    OLS30408, (DiagnosticSetting::Error, "parent attribute is not allowed in submenuitems"),
    /** "Various errors of RNG validation of XML files"
    */
    OLS30409, (DiagnosticSetting::Error, "Various errors of RNG validation of XML files"),
    /** "Data file not found in the module"
    */
    OLS30444, (DiagnosticSetting::Error, "Data file not found in the module"),
    /** "Data file should be an XML or a CSV file"
    */
    OLS30445, (DiagnosticSetting::Error, "Data file should be an XML or a CSV file"),
    /** Invalid XML ID '{0}'. It should not contain more than one dot.
    * An XML_ID should be in the format 'xml_id' or 'module.xml_id', but can't contains more dots
    */
    OLS30446, (DiagnosticSetting::Error, "Invalid XML ID '{0}'. It should not contain more than one dot."),
}
