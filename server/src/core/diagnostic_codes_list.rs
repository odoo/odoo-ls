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
    /** "{0} not found".
    * The symbol you are trying to import was not found.
    * Check your python environment, the effective your sys.path and your addon paths.
    */
    OLS02001, (DiagnosticSetting::Warning, "{0} not found"),
    /** "{0} not found".
    * The symbol you used as a base class can not be resolved.
    * Be sure that the symbol is referring to a valid python class.
    */
    OLS01001, (DiagnosticSetting::Warning, "{0} not found"),
    /** "{0} not found".
    * The symbol you used as a base class is not a class, or not evaluated to a class.
    * Be sure that the symbol is referring to a valid python class.
    */
    OLS01002, (DiagnosticSetting::Warning, "{0} not found"),
    /** "Failed to evaluate {0}".
    * The extension failed to evaluate a symbol. This occurs more specifically when the extension detect a loop in the imports.
    * If your code is working fine, it can happen if you use too many "import *" that can break the extension flow for now.
    */
    OLS02002, (DiagnosticSetting::Warning, "Failed to evaluate {0}"),
    /** "Multiple definition found for base class".
    * The extension is unable to handle a base class that has multiple possible definitions. This Warning should disappear in the future
    */
    OLS01003, (DiagnosticSetting::Warning, "Multiple definition found for base class"),
    /** "Deprecation Warning: Since 17.0: odoo.tests.common.Form is deprecated, use odoo.tests.Form".
    * Form is no longer available on odoo.tests.common, thus it should not be imported from there.
    */
    OLS03301, (DiagnosticSetting::Warning, "Deprecation Warning: Since 17.0: odoo.tests.common.Form is deprecated, use odoo.tests.Form"),
    /** "The active key is deprecated".
    * Deprecation Warning
    */
    OLS03302, (DiagnosticSetting::Warning, "The active key is deprecated"),
    /** Unable to parse file. Ruff_python_parser was unable to parse the file content.
    * See the error message to get the details from Ruff
    */
    OLS01000, (DiagnosticSetting::Error, "Unable to parse file. Ruff_python_parser was unable to parse the file content. See the error message to get the details from Ruff"),
    /** "Non-static method should have at least one parameter"
    */
    OLS01004, (DiagnosticSetting::Error, "Non-static method should have at least one parameter"),
    /** "This model is not in the dependencies of your module."
    * With the Environment (often via self.env), or in @api.returns, you are trying to get a recordset of a model that is not defined in the current module or in the dependencies of the current module.
    * Even if it could work, this is strongly not recommended, as the model you are referring to could be not available on a live database.
    * Do not forget that even if your model is in an auto-installed module, it can be uninstalled by a user.
    */
    OLS03001, (DiagnosticSetting::Error, "This model is not in the dependencies of your module."),
    /** "Unknown model. Check your addons path"
    * With the Environment (often via self.env), or in @api.returns, you are trying to get a recordset of a model that is unknown by OdooLS.
    * It means that if the model exists in the codebase, OdooLS is not aware of it.
    * Check the addons path you provided to be sure that the module declaring this model is in an addon path.
    */
    OLS03002, (DiagnosticSetting::Error, "Unknown model. Check your addons path"),
    /** "{0} is not in the dependencies of the module"
    * The symbol you are importing is in a module that is not in the dependencies of the current module.
    * You should check the dependencies in the __manifest__.py file of your module.
    */
    OLS03003, (DiagnosticSetting::Error, "{0} is not in the dependencies of the module"),
    /** "Model is inheriting from a model not declared in the dependencies of the module. Check the manifest."
    * The declared model is specifying an inheritance to a model that is not declared in the visible modules by the current one.
    * Consider updating the manifest of your module to include the relevant module.
    */
    OLS03004, (DiagnosticSetting::Error, "Model is inheriting from a model not declared in the dependencies of the module. Check the manifest."),
    /** "This model is inherited, but never declared."
    * The extension found some classes inheriting this model, but didn't find any class that declare it first, with only a _name.
    */
    OLS03005, (DiagnosticSetting::Error, "This model is inherited, but never declared."),
    /** "A manifest should contain exactly one dictionary".
    * A __manifest__.py file should be evaluated with a literal_eval to a single dictionary.
    * Do not store any other information in it.
    */
    OLS04001, (DiagnosticSetting::Error, "A manifest should contain exactly one dictionary"),
    /** "A manifest should not have duplicate keys".
    * A __manifest__.py dictionary should have at most one definition per key
    */
    OLS04002, (DiagnosticSetting::Error, "A manifest should not have duplicate keys"),
    /** "The name of the module should be a string".
    * The name key on the __manifest__.py should be a string
    */
    OLS04003, (DiagnosticSetting::Error, "The name of the module should be a string"),
    /** "The depends value should be a list".
    * "depends" value in module manifest should be a list
    */
    OLS04004, (DiagnosticSetting::Error, "The depends value should be a list"),
    /** "The depends key should be a list of strings".
    * Values in the manifest's "depends" list should be strings
    */
    OLS04005, (DiagnosticSetting::Error, "The depends key should be a list of strings"),
    /** "A module cannot depends on itself".
    * A module cannot have its own name as a dependency in its manifest
    */
    OLS04006, (DiagnosticSetting::Error, "A module cannot depends on itself"),
    /** "The data value should be a list".
    * "data" value in module manifest should be a list
    */
    OLS04007, (DiagnosticSetting::Error, "The data value should be a list"),
    /** "The data key should be a list of strings".
    * Values in the manifest's "data" list should be strings
    */
    OLS04008, (DiagnosticSetting::Error, "The data key should be a list of strings"),
    /** "Manifest keys should be strings".
    * Keys of the dictionary in manifest files have to be string literals
    */
    OLS04009, (DiagnosticSetting::Error, "Manifest keys should be strings"),
    /** "Module {0} depends on {1} which is not found. Please review your addons paths".
    * Module has dependency on a dependency that is either wrong or does not exist.
    * Check that module folder exists, and it contains __init__.py and __manifest__.py
    */
    OLS04010, (DiagnosticSetting::Error, "Module {0} depends on {1} which is not found. Please review your addons paths"),
    /** "Do not use dict unpacking to build your manifest".
    * Dict unpacking should be avoided. Do not create a dictionary with values that must be unpacked like in {"a":1, **d}
    */
    OLS04011, (DiagnosticSetting::Error, "Do not use dict unpacking to build your manifest"),
    /** "First Argument to super must be a class"
    */
    OLS01005, (DiagnosticSetting::Error, "First Argument to super must be a class"),
    /** "Super calls outside a class scope must have at least one argument"
    */
    OLS01006, (DiagnosticSetting::Error, "Super calls outside a class scope must have at least one argument"),
    /** "Domains should be a list of tuples".
    * The provided domain is not a list of tuples. A domain should be in the form [("field", "operator", "value")]
    */
    OLS03006, (DiagnosticSetting::Error, "Domains should be a list of tuples"),
    /** "Domain tuple should have 3 elements".
    * Tuples in a domain should contains 3 elements: ("field", "operator", "value")
    */
    OLS03007, (DiagnosticSetting::Error, "Domain tuple should have 3 elements"),
    /** "{0} takes {1} positional arguments but {2} was given".
    * Number of positional arguments given as parameter to the function is wrong.
    */
    OLS01007, (DiagnosticSetting::Error, "{0} takes {1} positional arguments but {2} was given"),
    /** "{0} got an unexpected keyword argument '{1}'".
    * You gave a named parameter that is not present in the function definition.
    */
    OLS01008, (DiagnosticSetting::Error, "{0} got an unexpected keyword argument '{1}'"),
    /** "A String value in search domain tuple should be '&', '|' or '!'".
    * For an string that represents an operator in a search domain, the only valid values are '&', '|' or '!'
    */
    OLS03008, (DiagnosticSetting::Error, "A String value in search domain tuple should be '&', '|' or '!'"),
    /** "Invalid comparison operator".
    * Comparison operators in search domain tuples should be of one of these values:
    * "=", "!=", ">", ">=", "<", "<=", "=?", "=like", "like", "not like", "ilike", "not ilike", "=ilike", "in", "not in", "child_of", "parent_of", "any", "not any"
    */
    OLS03009, (DiagnosticSetting::Error, "Invalid comparison operator"),
    /** "Missing tuple after a search domain operator".
    * If you use a search domain operator (&, ! or |), they should be followed by tuples or lists.
    */
    OLS03010, (DiagnosticSetting::Error, "Missing tuple after a search domain operator"),
    /** "Invalid search domain field: {0} is not a member of {1}".
    * In a search domain, the first element of a tuple must be a member of the model, or of any model in a relation if expression contains "." (see documentation)
    */
    OLS03011, (DiagnosticSetting::Error, "Invalid search domain field: {0} is not a member of {1}"),
    /** "Invalid search domain field: Unknown date granularity".
    * In a search domain, when using a dot separator on a Date field, you can use the following granularities to access part of the date:
    * "year_number", "quarter_number", "month_number", "iso_week_number", "day_of_week", "day_of_month", "day_of_year", "hour_number", "minute_number", "second_number"
    */
    OLS03012, (DiagnosticSetting::Error, "Invalid search domain field: Unknown date granularity"),
    /** "Invalid search domain field: Invalid dot notation".
    * In a search domain, when using a dot separator, it should be used either on a Date or Relational field.
    * If you used a relational field and get this error, check that the comodel of this field is valid.
    */
    OLS03013, (DiagnosticSetting::Error, "Invalid search domain field: Invalid dot notation"),
    /** "Field does not exist on model or not in dependencies".
    * In related keyword argument or decorators api.onchange/depends/constrains, the field provided
    * should exist and be able to be resolved from current module
    */
    OLS03014, (DiagnosticSetting::Error, "Field does not exist on model or not in dependencies"),
    /** "Field comodel_name's value is not in dependencies".
    * In relational fields, comodel_name's value supplied exists but is not in dependencies
    */
    OLS03015, (DiagnosticSetting::Error, "Field comodel_name's value is not in dependencies"),
    /** "Field comodel_name's value is does not exist".
    * In relational fields, comodel_name's value is does not exist in current configuration
    */
    OLS03016, (DiagnosticSetting::Error, "Field comodel_name's values is does not exist"),
    /** "Related field is not of the same type".
    * Type of references field in related keyword argument does not match the current field
    */
    OLS03017, (DiagnosticSetting::Error, "Related field is not of the same type"),
    /** "Method does not exist on current model".
    * For compute, search, inverse arguments, this error is shown when the method is not found on the current model
    */
    OLS03018, (DiagnosticSetting::Error, "Method does not exist on current model"),
    /** "Compute method not set to modify this field".
    * The compute method is set to modify a certain field(s).
    * Consider marking the modified field with the compute method
    */
    OLS03019, (DiagnosticSetting::Error, "Compute method not set to modify this field"),
    /** "Unknown XML ID".
    * The XML ID you referenced has not been found in any XML in this module or its dependencies
    */
    OLS05001, (DiagnosticSetting::Error, "Unknown XML ID"),
    /** "Unspecified module. Add the module name before the XML ID: 'module.xml_id'".
    * You provided an XML ID that has no module specified. Specify the module which XML_ID belong to with 'module.xml_id'
    */
    OLS05002, (DiagnosticSetting::Error, "Unspecified module. Add the module name before the XML ID: 'module.xml_id'"),
    /** "Unknown module".
    * The given module is unknown
    */
    OLS05003, (DiagnosticSetting::Error, "Unknown module"),
    /** "Invalid attribute".
    * odoo, openerp and data nodes can not contain this attribute.
    */
    OLS05004, (DiagnosticSetting::Error, "Invalid attribute"),
    /** "Invalid node tag".
    * This tag is invalid
    */
    OLS05005, (DiagnosticSetting::Error, "Invalid node tag"),
    /** "menuitem node must contains an id attribute"
    */
    OLS05006, (DiagnosticSetting::Error, "menuitem node must contains an id attribute"),
    /** "Invalid attribute {0} in menuitem node".
    * This attribute is not valid in a menuitem node
    */
    OLS05007, (DiagnosticSetting::Error, "Invalid attribute {0} in menuitem node"),
    /** "Sequence attribute must be a string representing a number"
    */
    OLS05008, (DiagnosticSetting::Error, "Sequence attribute must be a string representing a number"),
    /** "SubmenuItem is not allowed when action and parent attributes are defined on a menuitem"
    */
    OLS05009, (DiagnosticSetting::Error, "SubmenuItem is not allowed when action and parent attributes are defined on a menuitem"),
    /** "web_icon attribute is not allowed when parent is specified"
    */
    OLS05010, (DiagnosticSetting::Error, "web_icon attribute is not allowed when parent is specified"),
    /** "Invalid child node {0} in menuitem"
    */
    OLS05011, (DiagnosticSetting::Error, "Invalid child node {0} in menuitem"),
    /** "parent attribute is not allowed in submenuitems"
    */
    OLS05012, (DiagnosticSetting::Error, "parent attribute is not allowed in submenuitems"),
    /** "Invalid attribute {0} in record node"
    */
    OLS05013, (DiagnosticSetting::Error, "Invalid attribute {0} in record node"),
    /** "record node must contain a model attribute"
    * A <record> node in XML must have a 'model' attribute.
    */
    OLS05014, (DiagnosticSetting::Error, "record node must contain a model attribute"),
    /** "Invalid child node {0} in record. Only field node is allowed"
    * Only <field> nodes are allowed as children of <record>.
    */
    OLS05015, (DiagnosticSetting::Error, "Invalid child node {0} in record. Only field node is allowed"),
    /** "field node must contain a name attribute"
    * A <field> node in XML must have a 'name' attribute.
    */
    OLS05016, (DiagnosticSetting::Error, "field node must contain a name attribute"),
    /** "field node cannot have more than one of the attributes type, ref, eval or search"
    * A <field> node cannot have more than one of the following attributes: type, ref, eval, search.
    */
    OLS05017, (DiagnosticSetting::Error, "field node cannot have more than one of the attributes type, ref, eval or search"),
    /** "Invalid content for int field: {0}"
    * The content of a <field type="int"> must be a valid integer or 'None'.
    */
    OLS05018, (DiagnosticSetting::Error, "Invalid content for int field: {0}"),
    /** "Invalid content for float field: {0}"
    * The content of a <field type="float"> must be a valid float.
    */
    OLS05019, (DiagnosticSetting::Error, "Invalid content for float field: {0}"),
    /** "Invalid child node {0} in list/tuple field"
    * Only valid child nodes are allowed in <field type="list|tuple">.
    */
    OLS05020, (DiagnosticSetting::Error, "Invalid child node {0} in list/tuple field"),
    /** "text content is not allowed on a value that contains a file attribute"
    * <field> or <value> nodes with a 'file' attribute must not have text content.
    */
    OLS05021, (DiagnosticSetting::Error, "text content is not allowed on a value that contains a file attribute"),
    /** "text content is not allowed on a field with {0} attribute"
    * <field> nodes with 'ref', 'eval', or 'search' attributes must not have text content.
    */
    OLS05022, (DiagnosticSetting::Error, "text content is not allowed on a field with {0} attribute"),
    /** "model attribute is not allowed on field node without eval or search attribute"
    * The 'model' attribute is only allowed on <field> nodes with 'eval' or 'search'.
    */
    OLS05023, (DiagnosticSetting::Error, "model attribute is not allowed on field node without eval or search attribute"),
    /** "use attribute is only allowed on field node with search attribute"
    * The 'use' attribute is only allowed on <field> nodes with 'search'.
    */
    OLS05024, (DiagnosticSetting::Error, "use attribute is only allowed on field node with search attribute"),
    /** "Invalid attribute {0} in field node"
    * The attribute is not valid for <field> nodes.
    */
    OLS05025, (DiagnosticSetting::Error, "Invalid attribute {0} in field node"),
    /** "Fields only allow 'record' children nodes"
    * Only <record> nodes are allowed as children of <field> (except for xml/html fields).
    */
    OLS05026, (DiagnosticSetting::Error, "Fields only allow 'record' children nodes"),
    /** "search attribute is not allowed when eval or type attribute is present"
    * The 'search' attribute cannot be used together with 'eval' or 'type' on a <value> node.
    */
    OLS05027, (DiagnosticSetting::Error, "search attribute is not allowed when eval or type attribute is present"),
    /** "eval attribute is not allowed when search or type attribute is present"
    * The 'eval' attribute cannot be used together with 'search' or 'type' on a <value> node.
    */
    OLS05028, (DiagnosticSetting::Error, "eval attribute is not allowed when search or type attribute is present"),
    /** "type attribute is not allowed when search or eval attribute is present"
    * The 'type' attribute cannot be used together with 'search' or 'eval' on a <value> node.
    */
    OLS05029, (DiagnosticSetting::Error, "type attribute is not allowed when search or eval attribute is present"),
    /** "text content is not allowed on a value that contains a file attribute"
    * <value> nodes with a 'file' attribute must not have text content.
    */
    OLS05030, (DiagnosticSetting::Error, "text content is not allowed on a value that contains a file attribute"),
    /** "file attribute is only allowed on value node with type attribute"
    * The 'file' attribute is only allowed on <value> nodes with a 'type' attribute.
    */
    OLS05031, (DiagnosticSetting::Error, "file attribute is only allowed on value node with type attribute"),
    /** "Invalid attribute {0} in value node"
    * The attribute is not valid for <value> nodes.
    */
    OLS05032, (DiagnosticSetting::Error, "Invalid attribute {0} in value node"),
    /** "delete node must contain a model attribute"
    * A <delete> node in XML must have a 'model' attribute.
    */
    OLS05033, (DiagnosticSetting::Error, "delete node must contain a model attribute"),
    /** "delete node cannot have both id and search attributes"
    * A <delete> node cannot have both 'id' and 'search' attributes at the same time.
    */
    OLS05034, (DiagnosticSetting::Error, "delete node cannot have both id and search attributes"),
    /** "delete node must have either id or search attribute"
    * A <delete> node must have either an 'id' or a 'search' attribute.
    */
    OLS05035, (DiagnosticSetting::Error, "delete node must have either id or search attribute"),
    /** "act_window node must contain a {0} attribute"
    * An <act_window> node must have the specified attribute (id, name, or res_model).
    */
    OLS05036, (DiagnosticSetting::Error, "act_window node must contain a {0} attribute"),
    /** "Invalid attribute {0} in act_window node"
    * The attribute is not valid for <act_window> nodes.
    */
    OLS05037, (DiagnosticSetting::Error, "Invalid attribute {0} in act_window node"),
    /** "act_window node cannot have text content"
    * <act_window> nodes cannot have text content.
    */
    OLS05038, (DiagnosticSetting::Error, "act_window node cannot have text content"),
    /** "binding_type attribute must be either 'action' or 'report', found {0}"
    * The 'binding_type' attribute must be either 'action' or 'report'.
    */
    OLS05039, (DiagnosticSetting::Error, "binding_type attribute must be either 'action' or 'report', found {0}"),
    /** "binding_views attribute must be a comma-separated list of view types matching ^([a-z]+(,[a-z]+)*)?$, found {0}"
    * The 'binding_views' attribute must match the required pattern.
    */
    OLS05040, (DiagnosticSetting::Error, "binding_views attribute must be a comma-separated list of view types matching ^([a-z]+(,[a-z]+)*)?$, found {0}"),
    /** "report node must contain a {0} attribute"
    * A <report> node must have the specified attribute (string, model, or name).
    */
    OLS05041, (DiagnosticSetting::Error, "report node must contain a {0} attribute"),
    /** "Invalid attribute {0} in report node"
    * The attribute is not valid for <report> nodes.
    */
    OLS05042, (DiagnosticSetting::Error, "Invalid attribute {0} in report node"),
    /** "report node cannot have text content"
    * <report> nodes cannot have text content.
    */
    OLS05043, (DiagnosticSetting::Error, "report node cannot have text content"),
    /** "function node must contain a {0} attribute"
    * A <function> node must have the specified attribute (model or name).
    */
    OLS05044, (DiagnosticSetting::Error, "function node must contain a {0} attribute"),
    /** "function node cannot have value children when eval attribute is present"
    * <function> nodes cannot have <value> children when 'eval' attribute is present.
    */
    OLS05045, (DiagnosticSetting::Error, "function node cannot have value children when eval attribute is present"),
    /** "Invalid attribute {0} in function node"
    * The attribute is not valid for <function> nodes.
    */
    OLS05046, (DiagnosticSetting::Error, "Invalid attribute {0} in function node"),
    /** "function node cannot have function children when eval attribute is present"
    * <function> nodes cannot have <function> children when 'eval' attribute is present.
    */
    OLS05047, (DiagnosticSetting::Error, "function node cannot have function children when eval attribute is present"),
    /** "Invalid child node {0} in function node"
    * Only valid child nodes are allowed in <function> nodes.
    */
    OLS05048, (DiagnosticSetting::Error, "Invalid child node {0} in function node"),
    /** "Data file {0} not found in the module"
    */
    OLS05049, (DiagnosticSetting::Error, "Data file {0} not found in the module"),
    /** "Data file {0} is not a valid XML or CSV file"
    */
    OLS05050, (DiagnosticSetting::Error, "Data file {0} is not a valid XML or CSV file"),
    /** Invalid XML ID '{0}'. It should not contain more than one dot.
    * An XML_ID should be in the format 'xml_id' or 'module.xml_id', but can't contains more dots
    */
    OLS05051, (DiagnosticSetting::Error, "Invalid XML ID '{0}'. It should not contain more than one dot."),
}
