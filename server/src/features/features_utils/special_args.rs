use ruff_python_ast::{Expr, ExprStringLiteral};

use crate::core::symbols::storage::{FileContentParent, SymbolTable};
use crate::core::symbols::symbol_keys::{ClassKey, SymbolKey, VariableKey};
use crate::odoo_version::OdooVersion;
use crate::threads::SessionInfo;

use super::field_params::FieldParam;
use FieldParam::{Comodel, Depends, InverseField, ModelMethod, Related};
use SpecialArg::{Domain, Field};

/// A call argument with a special meaning in odoo context, e.g. a domain or a field parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialArg {
    /// A domain on the model the method is called on
    Domain,
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

/// The special parameters, each with its owner, as the same name can mean different things.
const SPECIAL_PARAMS: &[SpecialParam] = &[
    model_methods(&["search"], "domain", Domain),
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
                let SymbolKey::Function(function) = owner else {
                    return false;
                };
                let method = session.st()[function].name.as_str();
                let models: &[&str] = if session.sync_odoo.version >= (18, 1) {
                    &["odoo", "orm", "models"]
                } else {
                    &["odoo", "models"]
                };
                // The tree excludes the overrides
                methods.contains(&method)
                    && session.sync_odoo.get_main_entry_tree(function)
                        == (models, ["BaseModel", method].as_slice())
            }
            ParamOwner::FieldClasses(classes) => owner_class(session, owner).is_some_and(|class| {
                SymbolTable::is_specific_field_class(session, class.into(), classes)
            }),
            ParamOwner::AnyFieldClass => owner_class(session, owner)
                .is_some_and(|class| SymbolTable::is_field_class(session, class.into())),
        }
    }
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
