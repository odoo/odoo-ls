use itertools::Itertools;
use ruff_python_ast::{Expr, ExprCall, Keyword};
use ruff_text_size::{Ranged, TextRange, TextSize};
use crate::core::file_mgr::FileMgr;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::function_symbol::Argument;
use crate::utils::PathSanitizer;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Weak;
use std::{cell::RefCell, rc::Rc};

use crate::constants::SymType;
use crate::constants::OYarn;
use crate::core::evaluation::{Context, ContextValue, Evaluation, EvaluationSymbolPtr, EvaluationSymbolWeak, EvaluationValue};
use crate::core::symbols::symbol::Symbol;
use crate::threads::SessionInfo;
use crate::{oyarn, Sy, S};

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
        if let Some((_, keyword)) = call_expr.arguments.keywords.iter().enumerate().find(|(_, arg)|
            *offset > arg.range().start().to_usize() && *offset <= arg.range().end().to_usize()
        ){
            let Some(ref arg_id) = keyword.arg else {
                return vec![];
            };
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
        let evaluations = Evaluation::eval_from_ast(session, &call_expr.func, scope.clone(), &call_expr.func.range().start()).0;
        if !evaluations.iter().any(|eval|
            match eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None).weak.upgrade() {
                Some(sym_rc) => sym_rc.borrow().is_field_class(session),
                None => false
            }
        ){
            return vec![];
        }
        parent_class.clone().borrow().get_member_symbol(session, field_value, from_module.clone(), false, false, true, false).0
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
        parent_class.clone().borrow().get_member_symbol(session, field_name, from_module.clone(), false, false, true, false).0
    }

    fn find_nested_fields(
        session: &mut SessionInfo,
        base_symbol: Rc<RefCell<Symbol>>,
        from_module: Option<Rc<RefCell<Symbol>>>,
        field_range: &TextRange,
        field_name: &String,
        offset: &usize,
    ) ->  Vec<Rc<RefCell<Symbol>>>{
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
                let fields = parent_object.clone().unwrap().borrow().get_member_symbol(session, &name, from_module.clone(), false, true, true, false).0;
                return fields;
            } else {
                let (symbols, _diagnostics) = parent_object.clone().unwrap().borrow().get_member_symbol(session,
                    &name.to_string(),
                    from_module.clone(),
                    false,
                    true,
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
    ) ->  Vec<Rc<RefCell<Symbol>>>{
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
    ) -> Vec<Rc<RefCell<Symbol>>> {
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
    ) -> Vec<Rc<RefCell<Symbol>>>{
        let mut arg_symbols: Vec<Rc<RefCell<Symbol>>> = vec![];
        let callable_evals = Evaluation::eval_from_ast(session, &call_expr.func, scope.clone(), &call_expr.func.range().start()).0;
        for callable_eval in callable_evals.iter() {
            let callable = callable_eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None);
            let Some(callable_sym) = callable.weak.upgrade() else {
                continue
            };
            if callable_sym.borrow().typ() != SymType::FUNCTION {
                continue;
            }
            let func = callable_sym.borrow();

            // Check if we are in api.onchange/constrains/depends
            let func_sym_tree = func.get_tree();
            if func_sym_tree.0.ends_with(&[Sy!("odoo"), Sy!("api")]){
                if [vec![Sy!("onchange")], vec![Sy!("constrains")]].contains(&func_sym_tree.1) && SyncOdoo::is_in_main_entry(session, &func_sym_tree.0){
                arg_symbols.extend(
                    FeaturesUtils::find_simple_decorator_field_symbol(session, scope.clone(), from_module.clone(), field_name)
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
                callable.context.get(&S!("is_attr_of_instance")).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool());
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
    ) -> Vec<Rc<RefCell<Symbol>>>{
        // We only process the `related` keyword argument
        if keyword.arg.as_ref().filter(|kw_arg| kw_arg.id == "related").is_none(){
            return vec![];
        }
        let mut arg_symbols: Vec<Rc<RefCell<Symbol>>> = vec![];
        let callable_evals = Evaluation::eval_from_ast(session, &call_expr.func, scope.clone(), &call_expr.func.range().start()).0;
        for callable_eval in callable_evals.iter() {
            let callable = callable_eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None);
            let Some(callable_sym) = callable.weak.upgrade() else {
                continue
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
    ) -> Vec<Rc<RefCell<Symbol>>>{
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
            return string_domain_fields_syms;
        }
        let compute_kwarg_syms = FeaturesUtils::find_field_symbols(session, scope.clone(), from_module.clone(),  string_val, call_expr, &offset);
        if compute_kwarg_syms.len() >= 1{
            return compute_kwarg_syms;
        }
        vec![]
    }

    pub fn build_markdown_description(session: &mut SessionInfo, file_symbol: Option<Rc<RefCell<Symbol>>>, evals: &Vec<Evaluation>, call_expr: &Option<ExprCall>, offset: Option<usize>) -> String {
        let mut value = S!("");

        let mut name_counts = HashMap::new();
        for eval in evals.iter() {
            let Some(symbol) = eval.symbol.get_symbol(session, &mut None, &mut vec![], None).upgrade_weak() else {
                continue;
            };
            *name_counts.entry(symbol.borrow().name().clone()).or_insert(0) += 1;
        }

        for (index, eval) in evals.iter().enumerate() {
            if index != 0 {
                value += "  \n***  \n";
            }
            let eval_symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None);
            let Some(symbol) = eval_symbol.upgrade_weak() else {
                // Check if it is UNBOUND
                if let EvaluationSymbolPtr::UNBOUND(name) = eval_symbol{
                    value += format!("```python  \n(variable) {name}: Unbound```").as_str();
                }
                continue;
            };
            //search for a constant evaluation like a model name or domain field
            if let Some(eval_value) = eval.value.as_ref() {
                if let EvaluationValue::CONSTANT(Expr::StringLiteral(expr)) = eval_value {
                    let str = expr.value.to_string();
                    let from_module = file_symbol.as_ref().and_then(|file_symbol| file_symbol.borrow().find_module());
                    if let Some(model) = session.sync_odoo.models.get(&oyarn!("{}", str)).cloned() {
                        let main_classes = model.borrow().get_main_symbols(session, from_module.clone());
                        for main_class_rc in main_classes.iter() {
                            let main_class = main_class_rc.borrow();
                            if let Some(main_class_module) = main_class.find_module() {
                                value += format!("Model in {}: {}  \n", main_class_module.borrow().name(), main_class.name()).as_str();
                                if main_class.doc_string().is_some() {
                                    value = value + "  \n***  \n" + main_class.doc_string().as_ref().unwrap();
                                }
                                value += "  \n***  \n";
                                value += &model.borrow().all_symbols(session, from_module.clone()).into_iter()
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
                                            Some(module) => format!("inherited in {} (require {})  \n", mod_name, module),
                                            None => format!("inherited in {}  \n", mod_name)
                                        }
                                    }).collect::<String>();
                            }
                        }
                        continue;
                    }
                    if let (Some(call_expression), Some(file_sym), Some(offset)) = (call_expr, file_symbol.as_ref(), offset){
                        let special_string_syms = FeaturesUtils::check_for_string_special_syms(session, &str, call_expression, offset, expr.range, file_sym);
                        if special_string_syms.len() >= 1{
                            // restart with replacing current index evaluation with field evaluations
                            let string_domain_fields_evals: Vec<Evaluation> = special_string_syms.iter()
                                .map(|sym| Evaluation::eval_from_symbol(&Rc::downgrade(sym), Some(true)))
                                .chain(evals.iter().take(index).cloned())
                                .chain(evals.iter().skip(index + 1).cloned())
                                .collect();
                            return FeaturesUtils::build_markdown_description(session, file_symbol, &string_domain_fields_evals, call_expr, Some(offset))
                        }
                    }
                }
            }
            // BLOCK 1: (type) **name** -> inferred_type
            let mut context = Some(eval_symbol.as_weak().context.clone());
            let type_refs = Symbol::follow_ref(&eval_symbol, session, &mut context, false, false, None, &mut vec![]);
            value += FeaturesUtils::build_block_1(session, &symbol, &type_refs, &mut context).as_str();
            // BLOCK 2: useful links
            for typ in type_refs.iter() {
                let Some(typ) = typ.upgrade_weak() else {
                    continue;
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
                    value += format!("  \n***  \nSee also: [{}]({}#{})  \n", type_ref.name().as_str(), path.as_str(), range).as_str();
                }
            }
            // BLOCK 3: documentation
            let mut documentation_block = None;
            if *name_counts.get(symbol.borrow().name()).unwrap_or(&0) > 1 {
                if let Some(symbol_module) = symbol.borrow().find_module() {
                    let module_name = symbol_module.borrow().name().clone();
                    documentation_block = Some(format!("From module `{}`", module_name));
                }
            }
            for typ in type_refs.iter() {
                if let Some(typ) = typ.upgrade_weak() {
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
                        .join("  \n");
                        documentation_block = match documentation_block {
                            Some(from_module_str) => Some(from_module_str + "  \n" + &ds),
                            None => Some(ds)
                        };
                    }
                }
            }
            if let Some(documentation_block) = documentation_block{
                value = value + "  \n***  \n" + &documentation_block;
            }
        }
        value
    }

    /*
    Build the first block of the hover. It contains the name of the variable as well as the type.
    parameters:   (type_sym)  symbol: inferred_types
    For example: "(parameter) self: type[Self@ResPartner]"
     */
    fn build_block_1(session: &mut SessionInfo, rc_symbol: &Rc<RefCell<Symbol>>, inferred_types: &Vec<EvaluationSymbolPtr>, context: &mut Option<Context>) -> String {
        let symbol = rc_symbol.borrow();
        //python code balise
        let mut value = S!("```python  \n");
        //type name
        let type_sym = match symbol.typ(){
            SymType::VARIABLE if symbol.as_variable().is_import_variable => S!("import"),
            SymType::VARIABLE if symbol.as_variable().is_parameter => S!("parameter"),
            SymType::FUNCTION if symbol.as_func().is_property => S!("property"),
            SymType::FUNCTION if symbol.parent().unwrap().upgrade().unwrap().borrow().typ() == SymType::CLASS => S!("method"),
            type_ => type_.to_string().to_lowercase()
        };
        value += &format!("({}) ", type_sym);
        //variable name
        let mut single_func_eval = false;
        let mut inferred_types = inferred_types.clone();
        if inferred_types.len() == 1 && inferred_types[0].is_weak() && inferred_types[0].upgrade_weak().unwrap().borrow().typ() == SymType::FUNCTION && !inferred_types[0].upgrade_weak().unwrap().borrow().as_func().is_property {
            //display 'def' only if there is only a single evaluation to a function
            single_func_eval = true;
            let sym_eval_weak = inferred_types[0].as_weak();
            let sym_rc = sym_eval_weak.weak.upgrade().unwrap();
            let sym_ref = sym_rc.borrow();
            let function_sym = sym_ref.as_func();
            let argument_names = function_sym.args.iter().map(|arg| FeaturesUtils::argument_presentation(session, arg)).join(", ");
            value += &format!("def {}({}) -> ", symbol.name(), argument_names);

            let call_parent = match sym_eval_weak.context.get(&S!("base_attr")){
                Some(ContextValue::SYMBOL(s)) => s.clone(),
                _ => {
                    let parent = sym_ref.parent().and_then(|parent_weak| parent_weak.upgrade());
                    if parent.is_some() && parent.as_ref().unwrap().borrow().typ() == SymType::CLASS {
                        Rc::downgrade(&parent.unwrap())
                    } else {
                        Weak::new()
                    }
                }
            };
            // Set base_call to get correct function return type for syms with EvaluationSymbolPtr::SELF type
            context.as_mut().unwrap().insert(S!("base_call"), ContextValue::SYMBOL(call_parent));
            inferred_types = function_sym.evaluations.iter().map(|x| x.symbol.get_symbol_weak_transformed(session, context, &mut vec![], None)).collect();
            context.as_mut().unwrap().remove(&S!("base_call"));
        } else {
            if symbol.typ() == SymType::CLASS && inferred_types.len() == 1 && inferred_types[0].is_weak() && inferred_types[0].as_weak().is_super {
                value += &format!("(super[{}]) ", symbol.name());
            } else {
                value += symbol.name();
            }
            if symbol.typ() != SymType::CLASS {
                value += ": ";
            }
        }
        let values: Vec<String> = inferred_types.iter().map(|inferred_type| {
            FeaturesUtils::get_inferred_types(session, rc_symbol, inferred_type, context, single_func_eval)
        }).unique().collect();

        value += &(FeaturesUtils::print_return_types(values) + "  \n```");
        //end block
        value
    }

    fn print_return_types(values: Vec<String>) -> String {
        let len = values.len();
        let mut filtered_values: Vec<String> = values.into_iter().filter(|v| *v != "Any").collect();
        let found_any = filtered_values.len() != len;
        if found_any {
            filtered_values.push(S!("Any"));
        }
        let result = filtered_values.iter().join(" | ");
        if len > 1 {
            format!("({})", result)
        } else {
            result
        }
    }

    fn argument_presentation(session: &mut SessionInfo, arg: &Argument) -> String {
        let arg_name = arg.symbol.upgrade().unwrap().borrow().name().clone();
        match arg.annotation.as_ref() {
            Some(anno_expr) => {
                let Some(type_symbol) = arg.symbol.upgrade()
                .and_then(|arg_symbol| arg_symbol.borrow().parent())
                .and_then(|weak_parent| weak_parent.upgrade())
                .and_then(|parent| Evaluation::eval_from_ast(session, &anno_expr, parent.clone(), &anno_expr.range().start()).0.first().cloned())
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

    pub fn get_inferred_types(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>, eval: &EvaluationSymbolPtr, context: &mut Option<Context>, single_func_eval: bool) -> String {
        match eval {
            EvaluationSymbolPtr::WEAK(eval_weak) => {
                if let Some(inferred_type) = eval.upgrade_weak() {
                    if Rc::ptr_eq(symbol, &inferred_type) && inferred_type.borrow().typ() != SymType::FUNCTION && inferred_type.borrow().typ() != SymType::CLASS {
                        S!("Any")
                    } else {
                        let inferred_type = inferred_type.borrow();
                        match inferred_type.typ() {
                            SymType::FUNCTION if !inferred_type.as_func().is_property => {
                                let return_type = match inferred_type.evaluations() {
                                    Some(func_eval) => {
                                        let mut type_names = HashSet::new();
                                        for eval in func_eval.iter() {
                                            let eval_symbol = eval.symbol.get_symbol(session, context, &mut vec![], None);
                                            let weak_eval_symbols = Symbol::follow_ref(&eval_symbol, session, context, false, false, None, &mut vec![]);
                                            for weak_eval_symbol in weak_eval_symbols.iter() {
                                                let type_name = if let Some(s_type) = weak_eval_symbol.upgrade_weak() {
                                                    let typ = s_type.borrow();
                                                    //if fct is a variable, it means that evaluation is None.
                                                    if typ.typ() == SymType::VARIABLE {
                                                        "Any".to_string()
                                                    } else {
                                                        typ.name().to_string()
                                                    }
                                                } else {
                                                    "Any".to_string()
                                                };
                                                type_names.insert(type_name);
                                            }
                                        }
                                        if !type_names.is_empty() {type_names.iter().join(" | ")} else {S!("None")}
                                    },
                                    None => S!("Any"),
                                };
                                if !single_func_eval {
                                    let argument_names = inferred_type.as_func().args.iter().map(|arg| FeaturesUtils::argument_presentation(session, arg)).join(", ");
                                    format!("({}) -> {})", argument_names, return_type)
                                } else {
                                    return_type
                                }
                            },
                            SymType::FILE => S!("File"),
                            SymType::PACKAGE(_) => S!("Module"),
                            SymType::NAMESPACE => S!("Namespace"),
                            SymType::CLASS => if eval_weak.is_super {format!("super[{}]", inferred_type.name().to_string())} else {inferred_type.name().to_string()}, // TODO: Maybe do something special if it is a descriptor
                            _ => S!("Any")
                        }
                    }
                } else {
                    S!("Any")
                }
            }
            EvaluationSymbolPtr::ANY => S!("Any"),
            EvaluationSymbolPtr::NONE => S!("None"),
            _ => S!("Any"),
        }
    }


}