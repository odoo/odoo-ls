use ruff_python_ast::{Expr, ExprCall, ExprStringLiteral, Identifier, Keyword};
use ruff_text_size::{Ranged, TextRange};
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol_keys::{ModuleKey, SymbolKey};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::FunctionSymbol;
use crate::tree::OYarnExt;

use crate::constants::SymType;
use crate::constants::OYarn;
use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr};
use crate::core::evaluation_context::ContextKey;
use crate::threads::SessionInfo;
use crate::Sy;

use super::field_paths;
use super::string_resolution::SegmentPick;

fn follow_function_evaluation(
    session: &mut SessionInfo,
    scope: SymbolKey,
    call_expr: &ExprCall,
) -> Vec<EvaluationSymbolPtr> {
    let evaluations = Evaluation::eval_from_ast(session, &call_expr.func, scope, &call_expr.func.range().start(), false, &mut vec![]).0;
    let mut followed_evals = vec![];
    for eval in evaluations {
        followed_evals.extend(
            SymbolTable::follow_ref(
                &eval.symbol.get_symbol(session, None, &mut vec![], None),
                session,
                None,
                true,
                false,
                None,
                None,
            )
        );
    }
    followed_evals
}

pub(super) fn find_kwarg_methods_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
) -> Vec<(SymbolKey, TextRange)> {
    let Some(SymbolKey::Class(parent_class)) = session.st().get_in_parents(scope, &[SymType::CLASS], true) else {
        return vec![];
    };
    if session.st()[parent_class]._model.is_none(){
        return vec![];
    }
    let followed_evals = follow_function_evaluation(session, scope, call_expr);
    if !followed_evals.iter().any(|eval|
        eval.has_weak() && eval.get_weak().weak.upgrade(session.st()).map(|sym| SymbolTable::is_field_class(session, sym)).unwrap_or(false)
    ) {
        return vec![];
    }
    field_paths::find_simple_method_in_class(session, parent_class.into(), from_module, string.value.to_str())
        .into_iter()
                .map(|res| (res, string.range))
                .collect()
}

fn find_inverse_name_field_symbol(
    session: &mut SessionInfo,
    from_module: Option<ModuleKey>,
    field_value: &str,
    call_expr: &ExprCall,
) -> Vec<SymbolKey> {
    let model_name = if let Some(Expr::StringLiteral(expr)) = call_expr.arguments.args.first() {
        expr.value.to_str()
    } else {
        let Some(model_name) = call_expr.arguments.keywords.iter().find(|kw| kw.arg.as_ref().map(|arg| arg.id == "comodel_name").unwrap_or(false))
            .and_then(|kw| match &kw.value {
                Expr::StringLiteral(expr) => Some(expr.value.to_str()),
                _ => None
            }) else {
                return vec![];
            };
        model_name
    };
    let Some(model) = session.model_mgr().get_model_key(model_name) else {
        return vec![];
    };
    let main_syms = session.model_mgr()[model].get_main_symbols(session, from_module).collect::<Vec<_>>();
    main_syms.iter().flat_map(|&main_sym| field_paths::find_simple_field_in_class(session, main_sym, from_module, field_value)).collect()
}

fn find_simple_decorator_field_symbol(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    field_name: &str,
) -> Vec<SymbolKey> {
    let Some(SymbolKey::Class(parent_class)) = session.st().get_in_parents(scope, &[SymType::CLASS], true) else {
        return vec![];
    };
    if session.st()[parent_class]._model.is_none() {
        return vec![];
    }
    field_paths::find_simple_field_in_class(session, parent_class.into(), from_module, field_name)
}

