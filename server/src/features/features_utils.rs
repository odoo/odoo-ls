use itertools::Itertools;
use ruff_python_ast::{Expr, ExprCall, Keyword};
use ruff_text_size::{Ranged, TextRange, TextSize};
use crate::core::file_mgr::FileMgr;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::function_symbol::Argument;
use crate::utils::{compare_semver, PathSanitizer};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Weak;
use std::{cell::RefCell, rc::Rc};

use crate::constants::{PackageType, SymType};
use crate::constants::OYarn;
use crate::core::evaluation::{Context, ContextValue, Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak, EvaluationValue};
use crate::core::symbols::symbol::Symbol;
use crate::threads::SessionInfo;
use crate::{oyarn, Sy, S};


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
    pub fn find_field_symbols(
        session: &mut SessionInfo,
        scope: Rc<RefCell<Symbol>>,
        from_module: Option<Rc<RefCell<Symbol>>>,
        field_value: &String,
        call_expr: &ExprCall,
        offset: &usize,
    ) -> Vec<Rc<RefCell<Symbol>>>{
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
        let Some(parent_class) = scope.borrow().get_in_parents(&vec![SymType::CLASS], true).and_then(|p| p.upgrade()) else {
            return vec![];
        };
        if parent_class.borrow().as_class_sym()._model.is_none(){
            return vec![];
        }
        let evaluations = Evaluation::eval_from_ast(session, &call_expr.func, scope.clone(), &call_expr.func.range().start(), false, &mut vec![]).0;
        let mut followed_evals = vec![];
        for eval in evaluations {
            followed_evals.extend(Symbol::follow_ref(&eval.symbol.get_symbol(session, &mut None, &mut vec![], None), session, &mut None, true, false, None));
        }
        if !followed_evals.iter().any(|eval|
            eval.is_weak() && eval.as_weak().weak.upgrade().map(|sym| sym.borrow().is_field_class(session)).unwrap_or(false)
        ) {
            return vec![];
        }
        parent_class.clone().borrow().get_member_symbol(session, field_value, from_module.clone(), false, false, true, true, false).0
    }

    fn find_inverse_name_field_symbol(
        session: &mut SessionInfo,
        from_module: Option<Rc<RefCell<Symbol>>>,
        field_value: &String,
        call_expr: &ExprCall,
    ) -> Vec<Rc<RefCell<Symbol>>>{
        let model_name = if let Some(Expr::StringLiteral(expr)) = call_expr.arguments.args.first() {
            expr.value.to_string()
        } else {
            let Some(model_name) = call_expr.arguments.keywords.iter().find(|kw| kw.arg.as_ref().map(|arg| arg.id == "comodel_name").unwrap_or(false))
                .and_then(|kw| match &kw.value {
                    Expr::StringLiteral(expr) => Some(expr.value.to_string()),
                    _ => None
                }) else {
                    return vec![];
                };
            model_name
        };
        let Some(model) = session.sync_odoo.models.get(&oyarn!("{}", model_name)).cloned() else {
            return vec![];
        };
        let main_syms = model.borrow().get_main_symbols(session, from_module.clone());
        main_syms.iter().flat_map(|main_sym| main_sym.clone().borrow().get_member_symbol(session, field_value, from_module.clone(), false, true, false, true, false).0).collect()
    }

    fn find_simple_decorator_field_symbol(
        session: &mut SessionInfo,
        scope: Rc<RefCell<Symbol>>,
        from_module: Option<Rc<RefCell<Symbol>>>,
        field_name: &String,
    ) ->  Vec<Rc<RefCell<Symbol>>>{
        let Some(parent_class) = scope.borrow().get_in_parents(&vec![SymType::CLASS], true).and_then(|p| p.upgrade()) else {
            return vec![];
        };
        if parent_class.borrow().as_class_sym()._model.is_none(){
            return vec![];
        }
        parent_class.clone().borrow().get_member_symbol(session, field_name, from_module.clone(), false, true, false, true, false).0
    }

    fn find_nested_fields(
        session: &mut SessionInfo,
        base_symbol: Rc<RefCell<Symbol>>,
        from_module: Option<Rc<RefCell<Symbol>>>,
        field_range: &TextRange,
        field_name: &String,
        offset: &usize,
    ) ->  Vec<(Rc<RefCell<Symbol>>, TextRange)>{
        if base_symbol.borrow().as_class_sym()._model.is_none(){
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
                let fields = parent_object.clone().unwrap().borrow().get_member_symbol(session, &name, from_module.clone(), false, true, false,true, false).0;
                return fields.into_iter().map(|f| (f, TextRange::new(range_start, range_end - TextSize::new(1)))).collect();
            } else {
                let (symbols, _diagnostics) = parent_object.clone().unwrap().borrow().get_member_symbol(session,
                    &name.to_string(),
                    from_module.clone(),
                    false,
                    true,
                    false,
                    true,
                    false);
                if symbols.is_empty() {
                    break;
                }
                parent_object = None;
                for s in symbols.iter() {
                    if s.borrow().is_specific_field(session, &["Many2one", "One2many", "Many2many"]) && s.borrow().typ() == SymType::VARIABLE{
                        let models = s.borrow().as_variable().get_relational_model(session, from_module.clone());
                        if models.len() == 1 {
                            parent_object = Some(models[0].clone());
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
        scope: Rc<RefCell<Symbol>>,
        from_module: Option<Rc<RefCell<Symbol>>>,
        field_range: &TextRange,
        field_name: &String,
        offset: &usize,
    ) ->  Vec<(Rc<RefCell<Symbol>>, TextRange)>{
        let Some(parent_class) = scope.borrow().get_in_parents(&vec![SymType::CLASS], true).and_then(|p| p.upgrade()) else {
            return vec![];
        };
        FeaturesUtils::find_nested_fields(session, parent_class, from_module, field_range, field_name, offset)
    }

    fn find_domain_param_symbols(
        session: &mut SessionInfo,
        callable: &EvaluationSymbolWeak,
        field_range: &TextRange,
        field_name: &String,
        offset: &usize,
        from_module: &Option<Rc<RefCell<Symbol>>>,
    ) -> Vec<(Rc<RefCell<Symbol>>, TextRange)> {
        let Some(parent_object) = callable.context.get(&S!("base_attr")).and_then(|parent_object| parent_object.as_symbol().upgrade()) else {
            return vec![];
        };
        FeaturesUtils::find_nested_fields(session, parent_object, from_module.clone(), field_range, field_name, offset)
    }

    fn find_positional_argument_symbols(
        session: &mut SessionInfo,
        scope: Rc<RefCell<Symbol>>,
        from_module: Option<Rc<RefCell<Symbol>>>,
        field_name: &String,
        call_expr: &ExprCall,
        offset: usize,
        field_range: TextRange,
        arg_index: usize,
    ) -> Vec<(Rc<RefCell<Symbol>>, TextRange)> {
        let mut arg_symbols: Vec<(Rc<RefCell<Symbol>>, TextRange)> = vec![];
        let callable_evals = Evaluation::eval_from_ast(session, &call_expr.func, scope.clone(), &call_expr.func.range().start(), false, &mut vec![]).0;
        let mut followed_evals = vec![];
        for eval in callable_evals {
            followed_evals.extend(Symbol::follow_ref(&eval.symbol.get_symbol(session, &mut None, &mut vec![], None), session, &mut None, true, false, None));
        }
        for callable_eval in followed_evals {
            let EvaluationSymbolPtr::WEAK(callable) = callable_eval else {
                continue;
            };
            let Some(callable_sym) = callable.weak.upgrade() else {
                continue;
            };
            if callable_sym.borrow().typ() != SymType::FUNCTION {
                continue;
            }
            let func = callable_sym.borrow();

            // Check if we are in api.onchange/constrains/depends
            let func_sym_tree = func.get_tree();
            // TODO: account for change in tree after 18.1 odoo.orm.decorators
            let version_comparison = compare_semver(session.sync_odoo.full_version.as_str(), "18.1.0");
            if (version_comparison < Ordering::Equal && func_sym_tree.0.ends_with(&[Sy!("odoo"), Sy!("api")])) ||
                (version_comparison >= Ordering::Equal && func_sym_tree.0.ends_with(&[Sy!("odoo"), Sy!("orm"), Sy!("decorators")])){
                if [vec![Sy!("onchange")], vec![Sy!("constrains")]].contains(&func_sym_tree.1) && SyncOdoo::is_in_main_entry(session, &func_sym_tree.0) {
                    arg_symbols.extend(
                        FeaturesUtils::find_simple_decorator_field_symbol(session, scope.clone(), from_module.clone(), field_name)
                        .into_iter().map(|symbol| (symbol, field_range.clone()))
                    );
                    continue;
                } else if func_sym_tree.1 == vec![Sy!("depends")] && SyncOdoo::is_in_main_entry(session, &func_sym_tree.0){
                    arg_symbols.extend(
                        FeaturesUtils::find_nested_fields_in_class(session, scope.clone(), from_module.clone(), &field_range, field_name, &offset)
                    );
                    continue;
                }
            }

            let func_arg = func.as_func().get_indexed_arg_in_call(
                call_expr,
                arg_index as u32,
                callable.context.get(&S!("is_attr_of_instance")).map(|v| v.as_bool()));
            let Some(func_arg_sym) = func_arg.and_then(|func_arg| func_arg.symbol.upgrade()) else {
                continue
            };

            for evaluation in func_arg_sym.borrow().evaluations().unwrap().iter() {
                if matches!(evaluation.symbol.get_symbol_ptr(), EvaluationSymbolPtr::DOMAIN){
                    arg_symbols.extend(
                        FeaturesUtils::find_domain_param_symbols(session, &callable, &field_range, field_name, &offset, &from_module)
                    );
                }
            }

        }
        //Process kwargs related/comodel_name
        arg_symbols
    }

    fn find_keyword_argument_symbols(
        session: &mut SessionInfo,
        scope: Rc<RefCell<Symbol>>,
        from_module: Option<Rc<RefCell<Symbol>>>,
        field_name: &String,
        call_expr: &ExprCall,
        offset: usize,
        field_range: TextRange,
        keyword: &Keyword,
    ) -> Vec<(Rc<RefCell<Symbol>>, TextRange)> {
        // We only process the `related` keyword argument
        if keyword.arg.as_ref().filter(|kw_arg| kw_arg.id == "related").is_none(){
            return vec![];
        }
        let mut arg_symbols = vec![];
        let callable_evals = Evaluation::eval_from_ast(session, &call_expr.func, scope.clone(), &call_expr.func.range().start(), false, &mut vec![]).0;
        let mut followed_evals = vec![];
        for eval in callable_evals {
            followed_evals.extend(Symbol::follow_ref(&eval.symbol.get_symbol(session, &mut None, &mut vec![], None), session, &mut None, true, false, None));
        }
        for callable_eval in followed_evals {
            let EvaluationSymbolPtr::WEAK(callable) = callable_eval else {
                continue;
            };
            let Some(callable_sym) = callable.weak.upgrade() else {
                continue;
            };
            if !callable_sym.borrow().is_field_class(session) {
                continue;
            }
            arg_symbols.extend(
                FeaturesUtils::find_nested_fields_in_class(session, scope.clone(), from_module.clone(), &field_range, field_name, &offset)
            );
        }
        arg_symbols
    }

    pub fn find_argument_symbols(
        session: &mut SessionInfo,
        scope: Rc<RefCell<Symbol>>,
        from_module: Option<Rc<RefCell<Symbol>>>,
        field_name: &String,
        call_expr: &ExprCall,
        offset: usize,
        field_range: TextRange,
    ) -> Vec<(Rc<RefCell<Symbol>>, TextRange)>{
        if let Some((arg_index, _)) = call_expr.arguments.args.iter().enumerate().find(|(_, arg)|
            offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize()
        ){
            FeaturesUtils::find_positional_argument_symbols(session, scope, from_module, field_name, call_expr, offset, field_range, arg_index)
        } else if let Some((_, keyword)) = call_expr.arguments.keywords.iter().enumerate().find(|(_, arg)|
            offset > arg.range().start().to_usize() && offset <= arg.range().end().to_usize()
        ){
            FeaturesUtils::find_keyword_argument_symbols(session, scope, from_module, field_name, call_expr, offset, field_range, keyword)
        } else {
            vec![]
        }
    }

    fn check_for_string_special_syms(session: &mut SessionInfo, string_val: &String, call_expr: &ExprCall, offset: usize, field_range: TextRange, file_symbol: &Rc<RefCell<Symbol>>) -> Vec<Rc<RefCell<Symbol>>> {
        let from_module = file_symbol.borrow().find_module();
        let scope = Symbol::get_scope_symbol(file_symbol.clone(), offset as u32, false);
        let string_domain_fields_syms = FeaturesUtils::find_argument_symbols(session, scope.clone(), from_module.clone(),  string_val, call_expr, offset, field_range);
        if string_domain_fields_syms.len() >= 1 {
            return string_domain_fields_syms.into_iter().map(|(sym, _)| sym).collect();
        }
        let compute_kwarg_syms = FeaturesUtils::find_field_symbols(session, scope.clone(), from_module.clone(),  string_val, call_expr, &offset);
        if compute_kwarg_syms.len() >= 1{
            return compute_kwarg_syms;
        }
        vec![]
    }

    pub fn build_markdown_description(
        session: &mut SessionInfo,
        file_symbol: Option<Rc<RefCell<Symbol>>>,
        file_path: Option<&String>,
        evals: &Vec<Evaluation>,
        call_expr: &Option<ExprCall>,
        offset: Option<usize>
    ) -> String {
        #[derive(Debug, Eq, PartialEq, Hash)]
        struct SymbolKey {
            name: OYarn,
            type_: SymType,
        }
        struct SymbolInfo {
            sym_type_tag: String,
            from_module: Option<Rc<RefCell<Symbol>>>,
            inferred_types: Vec<InferredType>,
        }

        let mut blocks = vec![];
        let mut aggregator: HashMap<SymbolKey, Vec<SymbolInfo>> = HashMap::new();
        for (index, eval) in evals.iter().enumerate() {
            //search for a constant evaluation like a model name or domain field
            if let Some(EvaluationValue::CONSTANT(Expr::StringLiteral(expr))) = eval.value.as_ref() {
                let mut block = S!("");
                let str = expr.value.to_string();
                if let Some(SymType::PACKAGE(PackageType::MODULE)) = file_symbol.as_ref().map(|fs| fs.borrow().typ())
                && file_path.map_or(false, |fp| fp.ends_with("__manifest__.py")) {
                    // If we are in manifest, we check if the string is a module and list the underlying module dependencies
                    if let Some(module) = session.sync_odoo.modules.get(&oyarn!("{}", str)).and_then(|m| m.upgrade()) {
                        block += format!("Module: {}", module.borrow().name()).as_str();
                        let module_ref = module.borrow();
                        let dependencies = module_ref.as_module_package().get_all_depends();
                        if !dependencies.is_empty() {
                            block += "  \n***  \nDependencies:  \n";
                            block += &dependencies.iter().map(|dep| format!("- {}", dep)).join("  \n");
                        }
                    }
                    blocks.push(block);
                    continue;
                }
                let from_module = file_symbol.as_ref().and_then(|file_symbol| file_symbol.borrow().find_module());
                if let (Some(call_expression), Some(file_sym), Some(offset)) = (call_expr, file_symbol.as_ref(), offset){
                    let mut special_string_syms = FeaturesUtils::check_for_string_special_syms(session, &str, call_expression, offset, expr.range, file_sym);
                    // Inject `base_attr` to get descriptor type on follow_ref in features
                    if special_string_syms.len() >= 1{
                        special_string_syms.iter_mut().for_each(|sym_rc| {
                            sym_rc.borrow_mut().evaluations_mut().into_iter().flatten().for_each(|eval| {
                                match eval.symbol.get_mut_symbol_ptr() {
                                    EvaluationSymbolPtr::WEAK(weak) => {
                                        if let Some(field_parent) = weak.context.get(&S!("field_parent")) {
                                            if !weak.context.contains_key(&S!("base_attr")) {
                                                weak.context.insert(S!("base_attr"), field_parent.clone());
                                                weak.context.insert(S!("base_attr_inserted"), ContextValue::BOOLEAN(true));
                                            }
                                        }
                                    },
                                    _ => {}
                                }
                            });
                        });
                        // restart with replacing current index evaluation with field evaluations
                        let string_domain_fields_evals: Vec<Evaluation> = special_string_syms.iter()
                            .map(|sym| Evaluation::eval_from_symbol(&Rc::downgrade(sym), Some(true)))
                            .chain(evals.iter().take(index).cloned())
                            .chain(evals.iter().skip(index + 1).cloned())
                            .collect();
                        let r = FeaturesUtils::build_markdown_description(session, file_symbol, file_path, &string_domain_fields_evals, call_expr, Some(offset));
                        // remove the injected `base_attr` context value
                        special_string_syms.iter_mut().for_each(|sym_rc| {
                            sym_rc.borrow_mut().evaluations_mut().into_iter().flatten().for_each(|eval| {
                                match eval.symbol.get_mut_symbol_ptr() {
                                    EvaluationSymbolPtr::WEAK(weak) => {
                                        if let Some(ContextValue::BOOLEAN(true)) = weak.context.get(&S!("base_attr_inserted")) {
                                            // If we found a field parent, we can use it
                                            weak.context.remove(&S!("base_attr"));
                                            weak.context.remove(&S!("base_attr_inserted"));
                                        }
                                    },
                                    _ => {}
                                }
                            });
                        });
                        return r;
                    }
                }
                if let Some(model) = session.sync_odoo.models.get(&oyarn!("{}", str)).cloned() {
                    let main_classes = model.borrow().get_main_symbols(session, from_module.clone());
                    for main_class_rc in main_classes.iter() {
                        let main_class = main_class_rc.borrow();
                        if let Some(main_class_module) = main_class.find_module() {
                            block += format!("Model in {}: {}", main_class_module.borrow().name(), main_class.name()).as_str();
                            if main_class.doc_string().is_some() {
                                block = block + "  \n***  \n" + main_class.doc_string().as_ref().unwrap();
                            }
                            block += "  \n***  \n";
                            block += &model.borrow().all_symbols(session, from_module.clone(), false).into_iter()
                                .filter_map(|(sym, needed_module)| {
                                    if Rc::ptr_eq(&sym, main_class_rc) {
                                        None // Skip main_class
                                    } else {
                                        Some((sym.borrow().find_module().unwrap().borrow().name().clone(), needed_module))
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
                }
                blocks.push(block);
                continue;
            }
            let eval_symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let Some(symbol) = eval_symbol.upgrade_weak() else {
                if let EvaluationSymbolPtr::UNBOUND(name) = eval_symbol {
                    aggregator.entry(SymbolKey { name: name.clone(), type_: SymType::VARIABLE }).or_insert_with(Vec::new).push(
                        SymbolInfo {
                            sym_type_tag: S!("variable"), from_module: None, inferred_types: vec![InferredType{ eval_ptr: EvaluationSymbolPtr::UNBOUND(name.clone()), eval_info: TypeInfo::VALUE(S!("Unbound"))}]
                        }
                    );
                }
                continue;
            };
            let mut context = Some(eval_symbol.as_weak().context.clone());
            let evaluation_ptrs = Symbol::follow_ref(&eval_symbol, session, &mut context, false, false, None);

            let symbol_type = symbol.borrow().typ();
            let symbol_name = symbol.borrow().name().clone();
            let from_module = symbol.borrow().find_module();
            let sym_type_tag = FeaturesUtils::get_type_symbol_tag(&symbol);
            let return_types: Vec<TypeInfo> = evaluation_ptrs.iter().map(|eval| FeaturesUtils::get_inferred_types(session, eval, &mut context, &symbol_type)).unique().collect();
            let inferred_types = evaluation_ptrs.into_iter().zip(return_types.into_iter()).map(|(eval_ptr, eval_info)| InferredType{eval_ptr, eval_info}).collect();

            aggregator.entry(SymbolKey { name: symbol_name.clone(), type_: symbol_type.clone() }).or_insert_with(Vec::new).push(
                SymbolInfo { sym_type_tag, from_module, inferred_types}
            );
        }
        for (id, info_pieces) in aggregator.iter(){
            let mut block = S!("");
            let sym_type_tag = info_pieces.iter().map(|info| info.sym_type_tag.clone()).unique().collect::<Vec<_>>();
            let inferred_types = info_pieces.iter().flat_map(|info| info.inferred_types.clone()).collect::<Vec<_>>();
            let from_modules = info_pieces.iter().filter_map(|info| info.from_module.clone().map(|module_rc| module_rc.borrow().name().clone())).unique().collect::<Vec<_>>();
            // BLOCK 1: (type) **name** -> inferred_type
            block += FeaturesUtils::build_block_1(session, id.type_, &id.name, sym_type_tag, &inferred_types).as_str();
            // BLOCK 2: useful links
            block += inferred_types.iter().map(|typ| FeaturesUtils::get_useful_link(session, &typ.eval_ptr)).collect::<String>().as_str();
            // BLOCK 3: documentation
            if let Some(documentation_block) = FeaturesUtils::get_documentation_block(session, &from_modules, &inferred_types){
                block = block + "  \n***  \n" + &documentation_block;
            }
            blocks.push(block);
        }
        blocks.iter().join("  \n***  \n")
    }

    fn get_type_symbol_tag(rc_symbol: &Rc<RefCell<Symbol>>) -> String{
        let symbol = rc_symbol.borrow();
        match symbol.typ(){
            SymType::VARIABLE if symbol.as_variable().is_import_variable => S!("import"),
            SymType::VARIABLE if symbol.as_variable().is_parameter => S!("parameter"),
            SymType::FUNCTION if symbol.as_func().is_property => S!("property"),
            SymType::FUNCTION if symbol.parent().unwrap().upgrade().unwrap().borrow().typ() == SymType::CLASS => S!("method"),
            SymType::PACKAGE(_) => S!("package"),
            type_ => type_.to_string().to_lowercase()
        }
    }

    /// Given an Argument return its String representation
    /// Attempts to fetch the type hint to return `_arg: _arg_type`
    /// Otherwise just returns the argument name
    fn argument_presentation(session: &mut SessionInfo, arg: &Argument) -> String {
        let arg_name = arg.symbol.upgrade().unwrap().borrow().name().clone();
        match arg.annotation.as_ref() {
            Some(anno_expr) => {
                let Some(type_symbol) = arg.symbol.upgrade()
                .and_then(|arg_symbol| arg_symbol.borrow().parent())
                .and_then(|weak_parent| weak_parent.upgrade())
                .and_then(|parent| Evaluation::eval_from_ast(session, &anno_expr, parent.clone(), &anno_expr.range().start(), false, &mut vec![]).0.first().cloned())
                .and_then(|type_evaluation| type_evaluation.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None).weak.upgrade())
                 else {
                    return arg_name.to_string()
                };
                let type_name = type_symbol.borrow().name().clone();
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
            EvaluationSymbolPtr::WEAK(eval_weak) => {
                if let Some(inferred_type) = eval.upgrade_weak() {
                    let inferred_type = inferred_type.borrow();
                    match inferred_type.typ() {
                        SymType::FUNCTION if !inferred_type.as_func().is_property => {
                            let call_parent = match eval.as_weak().context.get(&S!("base_attr")){
                                Some(ContextValue::SYMBOL(s)) => s.clone(),
                                _ => {
                                    let parent = inferred_type.parent().and_then(|parent_weak| parent_weak.upgrade());
                                    if parent.is_some() && parent.as_ref().unwrap().borrow().typ() == SymType::CLASS {
                                        Rc::downgrade(&parent.unwrap())
                                    } else {
                                        Weak::new()
                                    }
                                }
                            };
                            context.as_mut().map(|ctx| ctx.insert(S!("base_call"), ContextValue::SYMBOL(call_parent)));
                            let return_type = match inferred_type.evaluations() {
                                Some(func_eval) => {
                                    let type_names: Vec<_> = func_eval.iter().flat_map(|eval|{
                                        let eval_symbol = eval.symbol.get_symbol_weak_transformed(session, context, &mut vec![], None);
                                        let weak_eval_symbols = Symbol::follow_ref(&eval_symbol, session, context, true, false, None);
                                        weak_eval_symbols.iter().map(|weak_eval_symbol| match weak_eval_symbol.upgrade_weak(){
                                            //if fct is a variable, it means that evaluation is None.
                                            Some(s_type) if s_type.borrow().typ() != SymType::VARIABLE => s_type.borrow().name().to_string(),
                                            _ => "Any".to_string()
                                        }).collect::<Vec<_>>()
                                    }).unique().collect();
                                    if !type_names.is_empty() {FeaturesUtils::represent_return_types(type_names)} else {S!("None")}
                                },
                                None => S!("None"),
                            };
                            context.as_mut().map(|ctx| ctx.remove(&S!("base_call")));
                            let argument_names = inferred_type.as_func().args.iter().map(|arg| FeaturesUtils::argument_presentation(session, arg)).join(", ");
                            TypeInfo::CALLABLE(CallableSignature { arguments: argument_names, return_types: return_type })
                        },
                        SymType::FILE => TypeInfo::VALUE(S!("File")),
                        SymType::PACKAGE(_) => TypeInfo::VALUE(S!("Module")),
                        SymType::NAMESPACE => TypeInfo::VALUE(S!("Namespace")),
                        SymType::CLASS => TypeInfo::VALUE(if eval_weak.is_super {format!("super[{}]", inferred_type.name())} else {inferred_type.name().to_string()}), // TODO: Maybe do something special if it is a descriptor
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
        && inferred_types[0].eval_ptr.is_weak()
        && inferred_types[0].eval_ptr.upgrade_weak().unwrap().borrow().typ() == SymType::FUNCTION
        && !inferred_types[0].eval_ptr.upgrade_weak().unwrap().borrow().as_func().is_property {
            //display 'def' only if there is only a single evaluation to a function
            single_func_eval = true;
            let TypeInfo::CALLABLE(CallableSignature { arguments, return_types: _ }) = &inferred_types[0].eval_info else {
                unreachable!("Information of a Function evaluation should be a Callable not a Value");
            };
            value += &format!("def {}({}) -> ", symbol_name, arguments);
        } else {
            if symbol_type == SymType::CLASS && inferred_types.len() == 1 && inferred_types[0].eval_ptr.is_weak() && inferred_types[0].eval_ptr.as_weak().is_super {
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
        if let Some(pos) = return_types.iter().position(|n| *n == S!("Any")) {
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

    /// Finds and returns useful links for an evaluation
    fn get_useful_link(_session: &mut SessionInfo, typ: &EvaluationSymbolPtr) -> String {
        // Possibly add more links in the future
        let Some(typ) = typ.upgrade_weak() else {
            return S!("")
        };
        let paths = &typ.borrow().paths();
        if paths.len() == 1 { //we won't put a link to a namespace
            let type_ref = typ.borrow();
            let base_path = match type_ref.typ() {
                SymType::PACKAGE(_) => PathBuf::from(paths.first().unwrap().clone()).join(format!("__init__.py{}", type_ref.as_package().i_ext())).sanitize(),
                _ => paths.first().unwrap().clone()
            };
            let path = FileMgr::pathname2uri(&base_path);
            let range = if type_ref.is_file_content() { type_ref.range().start().to_u32() } else { 0 };
            format!("  \n***  \nSee also: [{}]({}#{}){}", type_ref.name().as_str(), path.as_str(), range, "  \n")
        } else {
            S!("")
        }
    }

    /// Documentation block that includes the source module(s) and docstrings if found
    fn get_documentation_block(session: &mut SessionInfo, from_modules: &Vec<OYarn>, type_refs: &Vec<InferredType>) -> Option<String> {
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
            if let Some(typ) = typ.eval_ptr.upgrade_weak() {
                if typ.borrow().doc_string().is_some() {
                    // Replace leading spaces with nbsps to avoid it being parsed as a Markdown Codeblock
                    let ds = typ.borrow().doc_string().as_ref().unwrap()
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

    pub fn get_line_break(session: &mut SessionInfo<'_>) -> &'static str {
        if session.sync_odoo.capabilities.general.is_none() ||
        session.sync_odoo.capabilities.general.as_ref().unwrap().markdown.is_none() {
            return "  \\\n"
        }
        "  \n"
    }
}