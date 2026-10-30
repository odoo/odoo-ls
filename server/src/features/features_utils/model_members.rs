use crate::constants::SymType;
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::symbol_keys::{ModelSymbolKey, ModuleKey, SymbolKey};
use crate::threads::SessionInfo;

/// The model class `scope` is written in, if that class is a model.
pub(super) fn enclosing_model(session: &SessionInfo, scope: SymbolKey) -> Option<ModelSymbolKey> {
    let Some(SymbolKey::Class(class)) = session.st().get_in_parents(scope, &[SymType::CLASS], true) else {
        return None;
    };
    session.st()[class]._model.is_some().then_some(class.into())
}

/// The fields named `name` of `model`, one per class defining it.
pub(super) fn find_fields(
    session: &mut SessionInfo,
    model: ModelSymbolKey,
    from_module: Option<ModuleKey>,
    name: &str,
) -> Vec<SymbolKey> {
    SymbolTable::get_member_symbol(session, model.into(), name, from_module, false, true, false, true, false).0
}

/// The methods named `name` of `model`, one per class defining it.
pub(super) fn find_methods(
    session: &mut SessionInfo,
    model: ModelSymbolKey,
    from_module: Option<ModuleKey>,
    name: &str,
) -> Vec<SymbolKey> {
    SymbolTable::get_member_symbol(session, model.into(), name, from_module, false, false, true, true, false).0
}