fn find_positional_argument_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    arg_index: usize,
    arg_expr: &Expr,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    let mut arg_symbols: Vec<(SymbolKey, TextRange)> = vec![];
    let followed_evals = follow_function_evaluation(session, scope, call_expr);
    for callable_eval in followed_evals {
        let EvaluationSymbolPtr::WEAK(callable) = callable_eval else {
            continue;
        };
        let Some(callable_sym) = callable.weak.upgrade(session.st()) else {
            continue;
        };
        if ![SymType::FUNCTION, SymType::CLASS].contains(&callable_sym.typ()) {
            continue;
        }
        if let SymbolKey::Function(function_key) = callable_sym {
            if session.st()[function_key].is_property {
                continue;
            }
            // Check if we are in api.onchange/constrains/depends
            let func_sym_tree = session.st().get_tree(callable_sym);
            // TODO: account for change in tree after 18.1 odoo.orm.decorators
            let is_18_1_or_later = session.sync_odoo.version >= (18, 1);
            if (!is_18_1_or_later && func_sym_tree.0.ends_with_strs(&["odoo", "api"])) ||
                (is_18_1_or_later && func_sym_tree.0.ends_with_strs(&["odoo", "orm", "decorators"])) {
                if [vec![Sy!("onchange")], vec![Sy!("constrains")]].contains(&func_sym_tree.1) && SyncOdoo::is_in_main_entry(session, &func_sym_tree.0) {
                    arg_symbols.extend(
                        find_simple_decorator_field_symbol(session, scope, from_module, string.value.to_str())
                        .into_iter().map(|symbol| (symbol, string.range))
                    );
                    continue;
                } else if func_sym_tree.1 == ["depends"] && SyncOdoo::is_in_main_entry(session, &func_sym_tree.0){
                    arg_symbols.extend(
                        field_paths::find_nested_fields_in_class(session, scope, from_module, string, pick)
                    );
                    continue;
                }
            }
        }
        // if class get __init__ method, we need to get the argument from there
        let func_key = if callable_sym.typ() == SymType::CLASS {
            if let Some(&SymbolKey::Function(init_method)) = SymbolTable::get_member_symbol(session, callable_sym, "__init__", from_module, false, false, true, false, false).0.first() {
                init_method
            } else {
                continue;
            }
        } else {
            callable_sym.unwrap_function_key()
        };
        let is_on_instance = if callable_sym.typ() == SymType::CLASS {
            // skip self on __init__
            Some(true)
        } else {
            callable.context.get(ContextKey::IsAttrOfInstance).map(|v| v.as_bool())
        };

        let func_arg = FunctionSymbol::get_indexed_arg_in_call(
            session.st(),
            func_key,
            call_expr,
            arg_index as u32,
            is_on_instance);
        let Some(func_arg_sym) = func_arg.and_then(|func_arg| func_arg.symbol.upgrade(session.st())) else {
            continue
        };


        if session.st()[func_arg_sym].name == "inverse_name"
            && callable_sym.typ() == SymType::CLASS
            && SymbolTable::is_specific_field_class(session, callable_sym, &["One2many"]) {
            let Some(value) = Evaluation::expr_to_str(session, arg_expr, scope, &call_expr.range().end(), false, &mut vec![]).0 else {
                continue;
            };
            arg_symbols.extend(
                find_inverse_name_field_symbol(session, from_module, &value, call_expr)
                .into_iter().map(|res| (res, arg_expr.range()))
            );
            continue;
        }
        if session.st()[func_arg_sym].name == "selection"
            && callable_sym.typ() == SymType::CLASS
            && SymbolTable::is_field_class(session, callable_sym)
            && let Some(SymbolKey::Class(parent_class)) = session.st().get_in_parents(scope, &[SymType::CLASS], true) {
            arg_symbols.extend(
                field_paths::find_simple_method_in_class(session, parent_class.into(), from_module, string.value.to_str())
                .into_iter().map(|res| (res, arg_expr.range()))
            );
            continue;
        }
        for evaluation in session.st()[func_arg_sym].evaluations.clone() {
            if matches!(evaluation.symbol.get_symbol_ptr(), EvaluationSymbolPtr::DOMAIN) {
                arg_symbols.extend(
                    field_paths::find_domain_param_symbols(session, &callable, string, from_module, pick)
                );
            }
        }

    }
    //Process kwargs related/comodel_name
    arg_symbols
}

fn find_keyword_argument_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    keyword: &Keyword,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)> {
    // We only process the `related` and `depends` keyword arguments
    match keyword.arg.as_ref().map(Identifier::as_str) {
        Some("related") | Some("depends") => {},
        Some("inverse_name") => {
            return find_inverse_name_field_symbol(session, from_module, string.value.to_str(), call_expr)
                .into_iter()
                .map(|res| (res, string.range))
                .collect();
        },
        Some("compute" | "group_expand" | "inverse" | "search" | "selection") => {
            return find_kwarg_methods_symbols(session, scope, from_module, string, call_expr);
        },
        Some("compute_sql") if session.sync_odoo.version >= (19, 1) => {
            return find_kwarg_methods_symbols(session, scope, from_module, string, call_expr);
        },
        Some("init_storage") if session.sync_odoo.version >= (20, 0) => {
            return find_kwarg_methods_symbols(session, scope, from_module, string, call_expr);
        },
        _ => return vec![],
    }

    let mut arg_symbols = vec![];
    let followed_evals = follow_function_evaluation(session, scope, call_expr);

    for callable_eval in followed_evals {
        let EvaluationSymbolPtr::WEAK(callable) = callable_eval else {
            continue;
        };
        let Some(callable_sym) = callable.weak.upgrade(session.st()) else {
            continue;
        };
        if !SymbolTable::is_field_class(session, callable_sym) {
            continue;
        }
        arg_symbols.extend(
            field_paths::find_nested_fields_in_class(session, scope, from_module, string, pick)
        );
    }
    arg_symbols
}

pub(super) fn find_argument_symbols(
    session: &mut SessionInfo,
    scope: SymbolKey,
    from_module: Option<ModuleKey>,
    string: &ExprStringLiteral,
    call_expr: &ExprCall,
    pick: SegmentPick,
) -> Vec<(SymbolKey, TextRange)>{
    // Any byte inside the string picks the same call arg, so locate it from the range.
    let offset = string.range.start().to_usize() + 1;
    if let Some((arg_index, arg_expr)) = call_expr.arguments.args.iter().enumerate().find(|(_, arg)|
        offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize()
    ){
        find_positional_argument_symbols(session, scope, from_module, string, call_expr, arg_index, arg_expr, pick)
    } else if let Some((_, keyword)) = call_expr.arguments.keywords.iter().enumerate().find(|(_, arg)|
        offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize()
    ){
        find_keyword_argument_symbols(session, scope, from_module, string, call_expr, keyword, pick)
    } else {
        vec![]
    }
}
