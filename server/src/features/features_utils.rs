use itertools::Itertools;
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::{Ranged, TextRange, TextSize};
use crate::core::file_mgr::FileMgr;
use crate::utils::PathSanitizer;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Weak;
use std::{cell::RefCell, rc::Rc};

use crate::constants::SymType;
use crate::core::evaluation::{Context, ContextValue, Evaluation, EvaluationSymbolPtr, EvaluationValue};
use crate::core::symbols::symbol::Symbol;
use crate::threads::SessionInfo;
use crate::S;

pub struct FeaturesUtils {}

impl FeaturesUtils {

    pub fn find_compute_field_symbols(session: &mut SessionInfo, compute_str: &String, call_expr: &ExprCall, offset: usize, file_symbol: &Rc<RefCell<Symbol>>) -> Vec<Rc<RefCell<Symbol>>>{
        let mut compute_syms = vec![];
        let from_module = file_symbol.borrow().find_module();
        let scope = Symbol::get_scope_symbol(file_symbol.clone(), offset as u32, false);
        for arg in call_expr.arguments.keywords.iter() {
            let Some(ref arg_id) = arg.arg else {
                continue;
            };
            if arg_id.as_str() != "compute" {
                continue;
            }
            let callable_evals = Evaluation::eval_from_ast(session, &call_expr.func, scope.clone(), &call_expr.func.range().start()).0;
            for callable_eval in callable_evals.iter() {
                let callable = callable_eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None);
                let Some(callable_sym) = callable.weak.upgrade() else {
                     continue
                };
                if !callable_sym.borrow().is_field_class(session){
                    continue;
                }
                let Some(parent_class) = scope.borrow().get_in_parents(&vec![SymType::CLASS], true).and_then(|p| p.upgrade()) else {
                    continue;
                };
                if parent_class.borrow().as_class_sym()._model.is_none(){
                    continue;
                }
                compute_syms = parent_class.borrow().get_member_symbol(session, compute_str, from_module.clone(), false, false, true, false).0;
            }
        }
        compute_syms
    }

    pub fn find_domain_field_symbols(session: &mut SessionInfo, field_name: &String, call_expr: &ExprCall, offset: usize, field_range: TextRange, file_symbol: &Rc<RefCell<Symbol>>) -> Vec<Rc<RefCell<Symbol>>>{
        let mut string_domain_fields = vec![];
        let from_module = file_symbol.borrow().find_module();
        let scope = Symbol::get_scope_symbol(file_symbol.clone(), offset as u32, false);
        for (arg_index, arg) in call_expr.arguments.args.iter().enumerate() {
            if offset <= arg.range().start().to_usize() || offset > arg.range().end().to_usize() {
                continue;
            }
            let callable_evals = Evaluation::eval_from_ast(session, &call_expr.func, scope.clone(), &call_expr.func.range().start()).0;
            for callable_eval in callable_evals.iter() {
                let callable = callable_eval.symbol.get_symbol_as_weak(session, &mut None, &mut vec![], None);
                let Some(callable_sym) = callable.weak.upgrade() else {
                     continue
                };
                if callable_sym.borrow().typ() != SymType::FUNCTION {
                    continue
                }
                let func = callable_sym.borrow();
                let func_arg = func.as_func().get_indexed_arg_in_call(
                    call_expr,
                    arg_index as u32,
                    callable.context.get(&S!("is_attr_of_instance")).unwrap_or(&ContextValue::BOOLEAN(false)).as_bool());
                let Some(func_arg_sym) = func_arg.and_then(|func_arg| func_arg.symbol.upgrade()) else {
                    continue
                };
                for evaluation in func_arg_sym.borrow().evaluations().unwrap().iter() {
                    if !matches!(evaluation.symbol.get_symbol_ptr(), EvaluationSymbolPtr::DOMAIN){
                        continue;
                    }
                    let Some(mut parent_object) = callable.context.get(&S!("base_attr")).map(|parent_object| parent_object.as_symbol().upgrade()) else {
                        continue;
                    };
                    let mut range_start = field_range.start() + TextSize::new(1);
                    for name in field_name.split(".").map(|x| x.to_string()) {
                        if parent_object.is_none() {
                            break;
                        }
                        let range_end = range_start + TextSize::new((name.len() + 1) as u32);
                        let cursor_section = TextRange::new(range_start, range_end).contains(TextSize::new(offset as u32));
                        if cursor_section {
                            let fields = parent_object.clone().unwrap().borrow().get_member_symbol(session, &name, from_module.clone(), false, true, true, false).0;
                            string_domain_fields.extend(fields);
                            break;
                        } else {
                            let (symbols, _diagnostics) = parent_object.clone().unwrap().borrow().get_member_symbol(session,
                                &name.to_string(),
                                from_module.clone(),
                                false,
                                true,
                                false,
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
                                    }
                                }
                            }
                        }
                        range_start = range_end;
                    }
                }
            }
        }
        string_domain_fields
    }

    fn check_for_string_special_syms(session: &mut SessionInfo, string_val: &String, call_expr: &ExprCall, offset: usize, field_range: TextRange, file_symbol: &Rc<RefCell<Symbol>>) -> Vec<Rc<RefCell<Symbol>>> {
        let string_domain_fields_syms: Vec<Rc<RefCell<Symbol>>> = FeaturesUtils::find_domain_field_symbols(session, string_val, call_expr, offset, field_range, file_symbol);
        if string_domain_fields_syms.len() >= 1 {
            return string_domain_fields_syms;
        }
        let compute_kwarg_syms: Vec<Rc<RefCell<Symbol>>> = FeaturesUtils::find_compute_field_symbols(session, string_val, call_expr, offset, file_symbol);
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
                continue;
            };
            //search for a constant evaluation like a model name or domain field
            if let Some(eval_value) = eval.value.as_ref() {
                if let EvaluationValue::CONSTANT(Expr::StringLiteral(expr)) = eval_value {
                    let str = expr.value.to_string();
                    let from_module = file_symbol.as_ref().and_then(|file_symbol| file_symbol.borrow().find_module());
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
                    if let Some(model) = session.sync_odoo.models.get(&str).cloned() {
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
                    }
                    continue;
                }
            }
            // BLOCK 1: (type) **name** -> infered_type
            let mut context = Some(eval_symbol.as_weak().context.clone());
            let type_refs = Symbol::follow_ref(&eval_symbol, session, &mut context, true, false, None, &mut vec![]);
            value += FeaturesUtils::build_block_1(session, &symbol, &type_refs, &mut context).as_str();
            // BLOCK 2: useful links
            for typ in type_refs.iter() {
                if let Some(typ) = typ.upgrade_weak() {
                    let paths = &typ.borrow().paths();
                    if paths.len() == 1 { //we won't put a link to a namespace
                        let mut base_path = paths.first().unwrap().clone();
                        if matches!(typ.borrow().typ(), SymType::PACKAGE(_)) {
                            base_path = PathBuf::from(base_path).join(format!("__init__.py{}", typ.borrow().as_package().i_ext())).sanitize();
                        }
                        let path = FileMgr::pathname2uri(&base_path);
                        value += "  \n***  \n";
                        let mut range = 0;
                        if typ.borrow().is_file_content() {
                            range = typ.borrow().range().start().to_u32();
                        }
                        value += format!("See also: [{}]({}#{})  \n", typ.borrow().name().as_str(), path.as_str(), range).as_str();
                    }
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
    parameters:   (type_sym)  symbol: infered_types
    For example: "(parameter) self: type[Self@ResPartner]"
     */
    fn build_block_1(session: &mut SessionInfo, rc_symbol: &Rc<RefCell<Symbol>>, infered_types: &Vec<EvaluationSymbolPtr>, context: &mut Option<Context>) -> String {
        let symbol = rc_symbol.borrow();
        //python code balise
        let mut value = S!("```python  \n");
        //type name
        let mut type_sym = symbol.typ().to_string().to_lowercase();
        if symbol.typ() == SymType::VARIABLE && symbol.as_variable().is_import_variable {
            type_sym = S!("import");
        }
        if symbol.typ() == SymType::VARIABLE && symbol.as_variable().is_parameter {
            type_sym = S!("parameter");
        }
        else if symbol.typ() == SymType::FUNCTION {
            if symbol.as_func().is_property {
                type_sym = S!("property");
            }
            else if symbol.parent().unwrap().upgrade().unwrap().borrow().typ() == SymType::CLASS {
                type_sym = S!("method");
            }
        }
        value += &format!("({}) ", type_sym);
        //variable name
        let mut single_func_eval = false;
        let mut infered_types = infered_types.clone();
        if infered_types.len() == 1 && infered_types[0].is_weak() && infered_types[0].upgrade_weak().unwrap().borrow().typ() == SymType::FUNCTION && !infered_types[0].upgrade_weak().unwrap().borrow().as_func().is_property {
            //display 'def' only if there is only a single evaluation to a function
            single_func_eval = true;
            value += "def ";
            value += symbol.name();
            //display args
            let sym_eval_weak = infered_types[0].as_weak();
            let sym_rc = sym_eval_weak.weak.upgrade().unwrap();
            let sym_ref = sym_rc.borrow();
            let function_sym = sym_ref.as_func();
            value += "(";
            let max_index = function_sym.args.len() as i32 - 1;
            for (index, arg) in function_sym.args.iter().enumerate() {
                value += arg.symbol.upgrade().unwrap().borrow().name();
                //TODO add parameter type
                if index != max_index as usize {
                    value += ", ";
                }
            }
            value += ") -> ";
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
            infered_types = function_sym.evaluations.iter().map(|x| x.symbol.get_symbol_weak_transformed(session, context, &mut vec![], None)).collect();
            context.as_mut().unwrap().remove(&S!("base_call"));
        } else {
            if symbol.typ() == SymType::CLASS && infered_types.len() == 1 && infered_types[0].is_weak() && infered_types[0].as_weak().is_super{
                value += &format!("(super[{}]) ", symbol.name());
            } else {
                value += symbol.name();
            }
            if symbol.typ() != SymType::CLASS {
                value += ": ";
            }
        }
        let mut values = vec![];
        for infered_type in infered_types.iter() {
            for v in FeaturesUtils::get_infered_types(session, rc_symbol, infered_type, context, single_func_eval).iter() {
                if !values.contains(v) {
                    values.push(v.clone());
                }
            }
        }
        value += FeaturesUtils::print_return_types(&values).as_str();
        //end block
        value += "  \n```";
        value
    }

    fn print_return_types(values: &Vec<String>) -> String {
        let mut result = S!("");
        if values.len() > 1 {
            result += "(";
        }
        let mut found_any = false;
        for (index, value) in values.iter().enumerate() {
            if value == "Any" {
                found_any = true;
                continue;
            }
            result += value;
            if index != values.len() -1 {
                result += ", ";
            }
        }
        if found_any {
            if values.len() > 1 {
                result += ", ";
            }
            result += "Any";
        }
        if values.len() > 1 {
            result += ")";
        }
        result
    }

    pub fn get_infered_types(session: &mut SessionInfo, symbol: &Rc<RefCell<Symbol>>, eval: &EvaluationSymbolPtr, context: &mut Option<Context>, single_func_eval: bool) -> Vec<String> {
        let mut values = vec![];
        match eval {
            EvaluationSymbolPtr::WEAK(eval_weak) => {
                if let Some(infered_type) = eval.upgrade_weak() {
                    if Rc::ptr_eq(symbol, &infered_type) && infered_type.borrow().typ() != SymType::FUNCTION {
                        if infered_type.borrow().typ() != SymType::CLASS {
                            values.push(S!("Any"));
                        }
                    } else {
                        let infered_type = infered_type.borrow();
                        if infered_type.typ() == SymType::FUNCTION && !infered_type.as_func().is_property {
                            let func_eval = infered_type.evaluations();
                            let mut func_return_type = S!("");
                            if let Some(func_eval) = func_eval {
                                let mut type_names = HashSet::new();
                                for eval in func_eval.iter() {
                                    let eval_symbol = eval.symbol.get_symbol(session, context, &mut vec![], None);
                                    if !eval_symbol.is_expired_if_weak(){ //TODO improve
                                        let weak_eval_symbols = Symbol::follow_ref(&eval_symbol, session, context, true, false, None, &mut vec![]);
                                        for weak_eval_symbol in weak_eval_symbols.iter() {
                                            if let Some(s_type) = weak_eval_symbol.upgrade_weak() {
                                                let typ = s_type.borrow();
                                                if typ.typ() == SymType::VARIABLE {
                                                    //if fct is a variable, it means that evaluation is None.
                                                    type_names.insert("Any".to_string());
                                                } else {
                                                    type_names.insert(typ.name().clone());
                                                }
                                            } else {
                                                type_names.insert("Any".to_string());
                                            }
                                        }
                                    } else {
                                        type_names.insert("None".to_string());
                                    }
                                }
                                let max_eval: i32 = type_names.len() as i32 -1;
                                for (index, type_name) in type_names.iter().enumerate() {
                                    func_return_type += type_name.as_str();
                                    if index != max_eval as usize {
                                        func_return_type += " | ";
                                    }
                                }
                                if type_names.is_empty() {
                                    func_return_type += "None";
                                }
                            }
                            if single_func_eval {
                                values.push(func_return_type);
                            } else {
                                //TODO add args
                                values.push(format!("() -> {}", func_return_type));
                            }
                        } else if infered_type.typ() == SymType::FILE {
                            values.push(S!("File"));
                        } else if matches!(infered_type.typ(), SymType::PACKAGE(_)) {
                            values.push(S!("Module"));
                        } else if infered_type.typ() == SymType::NAMESPACE {
                            values.push(S!("Namespace"));
                        } else if infered_type.typ() == SymType::CLASS {
                            let mut class_type= if infered_type.as_class_sym().is_descriptor() {
                                //TODO actually the same than if not a descriptor. But we could choose to do something else if there is no base_attr
                                S!(infered_type.name())
                            } else {
                                S!(infered_type.name())
                            };
                            if eval_weak.is_super{
                                class_type = format!("super[{}]", class_type);
                            }
                            values.push(class_type);
                        } else {
                            values.push(S!("Any"));
                        }
                    }
                } else {
                    values.push(S!("Any"));
                }
            }
            EvaluationSymbolPtr::ANY => {
                values.push(S!("Any"));
            }
            EvaluationSymbolPtr::NONE => {
                values.push(S!("None"));
            }
            _ => {
                values.push(S!("Any"));
            }
        }

        values
    }


}