// This file contains only the list of diagnostic codes and their documentation for OdooLS diagnostics.
// Each entry is a doc comment and a code, followed by its default severity and message template.
//
// To add a new code, add it here in the same format.
/*
Error codes for OdooLS diagnostics are in the format:
OLS<Section><CodeNumber> (OLSXXYYY)
Sections:
- Python / Syntax 01
- Import 02
- Odoo / inheritance, model dependency, missing dependencies ,modules... 03
    - 033XX: Deprecations
- Manifest 04
- XML/CSV 05
 */

diagnostic_codes! {
/**
See the error message to get the details from Ruff
*/
OLS01000, DiagnosticSetting::Error, "Unable to parse file. Ruff_python_parser was unable to parse the file content. See the error message to get the details from Ruff",
/**
* The symbol you used as a base class can not be resolved.
* Be sure that the symbol is referring to a valid python class.
*/
OLS01001, DiagnosticSetting::Warning, "{0} not found",
/**
* The symbol you used as a base class is not a class, or not evaluated to a class.
* Be sure that the symbol is referring to a valid python class.
*/
OLS01002, DiagnosticSetting::Warning, "{0} not found",
/**
* The extension is unable to handle a base class that has multiple possible definitions. This Warning should disappear in the future
*/
OLS01003, DiagnosticSetting::Warning, "Multiple definition found for base class",
/**
*/
OLS01004, DiagnosticSetting::Error, "Non-static method should have at least one parameter",
/**
*/
OLS01005, DiagnosticSetting::Error, "First Argument to super must be a class",
/**
*/
OLS01006, DiagnosticSetting::Error, "Super calls outside a class scope must have at least one argument",
/**
* Number of positional arguments given as parameter to the function is wrong.
*/
OLS01007, DiagnosticSetting::Error, "{0} takes {1} positional arguments but {2} was given",
/**
* You gave a named parameter that is not present in the function definition.
*/
OLS01008, DiagnosticSetting::Error, "{0} got an unexpected keyword argument '{1}'",
/**
 * Arguments are not valid for all function or method definitions
 */
OLS01009, DiagnosticSetting::Warning, "Arguments are not valid for all function or method definitions",
/**
 * Arguments are not valid for all function or method definitions
 */
OLS01010, DiagnosticSetting::Error, "Missing keyword-only argument(s): {0}",
/**
* Check your python environment, the effective your sys.path and your addon paths.
*/
OLS02001, DiagnosticSetting::Warning, "{0} not found",
/**
* The extension failed to evaluate a symbol. This occurs more specifically when the extension detect a loop in the imports.
* If your code is working fine, it can happen if you use too many "import *" that can break the extension flow for now.
*/
OLS02002, DiagnosticSetting::Warning, "Failed to evaluate {0}",
/**
* With the Environment (often via self.env, or in @api.returns, you are trying to get a recordset of a model that is not defined in the current module or in the dependencies of the current module.
* Even if it could work, this is strongly not recommended, as the model you are referring to could be not available on a live database.
* Do not forget that even if your model is in an auto-installed module, it can be uninstalled by a user.
*/
OLS03001, DiagnosticSetting::Error, "This model is not in the dependencies of your module.",
/**
* With the Environment (often via self.env, or in @api.returns, you are trying to get a recordset of a model that is unknown by OdooLS.
* It means that if the model exists in the codebase, OdooLS is not aware of it.
* Check the addons path you provided to be sure that the module declaring this model is in an addon path.
*/
OLS03002, DiagnosticSetting::Error, "Unknown model. Check your addons path",
/**
* The symbol you are importing is in a module that is not in the dependencies of the current module.
* You should check the dependencies in the __manifest__.py file of your module.
*/
OLS03003, DiagnosticSetting::Error, "{0} is not in the dependencies of the module",
/**
* The declared model is specifying an inheritance to a model that is not declared in the visible modules by the current one.
* Consider updating the manifest of your module to include the relevant module.
*/
OLS03004, DiagnosticSetting::Error, "Model is inheriting from a model not declared in the dependencies of the module. Check the manifest.",
/**
* The extension found some classes inheriting this model, but didn't find any class that declare it first, with only a _name.
*/
OLS03005, DiagnosticSetting::Error, "This model is inherited, but never declared.",
/**
* The provided domain is not a list of tuples. A domain should be in the form [("field", "operator", "value")]
*/
OLS03006, DiagnosticSetting::Error, "Domains should be a list of tuples",
/**
* Tuples in a domain should contains 3 elements: ("field", "operator", "value")
*/
OLS03007, DiagnosticSetting::Error, "Domain tuple should have 3 elements",
/**
* For an string that represents an operator in a search domain, the only valid values are '&', '|' or '!'
*/
OLS03008, DiagnosticSetting::Error, "A String value in search domain tuple should be '&', '|' or '!'",
/**
* Comparison operators in search domain tuples should be of one of these values:
* "=", "!=", ">", ">=", "<", "<=", "=?", "=like", "like", "not like", "ilike", "not ilike", "=ilike", "in", "not in", "child_of", "parent_of", "any", "not any"
*/
OLS03009, DiagnosticSetting::Error, "Invalid comparison operator",
/**
* If you use a search domain operator (&, ! or |, they should be followed by tuples or lists.
*/
OLS03010, DiagnosticSetting::Error, "Missing tuple after a search domain operator",
/**
* In a search domain, the first element of a tuple must be a member of the model, or of any model in a relation if expression contains "." (see documentation)
*/
OLS03011, DiagnosticSetting::Error, "Invalid search domain field: {0} is not a member of {1}",
/**
* In a search domain, when using a dot separator on a Date field, you can use the following granularities to access part of the date:
* "year_number", "quarter_number", "month_number", "iso_week_number", "day_of_week", "day_of_month", "day_of_year", "hour_number", "minute_number", "second_number"
*/
OLS03012, DiagnosticSetting::Error, "Invalid search domain field: Unknown date granularity",
/**
* In a search domain, when using a dot separator, it should be used either on a Date or Relational field.
* If you used a relational field and get this error, check that the comodel of this field is valid.
*/
OLS03013, DiagnosticSetting::Error, "Invalid search domain field: Invalid dot notation",
/**
* In related keyword argument or decorators api.onchange/depends/constrains, the field provided
* should exist and be able to be resolved from current module
*/
OLS03014, DiagnosticSetting::Error, "Field does not exist on model or not in dependencies",
/**
* In relational fields, comodel_name's value supplied exists but is not in dependencies
*/
OLS03015, DiagnosticSetting::Error, "Field comodel_name's value is not in dependencies",
/**
* In relational fields, comodel_name's value is does not exist in current configuration
*/
OLS03016, DiagnosticSetting::Error, "Field comodel_name's values is does not exist",
/**
* Type of references field in related keyword argument does not match the current field
*/
OLS03017, DiagnosticSetting::Error, "Related field is not of the same type",
/**
* For compute, search, inverse arguments, this error is shown when the method is not found on the current model
*/
OLS03018, DiagnosticSetting::Error, "Method does not exist on current model",
/**
* The compute method is set to modify a certain field(s).
* Consider marking the modified field with the compute method
*/
OLS03019, DiagnosticSetting::Error, "Compute method not set to modify this field",
/**
 * _name is set on a class which creates a model, but the name is already used by another model.
 * Hence, this model is shadowing an existing model.
 */
OLS03020, DiagnosticSetting::Warning, "Model {0} is shadowing an existing model in dependencies",
/**
 * On a One2many field, the inverse_name should be a field on the comodel.
 */
OLS03021, DiagnosticSetting::Error, "Inverse field {0} does not exist on comodel {1}",
/**
 * On a One2many field, the inverse_name should be a field on the comodel that is a Many2one to the current model.
 */
OLS03022, DiagnosticSetting::Error, "Inverse field is not a Many2one field",
/**
 * On a One2many field, the inverse_name should be a field on the comodel that is a Many2one to the current model
 * -> current_model is not right
 */
OLS03023, DiagnosticSetting::Error, "Inverse field {0} is not pointing to the current model {1}, but rather to {2}",
/**
* Form is no longer available on odoo.tests.common, thus it should not be imported from there.
*/
OLS03301, DiagnosticSetting::Warning, "Deprecation Warning: Since 17.0: odoo.tests.common.Form is deprecated, use odoo.tests.Form",
/**
* Deprecation Warning
*/
OLS03302, DiagnosticSetting::Warning, "The active key is deprecated",
/**
* A __manifest__.py file should be evaluated with a literal_eval to a single dictionary.
* Do not store any other information in it.
*/
OLS04001, DiagnosticSetting::Error, "A manifest should contain exactly one dictionary",
/**
* A __manifest__.py dictionary should have at most one definition per key
*/
OLS04002, DiagnosticSetting::Error, "A manifest should not have duplicate keys",
/**
* The name key on the __manifest__.py should be a string
*/
OLS04003, DiagnosticSetting::Error, "The name of the module should be a string",
/**
* "depends" value in module manifest should be a list
*/
OLS04004, DiagnosticSetting::Error, "The depends value should be a list",
/**
* Values in the manifest's "depends" list should be strings
*/
OLS04005, DiagnosticSetting::Error, "The depends key should be a list of strings",
/**
* A module cannot have its own name as a dependency in its manifest
*/
OLS04006, DiagnosticSetting::Error, "A module cannot depends on itself",
/**
* "data" value in module manifest should be a list
*/
OLS04007, DiagnosticSetting::Error, "The data value should be a list",
/**
* Values in the manifest's "data" list should be strings
*/
OLS04008, DiagnosticSetting::Error, "The data key should be a list of strings",
/**
* Keys of the dictionary in manifest files have to be string literals
*/
OLS04009, DiagnosticSetting::Error, "Manifest keys should be strings",
/**
* Module has dependency on a dependency that is either wrong or does not exist.
* Check that module folder exists, and it contains __init__.py and __manifest__.py
*/
OLS04010, DiagnosticSetting::Error, "Module {0} depends on {1} which is not found. Please review your addons paths",
/**
* Dict unpacking should be avoided. Do not create a dictionary with values that must be unpacked like in {"a":1, **d}
*/
OLS04011, DiagnosticSetting::Error, "Do not use dict unpacking to build your manifest",
/**
* Module dependency cycle: make sure nested dependencies are correct
*/
OLS04012, DiagnosticSetting::Error, "Module dependency: module {0} depends on current module",
/**
* Parsing error in XML file
*/
OLS05000, DiagnosticSetting::Error, "Unable to parse XML file: {0}",
/**
* The XML ID you referenced has not been found in any XML in this module or its dependencies
*/
OLS05001, DiagnosticSetting::Error, "Unknown XML ID",
/**
* You provided an XML ID that has no module specified. Specify the module which XML_ID belong to with 'module.xml_id'
*/
OLS05002, DiagnosticSetting::Error, "Unspecified module. Add the module name before the XML ID: 'module.xml_id'",
/**
* The given module is unknown
*/
OLS05003, DiagnosticSetting::Error, "Unknown module",
/**
* odoo, openerp and data nodes can not contain this attribute.
*/
OLS05004, DiagnosticSetting::Error, "Invalid attribute",
/**
* This tag is invalid
*/
OLS05005, DiagnosticSetting::Error, "Invalid node tag",
/**
*/
OLS05006, DiagnosticSetting::Error, "menuitem node must contains an id attribute",
/**
* This attribute is not valid in a menuitem node
*/
OLS05007, DiagnosticSetting::Error, "Invalid attribute {0} in menuitem node",
/**
*/
OLS05008, DiagnosticSetting::Error, "Sequence attribute must be a string representing an integer",
/**
*/
OLS05009, DiagnosticSetting::Error, "SubmenuItem is not allowed when action and parent attributes are defined on a menuitem",
/**
*/
OLS05010, DiagnosticSetting::Error, "web_icon attribute is not allowed when parent is specified",
/**
*/
OLS05011, DiagnosticSetting::Error, "Invalid child node {0} in menuitem",
/**
*/
OLS05012, DiagnosticSetting::Error, "parent attribute is not allowed in submenuitems",
/**
*/
OLS05013, DiagnosticSetting::Error, "Invalid attribute {0} in record node",
/**
* A <record> node in XML must have a 'model' attribute.
*/
OLS05014, DiagnosticSetting::Error, "record node must contain a model attribute",
/**
* Only <field> nodes are allowed as children of <record>.
*/
OLS05015, DiagnosticSetting::Error, "Invalid child node {0} in record. Only field node is allowed",
/**
* A <field> node in XML must have a 'name' attribute.
*/
OLS05016, DiagnosticSetting::Error, "field node must contain a name attribute",
/**
* A <field> node cannot have more than one of the following attributes: type, ref, eval, search.
*/
OLS05017, DiagnosticSetting::Error, "field node cannot have more than one of the attributes type, ref, eval or search",
/**
* The content of a <field type="int"> must be a valid integer or 'None'.
*/
OLS05018, DiagnosticSetting::Error, "Invalid content for int field: {0}",
/**
* The content of a <field type="float"> must be a valid float.
*/
OLS05019, DiagnosticSetting::Error, "Invalid content for float field: {0}",
/**
* Only valid child nodes are allowed in <field type="list|tuple">.
*/
OLS05020, DiagnosticSetting::Error, "Invalid child node {0} in list/tuple field",
/**
* <field> or <value> nodes with a 'file' attribute must not have text content.
*/
OLS05021, DiagnosticSetting::Error, "text content is not allowed on a value that contains a file attribute",
/**
* <field> nodes with 'ref', 'eval', or 'search' attributes must not have text content.
*/
OLS05022, DiagnosticSetting::Error, "text content is not allowed on a field with {0} attribute",
/**
* The 'model' attribute is only allowed on <field> nodes with 'eval' or 'search'.
*/
OLS05023, DiagnosticSetting::Error, "model attribute is not allowed on field node without eval or search attribute",
/**
* The 'use' attribute is only allowed on <field> nodes with 'search'.
*/
OLS05024, DiagnosticSetting::Error, "use attribute is only allowed on field node with search attribute",
/**
* The attribute is not valid for <field> nodes.
*/
OLS05025, DiagnosticSetting::Error, "Invalid attribute {0} in field node",
/**
* Only <record> nodes are allowed as children of <field> (except for xml/html fields).
*/
OLS05026, DiagnosticSetting::Error, "Fields only allow 'record' children nodes",
/**
* The 'search' attribute cannot be used together with 'eval', 'type', `file`, or text data on a <value> node.
*/
OLS05027, DiagnosticSetting::Error, "search attribute is not allowed when eval, type, file, or text content is present",
/**
* The 'eval' attribute cannot be used together with 'search', 'type', `file`, or text data on a <value> node.
*/
OLS05028, DiagnosticSetting::Error, "eval attribute is not allowed when search, type, file, or text content is present",
/**
* The 'type' attribute cannot be used together with 'search' or 'eval' on a <value> node.
*/
OLS05029, DiagnosticSetting::Error, "type attribute is not allowed when search or eval attribute is present",
/**
* <value> nodes with a 'file' attribute must not have text content.
*/
OLS05030, DiagnosticSetting::Error, "text content is not allowed on a value that contains a file attribute",
/**
* The 'file' attribute cannot be used together with 'search' or 'eval' on a <value> node.
*/
OLS05031, DiagnosticSetting::Error, "file attribute is not allowed when search or eval attribute is present",
/**
* The attribute is not valid for <value> nodes.
*/
OLS05032, DiagnosticSetting::Error, "Invalid attribute {0} in value node",
/**
* A <delete> node in XML must have a 'model' attribute.
*/
OLS05033, DiagnosticSetting::Error, "delete node must contain a model attribute",
/**
* A <delete> node cannot have both 'id' and 'search' attributes at the same time.
*/
OLS05034, DiagnosticSetting::Error, "delete node cannot have both id and search attributes",
/**
* A <delete> node must have either an 'id' or a 'search' attribute.
*/
OLS05035, DiagnosticSetting::Error, "delete node must have either id or search attribute",
/**
* Empty Value data, text data or file attribute has to be provided on a <value> node.
*/
OLS05036, DiagnosticSetting::Error, "Empty Value data, text data or file attribute has to be provided when `type` attribute is present",
/**
* Empty Value data, one of text data, `file`, `eval`, or `search` has to be provided.
*/
OLS05037, DiagnosticSetting::Error, "Empty Value data, one of text data, `file`, `eval`, or `search` has to be provided",
/**
* Empty Function data, either of `eval` attribute , or one or more `value`, or `function` children have to be provided.
*/
OLS05038, DiagnosticSetting::Error, "Empty Function data, either of `eval` attribute , or one or more `value`, or `function` children have to be provided",
/**
* You provided an empty XML ID. Please provide a valid XML ID.
*/
OLS05039, DiagnosticSetting::Error, "Empty XML ID. Please provide a valid XML ID.",
/**
* The 'binding_views' attribute must match the required pattern.
*/
OLS05040, DiagnosticSetting::Error, "binding_views attribute must be a comma-separated list of view types matching ^([a-z]+(,[a-z]+)*)?$, found {0}",
/**
* A <report> node must have the specified attribute (string, model, or name).
*/
OLS05041, DiagnosticSetting::Error, "report node must contain a {0} attribute",
/**
* The attribute is not valid for <report> nodes.
*/
OLS05042, DiagnosticSetting::Error, "Invalid attribute {0} in report node",
/**
* <report> nodes cannot have text content.
*/
OLS05043, DiagnosticSetting::Error, "report node cannot have text content",
/**
* A <function> node must have the specified attribute (model or name).
*/
OLS05044, DiagnosticSetting::Error, "function node must contain a {0} attribute",
/**
* <function> nodes cannot have <value> children when 'eval' attribute is present.
*/
OLS05045, DiagnosticSetting::Error, "function node cannot have value children when eval attribute is present",
/**
* The attribute is not valid for <function> nodes.
*/
OLS05046, DiagnosticSetting::Error, "Invalid attribute {0} in function node",
/**
* <function> nodes cannot have <function> children when 'eval' attribute is present.
*/
OLS05047, DiagnosticSetting::Error, "function node cannot have function children when eval attribute is present",
/**
* Only valid child nodes are allowed in <function> nodes.
*/
OLS05048, DiagnosticSetting::Error, "Invalid child node {0} in function node",
/**
*/
OLS05049, DiagnosticSetting::Error, "Data file {0} not found in the module",
/**
*/
OLS05050, DiagnosticSetting::Error, "Data file {0} is not a valid XML or CSV file",
/**
* An XML_ID should be in the format 'xml_id' or 'module.xml_id', but can't contains more dots
*/
OLS05051, DiagnosticSetting::Error, "Invalid XML ID '{0}'. It should not contain more than one dot.",
/**
* The given parent_id does not exists in the dependents modules, or is not a menuitem
*/
OLS05052, DiagnosticSetting::Error, "Parent menuitem with id '{0}' does not exist",
/**
 * A menuitem is specifying an action that has not been declared before the menuitem.
 */
OLS05053, DiagnosticSetting::Error, "Action with id '{0}' does not exist",
/**
 * A menuitem is specifying a group that has not been declared before the menuitem
 */
OLS05054, DiagnosticSetting::Error, "Group(s) with id(s) '{0}' does not exist",
/**
 * Model not found in current module or its dependencies
 */
OLS05055, DiagnosticSetting::Error, "Model '{0}' not found in module '{1}' or its dependencies",
/**
 * Model not found
 */
OLS05056, DiagnosticSetting::Error, "Model '{0}' not found",
/**
 * Field not found in model
 */
OLS05057, DiagnosticSetting::Error, "Field '{0}' not found in model '{1}'",
}
