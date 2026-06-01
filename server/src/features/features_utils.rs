use itertools::Itertools;
use ruff_python_ast::{Expr, ExprCall, Keyword};
use ruff_text_size::{Ranged, TextRange, TextSize};
use crate::core::file_mgr::FileMgr;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::function_symbol::Argument;
use crate::core::symbols::symbol_keys::{ClassKey, ModuleKey, SourceFileKey, SymbolKey, Wk};
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::{FunctionSymbol, VariableSymbol};
use crate::tree::OYarnExt;
use crate::utils::HashMap;

use crate::constants::{SymType};
use crate::constants::OYarn;
use crate::core::evaluation::{Context, ContextKey, ContextValue, Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak, EvaluationValue};
use crate::threads::SessionInfo;
use crate::{Sy, S};


#[derive(Clone, Eq, PartialEq, Hash)]
pub struct CallableSignature {
    pub arguments: String,
    pub return_types: String,
}
#[derive(Clone, Eq, PartialEq, Hash)]
pub enum TypeInfo {
    CALLABLE(CallableSignature),
    VALUE(String),
}
impl TypeInfo {
    pub(crate) fn to_string(&self) -> String {
        match self {
            TypeInfo::CALLABLE(CallableSignature { arguments, return_types }) => format!("(({}) -> {})", arguments, return_types),
            TypeInfo::VALUE(value) => value.clone(),
        }
    }
}
#[derive(Clone)]
struct InferredType {
    eval_ptr: EvaluationSymbolPtr,
    eval_info: TypeInfo,
}
pub struct FeaturesUtils {}

impl FeaturesUtils {
    pub fn find_kwarg_methods_symbols(
        session: &mut SessionInfo,
        scope: SymbolKey,
        from_module: Option<ModuleKey>,
        field_value: &str,
        call_expr: &ExprCall,
        offset: &usize,
    ) -> Vec<SymbolKey>{
        if let Some(keyword) = call_expr.arguments.keywords.iter().find(|arg|
            *offset > arg.range().start().to_usize() && *offset <= arg.range().end().to_usize()
        ){
            let Some(ref arg_id) = keyword.arg else {
                return vec![];
            };
            if arg_id.as_str() == "inverse_name" {
                return FeaturesUtils::find_inverse_name_field_symbol(session, from_module, field_value, call_expr);
            }
            if !["compute", "inverse", "search"].contains(&arg_id.as_str()){
                return vec![];
            }
        } else {
            return vec![];
        }
        let Some(SymbolKey::Class(parent_class)) = session.st().get_in_parents(scope, &[SymType::CLASS], true) else {
            return vec![];
        };
        if session.st()[parent_class]._model.is_none(){
            return vec![];
        }
        let evaluations = Evaluation::eval_from_ast(session, &call_expr.func, scope, &call_expr.func.range().start(), false, &mut vec![]).0;
        let mut followed_evals = vec![];
        for eval in evaluations {
            followed_evals.extend(
                SymbolTable::follow_ref(
                    &eval.symbol.get_symbol(session, &mut None, &mut vec![], None),
                    session,
                    &mut None,
                    true,
                    false,
                    None,
                    None,
                )
            );
        }
        if !followed_evals.iter().any(|eval|
            eval.has_weak() && eval.get_weak().weak.upgrade(session.st()).map(|sym| SymbolTable::is_field_class(session, sym)).unwrap_or(false)
        ) {
            return vec![];
        }
        SymbolTable::get_member_symbol(session, parent_class.into(), field_value, from_module, false, false, true, true, false).0
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
        let Some(model) = session.sync_odoo.models.get(model_name).cloned() else {
            return vec![];
        };
        let main_syms = model.borrow().get_main_symbols(session, from_module);
        main_syms.iter().flat_map(|&main_sym| SymbolTable::get_member_symbol(session, main_sym.into(), field_value, from_module, false, true, false, true, false).0).collect()
    }

    fn find_simple_decorator_field_symbol(
        session: &mut SessionInfo,
        scope: SymbolKey,
        from_module: Option<ModuleKey>,
        field_name: &str,
    ) ->  Vec<SymbolKey>{
        let Some(SymbolKey::Class(parent_class)) = session.st().get_in_parents(scope, &[SymType::CLASS], true) else {
            return vec![];
        };
        if session.st()[parent_class]._model.is_none() {
            return vec![];
        }
        SymbolTable::get_member_symbol(session, parent_class.into(), field_name, from_module, false, true, false, true, false).0
    }

