use ruff_python_ast::{Expr, ExprStringLiteral};

use crate::core::symbols::storage::{FileContentParent, SymbolTable};
use crate::core::symbols::symbol_keys::{ClassKey, SymbolKey, VariableKey};
use crate::odoo_version::OdooVersion;
use crate::threads::SessionInfo;

use super::field_params::FieldParam;
use FieldParam::{Comodel, Depends, InverseField, ModelMethod, Related};
use SpecialArg::{Domain, Field, Order};

/// A call argument with a special meaning in odoo context, e.g. a domain or a field parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialArg {
    /// A domain on the model the method is called on
    Domain,
    /// An order on the model the method is called on, e.g. `search(order='name desc')`
    Order,
    /// A parameter of a field class, e.g. `fields.Char(related='partner_id.name')`
    Field(FieldParam),
}

impl SpecialArg {
    /// Classifies the call argument as a special argument, based on the parameter and keyword name.
    pub fn classify_call_argument(
        session: &SessionInfo,
        callable_sym: SymbolKey,
        param: Option<VariableKey>,
        keyword_name: Option<&str>,
    ) -> Option<Self> {
        // A declared parameter belongs to the function declaring it, e.g. `Many2one.__init__`,
        // a keyword in `**kwargs` to the callable
        let (name, owner) = match param {
            Some(param) => {
                let FileContentParent::Function(function) = session.st()[param].parent() else {
                    return None;
                };
                (session.st()[param].name.as_str(), function.into())
            }
            None => (keyword_name?, callable_sym),
        };
        special_params_named(session.sync_odoo.version, name)
            .find(|special_param| special_param.owner.matches(session, owner))
            .map(|special_param| special_param.kind)
    }

    /// Whether `string` is in the correct expected position in the `value` expression.
    pub(super) fn is_in_value(self, value: &Expr, string: &ExprStringLiteral) -> bool {
        match self {
            // Any string inside the domain, e.g. the field path of a leaf
            SpecialArg::Domain => true,
            // The order is the string itself, not an item of it
            SpecialArg::Order => {
                matches!(value, Expr::StringLiteral(value_string) if value_string.range == string.range)
            }
            SpecialArg::Field(field_param) => field_param.is_in_value(value, string),
        }
    }

    /// Cheap check if the given `name` can be a keyword argument with a special value in odoo context.
    pub(super) fn can_be_special_keyword(
        session: &SessionInfo,
        name: &str,
        value: &Expr,
        string: &ExprStringLiteral,
    ) -> bool {
        special_params_named(session.sync_odoo.version, name)
            .any(|special_param| special_param.kind.is_in_value(value, string))
    }
}

/// The relational field classes, taking a comodel.
const RELATIONAL_FIELDS: &[&str] = &["Many2one", "One2many", "Many2many"];

/// The search methods taking an order (`_name_search` up to 17.x).
const SEARCH_METHODS: &[&str] = &[
    "search",
    "search_fetch",
    "search_read",
    "_search",
    "_name_search",
];

/// The special parameters, each with its owner, as the same name can mean different things.
const SPECIAL_PARAMS: &[SpecialParam] = &[
    model_methods(&["search"], "domain", Domain),
    model_methods(SEARCH_METHODS, "order", Order),
    web_base_methods(&["web_search_read"], "order", Order),
    field_classes(RELATIONAL_FIELDS, "comodel_name", Field(Comodel)),
    field_classes(&["One2many"], "inverse_name", Field(InverseField)),
    field_classes(&["Selection", "Reference"], "selection", Field(ModelMethod)),
    any_field_class("related", Field(Related)),
    any_field_class("depends", Field(Depends)),
    any_field_class("compute", Field(ModelMethod)),
    any_field_class("group_expand", Field(ModelMethod)),
    any_field_class("inverse", Field(ModelMethod)),
    any_field_class("search", Field(ModelMethod)),
    any_field_class("compute_sql", Field(ModelMethod)).since(19, 1),
    any_field_class("init_storage", Field(ModelMethod)).since(20, 0),
];

/// Returns the special parameters named `name` in `version`, whatever their owner.
pub(super) fn special_params_named(
    version: OdooVersion,
    name: &str,
) -> impl Iterator<Item = &'static SpecialParam> {
    SPECIAL_PARAMS
        .iter()
        .filter(move |special_param| special_param.name == name && version >= special_param.since)
}

