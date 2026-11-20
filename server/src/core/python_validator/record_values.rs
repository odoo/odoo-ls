use crate::core::diagnostics::{DiagnosticCode, create_diagnostic};
use crate::core::file_mgr::FileMgr;
use crate::core::symbols::SymbolTable;
use crate::core::symbols::symbol_keys::{ModelSymbolKey, ModuleKey};
use crate::features::features_utils::SpecialArg;
use crate::threads::SessionInfo;
use lsp_types::Diagnostic;
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

/// Validate the field *key/name* given to `create`, `write` or `update` called on `model`.
/// Each string key of a dict of values must be a field of the model.
pub(crate) fn validate_record_values_arg(
    session: &mut SessionInfo<'_>,
    special_arg: SpecialArg,
    model: ModelSymbolKey,
    from_module: Option<ModuleKey>,
    value: &Expr,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    for dict in special_arg.values_dicts(value) {
        for key in dict.iter_keys().flatten() {
            let Expr::StringLiteral(field_name) = key else {
                continue;
            };

            let is_empty = SymbolTable::get_member_symbol(
                session,
                model.into(),
                field_name.value.to_str(),
                from_module,
                false,
                true,
                false,
                false,
                false,
            )
            .0
            .is_empty();
            if is_empty
                && let Some(diagnostic_base) =
                    create_diagnostic(session, DiagnosticCode::OLS03014, &[])
            {
                diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&field_name.range()),
                    ..diagnostic_base
                });
            }
        }
    }
    diagnostics
}
