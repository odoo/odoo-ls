use itertools::Itertools;
use crate::core::file_mgr::FileMgr;
use crate::core::odoo::SyncOdoo;
use crate::core::symbols::symbol_keys::{ModelSymbolKey, ModuleKey, SourceFileKey, SymbolKey, XmlId, XmlRecordKey};
use crate::core::symbols::storage::SymbolTable;
use crate::features::ast_utils::StringContext;
use crate::utils::HashMap;
use std::path::Path;

use crate::constants::SymType;
use crate::constants::OYarn;
use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr, EvaluationValue};
use crate::core::evaluation_context::{ContextKey, ContextValue};
use crate::threads::SessionInfo;
use crate::S;

use super::string_resolution::{resolve_string_in_context, SegmentPick, StringResolution};
use super::type_info::{get_inferred_types, represent_return_types, CallableSignature, TypeInfo};

#[derive(Clone)]
struct InferredType {
    eval_ptr: EvaluationSymbolPtr,
    eval_info: TypeInfo,
}

pub fn build_markdown_description(
    session: &mut SessionInfo,
    file_symbol: Option<SourceFileKey>,
    file_path: Option<&str>,
    evals: &[Evaluation],
    string_ctx: &Option<StringContext>,
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
            if let (Some(string_ctx_val), Some(file_sym), Some(offset)) = (string_ctx, file_symbol, offset){
                // A model is described below, as any model name
                let special_string_syms: Vec<SymbolKey> = match resolve_string_in_context(session, file_sym, expr, string_ctx_val, SegmentPick::Cursor(offset)) {
                    Some(StringResolution::Members(members)) => members.into_iter().map(|(sym, _)| sym).collect(),
                    _ => vec![],
                };
                // Inject `base_attr` to get descriptor type on follow_ref in features
                if !special_string_syms.is_empty() {
                    inject_base_attr(session, &special_string_syms);
                    // restart with replacing current index evaluation with field evaluations
                    let st = &session.sync_odoo.symbol_table;
                    let string_domain_fields_evals: Vec<Evaluation> = special_string_syms.iter()
                        .map(|&sym| Evaluation::eval_from_symbol(st, sym, Some(true)))
                        .chain(evals.iter().take(index).cloned())
                        .chain(evals.iter().skip(index + 1).cloned())
                        .collect();
                    let r = build_markdown_description(session, file_symbol, file_path, &string_domain_fields_evals, string_ctx, Some(offset));
                    // remove the injected `base_attr` context value
                    remove_base_attr(session, &special_string_syms);
                    return r;
                }
            }
            let mut string_handled = false;
            if let Some(model) = session.model_mgr().get_model_key(str) {
                let model_main_syms = session.model_mgr()[model].get_main_symbols(session, from_module);
                let mut block = S!("");
                for model_main_sym in model_main_syms {
                    match model_main_sym {
                        ModelSymbolKey::Class(class_key) => {
                            let model_sym: SymbolKey = class_key.into();
                            let st = &session.sync_odoo.symbol_table;
                            if let Some(main_class_module) = st.find_module(model_sym) {
                                block += format!("Model in {}: {}", st[main_class_module].name, st.repr(model_sym)).as_str();
                                if let Some(doc_string) = st.doc_string(model_sym) {
                                    block = block + "  \n***  \n" + doc_string;
                                }
                                block += "  \n***  \n";
                                block += &session.model_mgr()[model].all_model_classes_dependencies(session, from_module)
                                    .filter_map(|(sym, needed_module)| {
                                        if model_sym == sym {
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
                                            Some(module) => format!("inherited in {} (require {}){}", mod_name, module, get_line_break(session)),
                                            None => format!("inherited in {}{}", mod_name, get_line_break(session))
                                        }
                                    }).collect::<String>();
                            }
                        },
                            ModelSymbolKey::XmlRecord(xml_key) => {
                            if let Some(xml_block) = format_xml_record_block(session, xml_key) {
                                blocks.push(xml_block);
                            }
                        }
                    }
                }
                if !block.is_empty() {
                    blocks.push(block);
                }
                string_handled = true;
            }
            if let Some(file_sym) = file_symbol {
                let xml_ids = SyncOdoo::get_xml_ids(session, file_sym, str, &std::ops::Range { start: 0, end: 0 }, &mut vec![]);
                for xml_id in xml_ids.iter_valid(session.st()) {
                    if let XmlId::XmlRecord(record_key) = xml_id
                        && let Some(xml_block) = format_xml_record_block(session, record_key) {
                            blocks.push(xml_block);
                            string_handled = true;
                        }
                }
            }
            if string_handled {
                continue;
            }
        }
        let eval_symbol = eval.symbol.get_symbol(session, None, &mut vec![], None);
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
        if let SymbolKey::XmlRecord(xml_key) = symbol {
            if let Some(block) = format_xml_record_block(session, xml_key) {
                blocks.push(block);
            }
            continue;
        }
        let context = &eval_symbol.get_weak().context;
        let evaluation_ptrs = SymbolTable::follow_ref(&eval_symbol, session, Some(context), false, false, None, None);
        let symbol_type = symbol.typ();
        let symbol_name = session.st().name(symbol).clone();
        let from_module = session.st().find_module(symbol);
        let sym_type_tag = get_type_symbol_tag(&session.sync_odoo.symbol_table, symbol);
        let return_types: Vec<TypeInfo> = evaluation_ptrs.iter().map(|eval| get_inferred_types(session, eval, Some(context), &symbol_type)).unique().collect();
        let inferred_types = evaluation_ptrs.into_iter().zip(return_types).map(|(eval_ptr, eval_info)| InferredType{eval_ptr, eval_info}).collect();

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
        block += build_block_1(session, id.type_, &id.name, sym_type_tag, &inferred_types).as_str();
        // BLOCK 2: documentation
        if let Some(documentation_block) = get_documentation_block(session, &from_modules, &inferred_types){
            block = block + "  \n***  \n" + &documentation_block;
        }
        // BLOCK 3: useful links or directory paths
        block += inferred_types.iter().map(|typ| get_location_info(session, &typ.eval_ptr)).collect::<String>().as_str();
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
        SymbolKey::XmlRecord(_) => S!("XML record"),
        type_ => type_.typ().to_string().to_lowercase()
    }
}

