use ruff_text_size::TextRange;
use lsp_types::{Hover, HoverContents, MarkupContent, Range};
use weak_table::traits::WeakElement;
use crate::core::evaluation::{AnalyzeAstResult, Context, Evaluation, EvaluationSymbolWeak};
use crate::core::file_mgr::{FileInfo, FileMgr};
use crate::threads::SessionInfo;
use crate::utils::PathSanitizer as _;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use crate::core::symbols::symbol::Symbol;
use crate::constants::*;
use crate::features::ast_utils::AstUtils;
use crate::S;
use std::cell::RefCell;

pub struct HoverFeature {}

impl HoverFeature {

    pub fn get_hover(session: &mut SessionInfo, file_symbol: &Rc<RefCell<Symbol>>, file_info: &Rc<RefCell<FileInfo>>, line: u32, character: u32) -> Option<Hover> {
        let offset = file_info.borrow().position_to_offset(line, character);
        let (analyse_ast_result, range): (AnalyzeAstResult, Option<TextRange>) = AstUtils::get_symbols(session, file_symbol, file_info, offset as u32);
        let evals = analyse_ast_result.evaluations;
        if evals.is_empty() {
            return None;
        };
        let range = Some(Range {
            start: file_info.borrow().offset_to_position(range.unwrap().start().to_usize()),
            end: file_info.borrow().offset_to_position(range.unwrap().end().to_usize())
        });
        return Some(Hover { contents:
            HoverContents::Markup(MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: HoverFeature::build_markdown_description(session, &evals)
            }),
            range: range
        });
    }

    /*
    Build the first block of the hover. It contains the name of the variable as well as the type.
    parameters:   (type_sym)  symbol: infered_types
    For example: "(parameter) self: type[Self@ResPartner]"
     */
    fn build_block_1(session: &mut SessionInfo, rc_symbol: &Rc<RefCell<Symbol>>, infered_types: &Vec<EvaluationSymbolWeak>, context: &mut Option<Context>) -> String {
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
        if infered_types.len() == 1 && infered_types[0].weak.upgrade().unwrap().borrow().typ() == SymType::FUNCTION && !infered_types[0].weak.upgrade().unwrap().borrow().as_func().is_property {
            //display 'def' only if there is only a single evaluation to a function
            single_func_eval = true;
            value += "def ";
            value += &symbol.name();
            //display args
            let function = infered_types[0].weak.upgrade().unwrap();
            let function = function.borrow();
            let function = function.as_func();
            value += "(";
            let max_index = function.args.len() as i32 - 1;
            for (index, arg) in function.args.iter().enumerate() {
                value += arg.symbol.upgrade().unwrap().borrow().name();
                //TODO add parameter type
                if index != max_index as usize {
                    value += ", ";
                }
            }
            value += ") -> "
        } else {
            value += &symbol.name();
            if symbol.typ() != SymType::CLASS {
                value += ": ";
            }
        }
        let max_index = infered_types.len() as i32 -1;
        if max_index != 0 {
            value += "(";
        }
        for (index, infered_type) in infered_types.iter().enumerate() {
            let infered_type = infered_type.weak.upgrade();
            if let Some(infered_type) = infered_type {
                if Rc::ptr_eq(rc_symbol, &infered_type) && infered_type.borrow().typ() != SymType::FUNCTION {
                    if infered_type.borrow().typ() != SymType::CLASS {
                        value += "Any";
                    }
                } else {
                    let infered_type = infered_type.borrow();
                    if infered_type.typ() == SymType::FUNCTION && !infered_type.as_func().is_property {
                        let func_eval = infered_type.evaluations();
                        let mut func_return_type = S!("");
                        if let Some(func_eval) = func_eval {
                            let mut type_names = HashSet::new();
                            for eval in func_eval.iter() {
                                let s = eval.symbol.get_symbol(session, context, &mut vec![], None).weak;
                                if let Some(s) = s.upgrade() {
                                    let weak_eval_symbols = Symbol::follow_ref(&s, session, context, true, false, None, &mut vec![]);
                                    for weak_eval_symbol in weak_eval_symbols.iter() {
                                        if let Some(s_type) = weak_eval_symbol.weak.upgrade() {
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
                            if type_names.len() == 0 {
                                func_return_type += "None";
                            }
                        }
                        if single_func_eval {
                            value += func_return_type.as_str();
                        } else {
                            //TODO add args
                            value += format!("() -> {}", func_return_type).as_str();
                        }
                    } else if infered_type.typ() == SymType::FILE {
                        value += "File";
                    } else if infered_type.typ() == SymType::PACKAGE {
                        value += "Module";
                    } else if infered_type.typ() == SymType::NAMESPACE {
                        value += "Namespace";
                    } else if symbol.typ() != SymType::CLASS {
                        value += &infered_type.name();
                    }
                }
            }
            if index != max_index as usize {
                value += ", ";
            }
        }
        if max_index != 0 {
            value += ")";
        }
        //end block
        value += "  \n```";
        value
    }

    pub fn build_markdown_description(session: &mut SessionInfo, evals: &Vec<Evaluation>) -> String {
        //let eval = &evals[0]; //TODO handle more evaluations
        let mut value = S!("");
        for (index, eval) in evals.iter().enumerate() {
            if index != 0 {
                value += "  \n***  \n";
            }
            let symbol = eval.symbol.get_symbol(session, &mut None, &mut vec![], None).weak;
            if symbol.is_expired() {
                continue;
            }
            let symbol = symbol.upgrade().unwrap();
            let mut context = Some(eval.symbol.context.clone());
            let type_refs = Symbol::follow_ref(&symbol, session, &mut context, true, false, None, &mut vec![]);
            // BLOCK 1: (type) **name** -> infered_type
            value += HoverFeature::build_block_1(session, &symbol, &type_refs, &mut context).as_str();
            // BLOCK 2: useful links
            for typ in type_refs.iter() {
                let typ = typ.weak.upgrade();
                if let Some(typ) = typ {
                    let paths = &typ.borrow().paths();
                    if paths.len() == 1 { //we won't put a link to a namespace
                        let mut base_path = paths.first().unwrap().clone();
                        if typ.borrow().typ() == SymType::PACKAGE {
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
            for typ in type_refs.iter() {
                let typ = typ.weak.upgrade();
                if let Some(typ) = typ {
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
                        .join("\n\n");
                        value = value + "  \n***  \n" + &ds;
                    }
                }
            }
        }
        value
    }
}