/// A parameter with a special meaning in odoo context, for its owner only.
/// e.g. the `domain` of `BaseModel.search`, not of an override or of another method.
pub(super) struct SpecialParam {
    owner: ParamOwner,
    name: &'static str,
    /// The first Odoo version taking it
    since: (u32, u32),
    pub(super) kind: SpecialArg,
}

impl SpecialParam {
    const fn new(owner: ParamOwner, name: &'static str, kind: SpecialArg) -> Self {
        SpecialParam {
            owner,
            name,
            since: (0, 0),
            kind,
        }
    }

    const fn since(self, major: u32, minor: u32) -> Self {
        SpecialParam {
            since: (major, minor),
            ..self
        }
    }
}

/// Returns the special parameter `name` of the `BaseModel` methods in `methods`.
const fn model_methods(
    methods: &'static [&'static str],
    name: &'static str,
    kind: SpecialArg,
) -> SpecialParam {
    SpecialParam::new(ParamOwner::ModelMethods(methods), name, kind)
}

/// Returns the special parameter `name` of the methods in `methods` that the `web` module adds to
/// all the models.
const fn web_base_methods(
    methods: &'static [&'static str],
    name: &'static str,
    kind: SpecialArg,
) -> SpecialParam {
    SpecialParam::new(ParamOwner::WebBaseMethods(methods), name, kind)
}

/// Returns the special parameter `name` of the field classes in `classes`.
const fn field_classes(
    classes: &'static [&'static str],
    name: &'static str,
    kind: SpecialArg,
) -> SpecialParam {
    SpecialParam::new(ParamOwner::FieldClasses(classes), name, kind)
}

/// Returns the special parameter `name` of all the field classes.
const fn any_field_class(name: &'static str, kind: SpecialArg) -> SpecialParam {
    SpecialParam::new(ParamOwner::AnyFieldClass, name, kind)
}

/// What owns a special parameter: the function declaring it, or the class taking it in `**kwargs`.
#[derive(Clone, Copy)]
enum ParamOwner {
    /// Methods of Odoo's `BaseModel`, not their overrides, e.g. `search`
    ModelMethods(&'static [&'static str]),
    /// Methods the `web` module adds to all the models (its `Base`), not their overrides,
    /// e.g. `web_search_read`
    WebBaseMethods(&'static [&'static str]),
    /// Some field classes, e.g. `Selection`
    FieldClasses(&'static [&'static str]),
    /// All the field classes, e.g. for the parameters in `**kwargs`
    AnyFieldClass,
}

impl ParamOwner {
    /// Whether the given `owner` matches this `ParamOwner`.
    fn matches(self, session: &SessionInfo, owner: SymbolKey) -> bool {
        match self {
            ParamOwner::ModelMethods(methods) => {
                let models: &[&str] = if session.sync_odoo.version >= (18, 1) {
                    &["odoo", "orm", "models"]
                } else {
                    &["odoo", "models"]
                };
                is_method_of(session, owner, methods, models, "BaseModel")
            }
            ParamOwner::WebBaseMethods(methods) => {
                let web_models: &[&str] = &["odoo", "addons", "web", "models", "models"];
                is_method_of(session, owner, methods, web_models, "Base")
            }
            ParamOwner::FieldClasses(classes) => owner_class(session, owner).is_some_and(|class| {
                SymbolTable::is_specific_field_class(session, class.into(), classes)
            }),
            ParamOwner::AnyFieldClass => owner_class(session, owner)
                .is_some_and(|class| SymbolTable::is_field_class(session, class.into())),
        }
    }
}

/// Whether `owner` is one of the `methods` of `class` in `module`, not an override.
fn is_method_of(
    session: &SessionInfo,
    owner: SymbolKey,
    methods: &[&str],
    module: &[&str],
    class: &str,
) -> bool {
    let SymbolKey::Function(function) = owner else {
        return false;
    };
    let method = session.st()[function].name.as_str();
    // The tree excludes the overrides
    methods.contains(&method)
        && session.sync_odoo.get_main_entry_tree(function) == (module, [class, method].as_slice())
}

/// Returns `owner` if it is a class, or its class if it is a constructor.
fn owner_class(session: &SessionInfo, owner: SymbolKey) -> Option<ClassKey> {
    match owner {
        SymbolKey::Class(class) => Some(class),
        SymbolKey::Function(function) if session.st()[function].name.as_str() == "__init__" => {
            match session.st()[function].parent() {
                FileContentParent::Class(class) => Some(class),
                _ => None,
            }
        }
        _ => None,
    }
}
