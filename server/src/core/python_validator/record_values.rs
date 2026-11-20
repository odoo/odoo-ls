use crate::core::diagnostics::{DiagnosticCode, create_diagnostic};
use crate::core::file_mgr::FileMgr;
use crate::core::selection::MergedSelection;
use crate::core::symbols::SymbolTable;
use crate::core::symbols::symbol_keys::{ModelSymbolKey, ModuleKey};
use crate::features::features_utils::SpecialArg;
use crate::threads::SessionInfo;
use lsp_types::Diagnostic;
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

/// Validate the values given to `create`, `write` or `update` called on `model`, if it is known.
/// Each string key of a dict of values must be a field of the model.
/// Each string value of a Selection field must be one of its values.
pub(crate) fn validate_record_values_arg(
    session: &mut SessionInfo<'_>,
    special_arg: SpecialArg,
    model: Option<ModelSymbolKey>,
    from_module: Option<ModuleKey>,
    value: &Expr,
) -> Vec<Diagnostic> {
    let Some(model) = model else {
        return vec![];
    };
    let mut diagnostics = vec![];
    for dict in special_arg.values_dicts(value) {
        for item in dict.iter() {
            let Some(Expr::StringLiteral(field_name)) = &item.key else {
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
                continue;
            }
            // A string value of a Selection field must be one of its values, when they're all known
            if let Expr::StringLiteral(value) = &item.value
                && let Some(selection) = MergedSelection::from_model_field(
                    session,
                    model,
                    field_name.value.to_str(),
                    from_module,
                )
                && selection.complete
                && !selection.contains(value.value.to_str())
                && let Some(diagnostic_base) = create_diagnostic(
                    session,
                    DiagnosticCode::OLS03030,
                    &[value.value.to_str(), field_name.value.to_str()],
                )
            {
                diagnostics.push(Diagnostic {
                    range: FileMgr::textRange_to_temporary_Range(&value.range()),
                    ..diagnostic_base
                });
            }
        }
    }
    diagnostics
}