/*
Build the first block of the hover. It contains the name of the variable as well as the type.
parameters:   (type_sym)  symbol: inferred_types
For example: "(parameter) self: type[Self@ResPartner]"
    */
fn build_block_1(session: &mut SessionInfo, symbol_type: SymType, symbol_name: &OYarn, type_sym: Vec<String>, inferred_types: &[InferredType]) -> String {
    //python code balise
    let mut value = S!(format!("```python{}", get_line_break(session)));
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
    value += &format!("{}{}```", represent_return_types(return_types_string), "  \n"); //No need to put an escaped \n in code bloc
    //end block
    value
}

/// Finds and returns useful links or directory locations for an evaluation
fn get_location_info(session: &SessionInfo, typ: &EvaluationSymbolPtr) -> String {
    let lb = get_line_break(session);
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
fn get_documentation_block(session: &SessionInfo, from_modules: &[OYarn], type_refs: &[InferredType]    ) -> Option<String> {
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
            let doc = get_doc_string(symbol_table, sym);
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
                .join(get_line_break(session));
                documentation_block = match documentation_block {
                    Some(from_module_str) => Some(from_module_str + get_line_break(session) + &ds),
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

fn get_line_break(session: &SessionInfo) -> &'static str {
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
            if let EvaluationSymbolPtr::WEAK(weak) = eval.symbol.get_mut_symbol_ptr()
                && let Some(field_parent) = weak.context.get(ContextKey::FieldParent).cloned()
                    && !weak.context.contains_key(&ContextKey::BaseAttr) {
                        weak.context.insert(ContextKey::BaseAttr, field_parent);
                        weak.context.insert(ContextKey::BaseAttrInserted, ContextValue::BOOLEAN(true));
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
            if let EvaluationSymbolPtr::WEAK(weak) = eval.symbol.get_mut_symbol_ptr()
                && let Some(ContextValue::BOOLEAN(true)) = weak.context.get(ContextKey::BaseAttrInserted) {
                    weak.context.remove(ContextKey::BaseAttrInserted);
                    weak.context.remove(ContextKey::BaseAttr);
                }
        }
    }
}

fn format_xml_record_block(session: &SessionInfo, xml_record_key: XmlRecordKey) -> Option<String> {
    let st = &session.sync_odoo.symbol_table;
    let record = &st[xml_record_key];
    let local_id = record.xml_id.as_ref()?;
    let module_name = st.find_module(xml_record_key)
        .map(|mk| st[mk].name.clone())
        .unwrap_or_default();
    let full_xml_id = if module_name.is_empty() {
        local_id.to_string()
    } else {
        format!("{}.{}", module_name, local_id)
    };
    let file_name = st.get_file(xml_record_key.into())
        .map(|f| {
            let path = st.file_path(f);
            Path::new(path).file_name().unwrap_or_default().to_str().unwrap_or_default().to_string()
        })
        .unwrap_or_default();
    Some(format!("```\n(XML record) {}\nmodel: {}\nfile: {}\n```", full_xml_id, record.model.0, file_name))
}