    fn find_nested_fields(
        session: &mut SessionInfo,
        base_symbol: ClassKey,
        from_module: Option<ModuleKey>,
        field_range: &TextRange,
        field_name: &str,
        offset: &usize,
    ) -> Vec<(SymbolKey, TextRange)> {
        if session.st()[base_symbol]._model.is_none() {
            return vec![];
        }
        let mut parent_object = Some(base_symbol);
        let mut range_start = field_range.start() + TextSize::new(1);
        for name in field_name.split(".").map(|x| x.to_string()) {
            if parent_object.is_none() {
                break;
            }
            let range_end = range_start + TextSize::new((name.len() + 1) as u32);
            let cursor_section = TextRange::new(range_start, range_end).contains(TextSize::new(*offset as u32));
            if cursor_section {
                let fields = SymbolTable::get_member_symbol(session, parent_object.unwrap().into(), &name, from_module, false, true, false, true, false).0;
                return fields.into_iter().map(|f| (f, TextRange::new(range_start, range_end - TextSize::new(1)))).collect();
            } else {
                let (symbols, _diagnostics) = SymbolTable::get_member_symbol(session,
                    parent_object.unwrap().into(),
                    &name.to_string(),
                    from_module,
                    false,
                    true,
                    false,
                    true,
                    false);
                if symbols.is_empty() {
                    break;
                }
                parent_object = None;
                for s in symbols {
                    if let SymbolKey::Variable(variable_key) = s && SymbolTable::is_specific_field(session, s, &["Many2one", "One2many", "Many2many"]) {
                        let models = VariableSymbol::get_relational_model(variable_key, session, from_module);
                        if models.len() == 1 {
                            parent_object = Some(models[0]);
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            range_start = range_end;
        }
        vec![]
    }

    fn find_nested_fields_in_class(
        session: &mut SessionInfo,
        scope: SymbolKey,
        from_module: Option<ModuleKey>,
        field_range: &TextRange,
        field_name: &str,
        offset: &usize,
    ) ->  Vec<(SymbolKey, TextRange)>{
        let Some(SymbolKey::Class(parent_class)) = session.st().get_in_parents(scope, &[SymType::CLASS], true) else {
            return vec![];
        };
        FeaturesUtils::find_nested_fields(session, parent_class, from_module, field_range, field_name, offset)
    }

    fn find_domain_param_symbols(
        session: &mut SessionInfo,
        callable: &EvaluationSymbolWeak,
        field_range: &TextRange,
        field_name: &str,
        offset: &usize,
        from_module: Option<ModuleKey>,
    ) -> Vec<(SymbolKey, TextRange)> {
        let Some(SymbolKey::Class(parent_object)) = callable.context.get(ContextKey::BaseAttr).and_then(|parent_object| parent_object.as_symbol().upgrade(session.st())) else {
            return vec![];
        };
        FeaturesUtils::find_nested_fields(session, parent_object, from_module, field_range, field_name, offset)
    }

    fn find_positional_argument_symbols(
        session: &mut SessionInfo,
        scope: SymbolKey,
        from_module: Option<ModuleKey>,
        field_name: &str,
        call_expr: &ExprCall,
        offset: usize,
        field_range: TextRange,
        arg_index: usize,
        arg_expr: &Expr,
    ) -> Vec<(SymbolKey, TextRange)> {
        let mut arg_symbols: Vec<(SymbolKey, TextRange)> = vec![];
        let callable_evals = Evaluation::eval_from_ast(session, &call_expr.func, scope, &call_expr.func.range().start(), false, &mut vec![]).0;
        let mut followed_evals = vec![];
        for eval in callable_evals {
            followed_evals.extend(
                SymbolTable::follow_ref(
                    &eval.symbol.get_symbol(session, &mut None, &mut vec![], None),
                    session,
                    &mut None,
                    true,
                    false,
                    None,
                    None,
                )
            );
        }
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
                            FeaturesUtils::find_simple_decorator_field_symbol(session, scope, from_module, field_name)
                            .into_iter().map(|symbol| (symbol, field_range.clone()))
                        );
                        continue;
                    } else if func_sym_tree.1 == &["depends"] && SyncOdoo::is_in_main_entry(session, &func_sym_tree.0){
                        arg_symbols.extend(
                            FeaturesUtils::find_nested_fields_in_class(session, scope, from_module, &field_range, field_name, &offset)
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
                    FeaturesUtils::find_inverse_name_field_symbol(session, from_module, &value, call_expr)
                    .into_iter().map(|res| (res, arg_expr.range()))
                );
                continue;
            }
            for evaluation in session.st()[func_arg_sym].evaluations.clone() {
                if matches!(evaluation.symbol.get_symbol_ptr(), EvaluationSymbolPtr::DOMAIN) {
                    arg_symbols.extend(
                        FeaturesUtils::find_domain_param_symbols(session, &callable, &field_range, field_name, &offset, from_module)
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
        field_name: &str,
        call_expr: &ExprCall,
        offset: usize,
        field_range: TextRange,
        keyword: &Keyword,
    ) -> Vec<(SymbolKey, TextRange)> {
        // We only process the `related` keyword argument
        if keyword.arg.as_ref().filter(|kw_arg| kw_arg.id == "related").is_none(){
            return vec![];
        }
        let mut arg_symbols = vec![];
        let callable_evals = Evaluation::eval_from_ast(session, &call_expr.func, scope, &call_expr.func.range().start(), false, &mut vec![]).0;
        let mut followed_evals = vec![];
        for eval in callable_evals {
            followed_evals.extend(
                SymbolTable::follow_ref(
                    &eval.symbol.get_symbol(session, &mut None, &mut vec![], None),
                    session,
                    &mut None,
                    true,
                    false,
                    None,
                    None,
                )
            );
        }
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
                FeaturesUtils::find_nested_fields_in_class(session, scope, from_module, &field_range, field_name, &offset)
            );
        }
        arg_symbols
    }


    pub fn find_argument_symbols(
        session: &mut SessionInfo,
        scope: SymbolKey,
        from_module: Option<ModuleKey>,
        field_name: &str,
        call_expr: &ExprCall,
        offset: usize,
        field_range: TextRange,
    ) -> Vec<(SymbolKey, TextRange)>{
        if let Some((arg_index, arg_expr)) = call_expr.arguments.args.iter().enumerate().find(|(_, arg)|
            offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize()
        ){
            FeaturesUtils::find_positional_argument_symbols(session, scope, from_module, field_name, call_expr, offset, field_range, arg_index, arg_expr)
        } else if let Some((_, keyword)) = call_expr.arguments.keywords.iter().enumerate().find(|(_, arg)|
            offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize()
        ){
            FeaturesUtils::find_keyword_argument_symbols(session, scope, from_module, field_name, call_expr, offset, field_range, keyword)
        } else {
            vec![]
        }
    }


    fn check_for_string_special_syms(session: &mut SessionInfo, string_val: &str, call_expr: &ExprCall, offset: usize, field_range: TextRange, file_symbol: SourceFileKey) -> Vec<SymbolKey> {
        let from_module = session.st().find_module(file_symbol);
        let scope = session.st().get_scope_symbol(file_symbol, offset as u32, false);
        let string_domain_fields_syms = FeaturesUtils::find_argument_symbols(session, scope, from_module,  string_val, call_expr, offset, field_range);
        if string_domain_fields_syms.len() >= 1 {
            return string_domain_fields_syms.into_iter().map(|(sym, _)| sym).collect();
        }
        let kwarg_syms = FeaturesUtils::find_kwarg_methods_symbols(session, scope, from_module,  string_val, call_expr, &offset);
        if kwarg_syms.len() >= 1{
            return kwarg_syms;
        }
        vec![]
    }

    pub fn build_markdown_description(
        session: &mut SessionInfo,
        file_symbol: Option<SourceFileKey>,
        file_path: Option<&String>,
        evals: &Vec<Evaluation>,
        call_expr: &Option<ExprCall>,
        offset: Option<usize>
    ) -> String {
        #[derive(Debug, Eq, PartialEq, Hash)]
        struct SymbolGroupKey {
            name: OYarn,
            type_: SymType,
        }
        struct SymbolInfo {
            sym_type_tag: String,
            from_module: Option<ModuleKey>,
            inferred_types: Vec<InferredType>,
        }

        let mut blocks = vec![];
        let mut aggregator: HashMap<SymbolGroupKey, Vec<SymbolInfo>> = HashMap::default();
        for (index, eval) in evals.iter().enumerate() {
            //search for a constant evaluation like a model name or domain field
            if let Some(expr) = eval.value.as_ref().and_then(EvaluationValue::as_string_literal) {
                let str = expr.value.to_str();
                if let Some(SourceFileKey::Module(_)) = file_symbol
                && file_path.is_some_and(|fp| fp.ends_with("__manifest__.py")) {
                    let mut block = S!("");
                    // If we are in manifest, we check if the string is a module and list the underlying module dependencies
                    if let Some(module_key) = session.sync_odoo.modules.get(str).copied().and_then(|m| m.upgrade(session.st())) {
                        let module = &session.st()[module_key];
                        block += format!("Module: {}", module.name).as_str();
                        let dependencies = module.get_all_depends();
                        if !dependencies.is_empty() {
                            block += "  \n***  \nDependencies:  \n";
                            block += &dependencies.iter().map(|dep| format!("- {}", dep)).join("  \n");
                        }
                    }
                    blocks.push(block);
                    continue;
                }
                let from_module = file_symbol.and_then(|fs| session.st().find_module(fs));
                if let (Some(call_expression), Some(file_sym), Some(offset)) = (call_expr, file_symbol, offset){
                    let special_string_syms = FeaturesUtils::check_for_string_special_syms(session, str, call_expression, offset, expr.range, file_sym);
                    // Inject `base_attr` to get descriptor type on follow_ref in features
                    if !special_string_syms.is_empty() {
                        FeaturesUtils::inject_base_attr(session, &special_string_syms);
                        // restart with replacing current index evaluation with field evaluations
                        let st = &session.sync_odoo.symbol_table;
                        let string_domain_fields_evals: Vec<Evaluation> = special_string_syms.iter()
                            .map(|&sym| Evaluation::eval_from_symbol(st, sym, Some(true)))
                            .chain(evals.iter().take(index).cloned())
                            .chain(evals.iter().skip(index + 1).cloned())
                            .collect();
                        let r = FeaturesUtils::build_markdown_description(session, file_symbol, file_path, &string_domain_fields_evals, call_expr, Some(offset));
                        // remove the injected `base_attr` context value
                        FeaturesUtils::remove_base_attr(session, &special_string_syms);
                        return r;
                    }
                }
                if let Some(model) = session.sync_odoo.models.get(str).cloned() {
                    let main_classes = model.borrow().get_main_symbols(session, from_module);
                    let mut block = S!("");
                    for main_class in main_classes {
                        let st = &session.sync_odoo.symbol_table;
                        if let Some(main_class_module) = st.find_module(main_class) {
                            block += format!("Model in {}: {}", st[main_class_module].name, st[main_class].name).as_str();
                            if let Some(doc_string) = &st[main_class].doc_string {
                                block = block + "  \n***  \n" + doc_string;
                            }
                            block += "  \n***  \n";
                            block += &model.borrow().all_symbols(session, from_module, false).into_iter()
                                .filter_map(|(sym, needed_module)| {
                                    if sym == main_class {
                                        None // Skip main_class
                                    } else {
                                        let module = st.find_module(sym).unwrap();
                                        Some((st[module].name.clone(), needed_module))
                                    }
                                }).unique_by(|(name, _)| name.clone())
                                .sorted_by(|x, y| {
                                    if x.1.is_none() && y.1.is_some() {
                                        std::cmp::Ordering::Less
                                    } else if x.1.is_some() && y.1.is_none() {
                                        std::cmp::Ordering::Greater
                                    } else {
                                        x.0.cmp(&y.0)
                                    }
                                })
                                .map(|(mod_name, needed_module)| {
                                    match needed_module {
                                        Some(module) => format!("inherited in {} (require {}){}", mod_name, module, FeaturesUtils::get_line_break(session)),
                                        None => format!("inherited in {}{}", mod_name, FeaturesUtils::get_line_break(session))
                                    }
                                }).collect::<String>();
                        }
                    }
                    blocks.push(block);
                    continue;
                }
            }
            let eval_symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let Some(symbol) = eval_symbol.upgrade_weak(session.st()) else {
                if let EvaluationSymbolPtr::UNBOUND(name) = eval_symbol {
                    aggregator.entry(SymbolGroupKey { name: name.clone(), type_: SymType::VARIABLE }).or_default().push(
                        SymbolInfo {
                            sym_type_tag: S!("variable"), from_module: None, inferred_types: vec![InferredType{ eval_ptr: EvaluationSymbolPtr::UNBOUND(name.clone()), eval_info: TypeInfo::VALUE(S!("Unbound"))}]
                        }
                    );
                }
                continue;
            };
            let mut context = Some(eval_symbol.get_weak().context.clone());
            let evaluation_ptrs = SymbolTable::follow_ref(&eval_symbol, session, &mut context, false, false, None, None);

            let symbol_type = symbol.typ();
            let symbol_name = session.st().name(symbol).clone();
            let from_module = session.st().find_module(symbol);
            let sym_type_tag = FeaturesUtils::get_type_symbol_tag(&session.sync_odoo.symbol_table, symbol);
            let return_types: Vec<TypeInfo> = evaluation_ptrs.iter().map(|eval| FeaturesUtils::get_inferred_types(session, eval, &mut context, &symbol_type)).unique().collect();
            let inferred_types = evaluation_ptrs.into_iter().zip(return_types.into_iter()).map(|(eval_ptr, eval_info)| InferredType{eval_ptr, eval_info}).collect();

            aggregator.entry(SymbolGroupKey { name: symbol_name.clone(), type_: symbol_type }).or_default().push(
                SymbolInfo { sym_type_tag, from_module, inferred_types}
            );
        }
        for (id, info_pieces) in aggregator.iter(){
            let mut block = S!("");
            let sym_type_tag = info_pieces.iter().map(|info| info.sym_type_tag.clone()).unique().collect::<Vec<_>>();
            let inferred_types = info_pieces.iter().flat_map(|info| info.inferred_types.clone()).collect::<Vec<_>>();
            let from_modules = info_pieces.iter().filter_map(|info| info.from_module.map(|mk| session.st()[mk].name.clone())).unique().collect::<Vec<_>>();
            // BLOCK 1: (type) **name** -> inferred_type
            block += FeaturesUtils::build_block_1(session, id.type_, &id.name, sym_type_tag, &inferred_types).as_str();
            // BLOCK 2: documentation
            if let Some(documentation_block) = FeaturesUtils::get_documentation_block(session, &from_modules, &inferred_types){
                block = block + "  \n***  \n" + &documentation_block;
            }
            // BLOCK 3: useful links or directory paths
            block += inferred_types.iter().map(|typ| FeaturesUtils::get_location_info(session, &typ.eval_ptr)).collect::<String>().as_str();
            blocks.push(block);
        }
        blocks.iter().join("  \n***  \n")
    }

    fn get_type_symbol_tag(symbol_table: &SymbolTable, symbol_key: SymbolKey) -> String {
        match symbol_key {
            SymbolKey::Variable(v) if symbol_table[v].is_import_variable => S!("import"),
            SymbolKey::Variable(v) if symbol_table[v].is_parameter => S!("parameter"),
            SymbolKey::Function(f) if symbol_table[f].is_property => S!("property"),
            SymbolKey::Function(f) if symbol_table.parent(f).unwrap().typ() == SymType::CLASS => S!("method"),
            SymbolKey::PythonPackage(_) | SymbolKey::Module(_) => S!("package"),
            type_ => type_.typ().to_string().to_lowercase()
        }
    }


    /// Given an Argument return its String representation
    /// Attempts to fetch the type hint to return `_arg: _arg_type`
    /// Otherwise just returns the argument name
    fn argument_presentation(session: &mut SessionInfo, arg: &Argument) -> String {
        let arg_symbol = arg.symbol.upgrade(session.st()).unwrap();
        let arg_name = session.st()[arg_symbol].name.clone();
        match arg.annotation.as_ref() {
            Some(anno_expr) => {
                let Some(type_symbol) = session.st().parent(arg_symbol)
                .and_then(|parent| Evaluation::eval_from_ast(session, &anno_expr, parent, &anno_expr.range().start(), false, &mut vec![]).0.first().cloned())
                .and_then(|type_evaluation| type_evaluation.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None).weak.upgrade(session.st()))
                 else {
                    return arg_name.to_string()
                };
                let type_name = session.st().name(type_symbol);
                format!("{}: {}", arg_name, type_name)
            },
            None => arg_name.to_string()
        }
    }


    /// Return return type representation of evaluation
    /// for a function evaluation it is typically (_arg: _arg_type, ...) -> (_result_type)
    /// for variable it just shows the type, or Any if it fails to find it
    pub fn get_inferred_types(session: &mut SessionInfo, eval: &EvaluationSymbolPtr, context: &mut Option<Context>, symbol_type: &SymType) -> TypeInfo {
        if *symbol_type == SymType::CLASS{
            return TypeInfo::VALUE(S!(""));
        }
        match eval {
            EvaluationSymbolPtr::WEAK(eval_weak) | EvaluationSymbolPtr::SELF(eval_weak)  => {
                if let Some(inferred_type) = eval.upgrade_weak(session.st()) {
                    // let inferred_type = inferred_type.borrow();
                    match inferred_type {
                        SymbolKey::Function(function_key) if !session.st()[function_key].is_property => {
                            let base_attr_ctx_ref = eval.get_weak().context.get(ContextKey::BaseAttr).or(context.as_mut().and_then(|ctx| ctx.get(ContextKey::BaseAttr)));
                            let call_parent = match base_attr_ctx_ref {
                                Some(ContextValue::SYMBOL(s)) => *s,
                                _ => {
                                    let parent_option = session.st().parent(inferred_type);
                                    if let Some(parent) = parent_option && parent.typ() == SymType::CLASS {
                                        Wk::from(parent)
                                    } else {
                                        Wk::null()
                                    }
                                }
                            };
                            context.as_mut().map(|ctx| ctx.insert(ContextKey::BaseCall, ContextValue::SYMBOL(call_parent)));
                            let return_type = {
                                let func_eval = session.st()[function_key].evaluations.clone();
                                let type_names: Vec<_> = func_eval.iter().flat_map(|eval|{
                                    let eval_symbol = eval.symbol.get_symbol_weak_transformed(session, context, &mut vec![], None);
                                    let weak_eval_symbols = SymbolTable::follow_ref(&eval_symbol, session, context, false, false, None, None);
                                    weak_eval_symbols.iter().map(|weak_eval_symbol| match weak_eval_symbol.upgrade_weak(session.st()) {
                                        //if fct is a variable, it means that evaluation is None.
                                        Some(s_type) if s_type.typ() != SymType::VARIABLE => session.st().name(s_type).to_string(),
                                        _ => "Any".to_string()
                                    }).collect::<Vec<_>>()
                                }).unique().collect();
                                if !type_names.is_empty() {FeaturesUtils::represent_return_types(type_names)} else {S!("None")}
                            };
                            context.as_mut().map(|ctx| ctx.remove(ContextKey::BaseCall));
                            let argument_names = session.st()[function_key].args.clone().iter().map(|arg| FeaturesUtils::argument_presentation(session, arg)).join(", ");
                            TypeInfo::CALLABLE(CallableSignature { arguments: argument_names, return_types: return_type })
                        },
                        SymbolKey::File(_) => TypeInfo::VALUE(S!("File")),
                        SymbolKey::PythonPackage(_) | SymbolKey::Module(_) => TypeInfo::VALUE(S!("Module")),
                        SymbolKey::Namespace(_) => TypeInfo::VALUE(S!("Namespace")),
                        SymbolKey::Class(class_key) => TypeInfo::VALUE(if eval_weak.is_super {format!("super[{}]", session.st()[class_key].name)} else {session.st()[class_key].name.to_string()}), // TODO: Maybe do something special if it is a descriptor
                        _ => TypeInfo::VALUE(S!("Any"))
                    }
                } else {
                    TypeInfo::VALUE(S!("Any"))
                }
            }
            EvaluationSymbolPtr::ANY => TypeInfo::VALUE(S!("Any")),
            EvaluationSymbolPtr::NONE => TypeInfo::VALUE(S!("None")),
            _ => TypeInfo::VALUE(S!("Any")),
        }
    }

    /*
    Build the first block of the hover. It contains the name of the variable as well as the type.
    parameters:   (type_sym)  symbol: inferred_types
    For example: "(parameter) self: type[Self@ResPartner]"
     */
    fn build_block_1(session: &mut SessionInfo, symbol_type: SymType, symbol_name: &OYarn, type_sym: Vec<String>, inferred_types: &Vec<InferredType>) -> String {
        //python code balise
        let mut value = S!(format!("```python{}", FeaturesUtils::get_line_break(session)));
        //type name
        value += &format!("({}) ", type_sym.iter().join(" | "));
        let mut single_func_eval = false;
        //variable name
        if inferred_types.len() == 1
        && inferred_types[0].eval_ptr.upgrade_weak(session.st())
            .is_some_and(|s| matches!(s, SymbolKey::Function(fk) if !session.st()[fk].is_property)) {
            //display 'def' only if there is only a single evaluation to a function
            single_func_eval = true;
            let TypeInfo::CALLABLE(CallableSignature { arguments, return_types: _ }) = &inferred_types[0].eval_info else {
                unreachable!("Information of a Function evaluation should be a Callable not a Value");
            };
            value += &format!("def {}({}) -> ", symbol_name, arguments);
        } else {
            if symbol_type == SymType::CLASS && inferred_types.len() == 1 && inferred_types[0].eval_ptr.has_weak() && inferred_types[0].eval_ptr.get_weak().is_super {
                value += &format!("(super[{}]) ", symbol_name);
            } else {
                value += symbol_name;
            }
            if symbol_type != SymType::CLASS {
                value += ": ";
            }
        }
        let return_types_string = inferred_types.iter().map(|rt| match &rt.eval_info {
            TypeInfo::CALLABLE(CallableSignature { arguments, return_types }) => {
                if single_func_eval {return_types.clone()} else {format!("(({}) -> {})", arguments, return_types)}
            },
            TypeInfo::VALUE(value) => value.clone(),
        }).unique().collect::<Vec<_>>();
        value += &format!("{}{}```", FeaturesUtils::represent_return_types(return_types_string), "  \n"); //No need to put an escaped \n in code bloc
        //end block
        value
    }
    /// Given a list of return types from evaluations
    /// Combine them into one string, surround with parenthesis
    ///   if there is more than one type.
    /// Always puts the Any type at the end. e.g. `(int | dict | Any)``
    fn represent_return_types(return_types: Vec<String>) -> String {
        let mut return_types = return_types.clone();
        if let Some(pos) = return_types.iter().position(|n| n == "Any") {
            let last_index = return_types.len() - 1;
            return_types.swap(pos, last_index);
        }
        let return_types_string = return_types.iter().join(" | ");
        if return_types.len() > 1 {
            format!("({})", return_types_string)
        } else {
            return_types_string
        }

    }
    /// Finds and returns useful links or directory locations for an evaluation
    fn get_location_info(session: &SessionInfo, typ: &EvaluationSymbolPtr) -> String {
        let lb = FeaturesUtils::get_line_break(session);
        let symbol_table = &session.sync_odoo.symbol_table;
        // Possibly add more links in the future
        let Some(sym) = typ.upgrade_weak(symbol_table) else {
            return S!("")
        };
        match sym {
            SymbolKey::Namespace(ns_key) => {
                // List namespace directories
                let namespace = &symbol_table[ns_key];
                let paths = namespace.paths();
                let name = &namespace.name;
                match paths.len() {
                    0 => S!(""),
                    1 => format!("  \n***  \n`{name}` namespace directory: `{}`{lb}", paths[0]),
                    _ => {
                        let path_list = paths.iter().map(|p| format!("- `{p}`")).join(lb);
                        format!("  \n***  \n`{name}` namespace directories:{lb}{path_list}{lb}")
                    }
                }
            }
            SymbolKey::PythonPackage(_) | SymbolKey::Module(_)
            | SymbolKey::File(_) | SymbolKey::XmlFile(_) | SymbolKey::CsvFile(_) => {
                // Get useful link
                let source_file = sym.as_source_file_key().unwrap();
                let uri = FileMgr::pathname2uri(symbol_table.file_path(source_file));
                let name = symbol_table.name(sym);
                format!("  \n***  \nSee also: [{}]({}#{}){lb}", name, uri.as_str(), 0)
            }
            SymbolKey::Compiled(compiled_key) => {
                let compile_symbol = &symbol_table[compiled_key];
                format!("  \n***  \n`{}` is a binary at `{}`{lb}", compile_symbol.name, compile_symbol.path)
            }
            _ => S!(""),
        }
    }

    /// Documentation block that includes the source module(s) and docstrings if found
    fn get_documentation_block(session: &SessionInfo, from_modules: &Vec<OYarn>, type_refs: &Vec<InferredType>) -> Option<String> {
        let symbol_table = &session.sync_odoo.symbol_table;
        let mut documentation_block = None;
        if !from_modules.is_empty(){
            documentation_block = Some(
                format!(
                    "From module{} `{}`",
                    if from_modules.len() > 1 {"s"} else {""},
                    from_modules.iter().join(", "))
            );
        }
        for typ in type_refs.iter() {
            if let Some(sym) = typ.eval_ptr.upgrade_weak(symbol_table) {
                let doc = FeaturesUtils::get_doc_string(symbol_table, sym);
                if let Some(ds_str) = doc {
                    // Replace leading spaces with nbsps to avoid it being parsed as a Markdown Codeblock
                    let ds = ds_str
                    .lines()
                    .map(|line| {
                        let leading_spaces = line.chars().take_while(|&ch| ch == ' ').count();
                        let nbsp_replacement = "&nbsp;".repeat(leading_spaces);
                        format!("{}{}", nbsp_replacement, &line[leading_spaces..])
                    })
                    .collect::<Vec<String>>()
                    .join(FeaturesUtils::get_line_break(session));
                    documentation_block = match documentation_block {
                        Some(from_module_str) => Some(from_module_str + FeaturesUtils::get_line_break(session) + &ds),
                        None => Some(ds)
                    };
                }
            }
        }
        documentation_block
    }

    fn get_doc_string(symbol_table: & SymbolTable, sym: SymbolKey) -> Option<&String> {
        match sym {
            SymbolKey::Class(k) => symbol_table[k].doc_string.as_ref(),
            SymbolKey::Function(k) => symbol_table[k].doc_string.as_ref(),
            SymbolKey::Variable(k) => symbol_table[k].doc_string.as_ref(),
            _ => None,
        }
    }

    pub fn get_line_break(session: &SessionInfo) -> &'static str {
        if session.sync_odoo.capabilities.general.is_none() ||
        session.sync_odoo.capabilities.general.as_ref().unwrap().markdown.is_none() {
            return "  \\\n"
        }
        "  \n"
    }

    /// Helper: inject `base_attr` context into symbol evaluations
    fn inject_base_attr(session: &mut SessionInfo, syms: &[SymbolKey]) {
        for &sym in syms {
            let evals = match sym {
                SymbolKey::Function(fk) => &mut session.sync_odoo.symbol_table[fk].evaluations,
                SymbolKey::Variable(vk) => &mut session.sync_odoo.symbol_table[vk].evaluations,
                _ => continue,
            };
            for eval in evals {
                if let EvaluationSymbolPtr::WEAK(weak) = eval.symbol.get_mut_symbol_ptr() {
                    if let Some(field_parent) = weak.context.get(ContextKey::FieldParent).cloned() {
                        if !weak.context.contains_key(&ContextKey::BaseAttr) {
                            weak.context.insert(ContextKey::BaseAttr, field_parent);
                            weak.context.insert(ContextKey::BaseAttrInserted, ContextValue::BOOLEAN(true));
                        }
                    }
                }
            }
        }
    }

    /// Helper: remove injected `base_attr` context from symbol evaluations
    fn remove_base_attr(session: &mut SessionInfo, syms: &[SymbolKey]) {
        for &sym in syms {
            let evals = match sym {
                SymbolKey::Function(fk) => &mut session.sync_odoo.symbol_table[fk].evaluations,
                SymbolKey::Variable(vk) => &mut session.sync_odoo.symbol_table[vk].evaluations,
                _ => continue,
            };
            for eval in evals {
                if let EvaluationSymbolPtr::WEAK(weak) = eval.symbol.get_mut_symbol_ptr() {
                    if let Some(ContextValue::BOOLEAN(true)) = weak.context.get(ContextKey::BaseAttrInserted) {
                        weak.context.remove(ContextKey::BaseAttrInserted);
                        weak.context.remove(ContextKey::BaseAttr);
                    }
                }
            }
        }
    }
}
