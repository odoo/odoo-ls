use std::{cell::RefCell, path::Path, rc::Rc};

use lsp_types::{LocationLink, Range};
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::TextRange;

use crate::constants::PackageType;
use crate::core::symbols::symbol_keys::SourceFileKey;
use crate::features::features_utils::FeaturesUtils;
use crate::{
    constants::{OYarn, SymType},
    core::{
        evaluation::{Evaluation, EvaluationValue, ExprOrIdent},
        file_mgr::{FileInfo, FileMgr},
        odoo::SyncOdoo,
        python_odoo_builder::MAGIC_FIELDS,
        symbols::{
            symbol_keys::SymbolKey,
            storage::SymbolTable,
        }
    },
    features::{
        ast_utils::AstUtils,
        csv_ast_utils::CsvAstUtils,
        xml_ast_utils::XmlAstUtils,
    },
    oyarn,
    threads::SessionInfo,
    utils::PathSanitizer,
    Sy,
};

pub enum GotoRequest {
    Definition,
    Declaration,
}

pub enum GotoSourceType {
    SymbolKey(SymbolKey),
    Location { uri: String, range: TextRange }, // If the source is not expressable with SymbolKey, we can define it with an uri and a range. Mostly usefuul in xml/js
}

pub struct GotoSource {
    pub source: GotoSourceType,
    pub origin_selection_range: Option<Range>,
}

pub struct GotoUtils {}

impl GotoUtils {
    fn check_for_domain_field(session: &mut SessionInfo, eval: &Evaluation, file_symbol: SourceFileKey, call_expr: &Option<ExprCall>, offset: usize, sources: &mut Vec<GotoSource>) -> bool {
        let (field_name, field_range) = if let Some(eval_value) = eval.value.as_ref() {
            if let Some(expr) = eval_value.as_string_literal() {
                (expr.value.to_str(), expr.range)
            } else {
                return false;
            }
        } else {
            return false;
        };
        let Some(call_expr) = call_expr else { return false };
        let module = session.st().find_module(file_symbol);
        let scope = session.st().get_scope_symbol(file_symbol, offset as u32, false);
        let string_domain_fields = FeaturesUtils::find_argument_symbols(
            session, scope, module, field_name, call_expr, offset, field_range
        );
        let mut domain_found = false;
        string_domain_fields.iter().for_each(|(field, field_range)| {
            if session.st().get_file(*field).is_some() {
                domain_found = true;
                let path = session.st().path(file_symbol).to_string();
                sources.push(GotoSource {
                    source: GotoSourceType::SymbolKey(*field),
                    origin_selection_range: Some(session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, field_range))
                });
            }
        });
        domain_found
    }

    fn check_for_model_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: SourceFileKey, sources: &mut Vec<GotoSource>) -> bool {
        let value = if let Some(eval_value) = eval.value.as_ref() {
            if let Some(expr) = eval_value.as_string_literal() {
                oyarn!("{}", expr.value.to_str())
            } else {
                return false;
            }
        } else {
            return  false;
        };
        let model = session.sync_odoo.models.get(&value).cloned();
        let Some(model) = model else {
            return false;
        };
        let mut model_found = false;
        let from_module = session.st().find_module(file_symbol);
        let model_syms = model.borrow().get_model_symbols(session.st(), from_module).collect::<Vec<_>>();
        let len_syms = model_syms.len();
        for sym_key in model_syms.into_iter() {
            if let (Some(eval_range), Some(class_file)) = (eval.range, session.st().get_file(sym_key.into()))
                && (file_symbol == class_file) && session.st().range(sym_key.into()).contains(eval_range.start()) && len_syms > 1
            {
                continue; // if we are already on the class, skip, unless it is the only result
            }
            model_found = true;
            let path = session.st().path(file_symbol).to_string();
            sources.push(GotoSource {
                source: GotoSourceType::SymbolKey(sym_key.into()),
                origin_selection_range: eval.range.map(|r| session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, &r))
            });
        }
        model_found
    }

    fn check_for_module_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: SourceFileKey, file_path: &str, sources: &mut Vec<GotoSource>) -> bool {
        let SourceFileKey::Module(module_key) = file_symbol else {
            return false;
        };
        if !file_path.ends_with("__manifest__.py") {
            // If not on manifest, we don't check for modules
            return false;
        }
        let mut value = if let Some(eval_value) = eval.value.as_ref() {
            if let Some(expr) = eval_value.as_string_literal() {
                oyarn!("{}", expr.value.to_str())
            } else {
                return false;
            }
        } else {
            return false;
        };
        let module_symbol = &session.st()[module_key];
        if value == module_symbol.module_name {
            value = module_symbol.dir_name.clone();
        }
        let Some(module) = session.sync_odoo.modules.get(&value).copied().and_then(|m| m.upgrade(session.st())) else {
            return false;
        };
        sources.push(GotoSource{
            source: GotoSourceType::SymbolKey(module.into()),
            origin_selection_range: None,
        });
        true
    }

    fn check_for_xml_id_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: SourceFileKey, sources: &mut Vec<GotoSource>) -> bool {
        let value = if let Some(eval_value) = eval.value.as_ref() {
            if let Some(expr) = eval_value.as_string_literal() {
                oyarn!("{}", expr.value.to_str())
            } else {
                return false;
            }
        } else {
            return  false;
        };
        let mut xml_found = false;
        let xml_ids = SyncOdoo::get_xml_ids(session, file_symbol, value.as_str(), &std::ops::Range{start: 0, end: 0}, &mut vec![]);
        for xml_id in xml_ids.iter_valid(session.st()) {
            let file = session.st().get_file(xml_id.into());
            if file.is_none() {
                continue;
            }
            xml_found = true;
            let path = session.st().path(file_symbol).to_string();
            sources.push(GotoSource{
                source: GotoSourceType::SymbolKey(xml_id.into()),
                origin_selection_range: eval.range.map(|r| session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, &r))
            });
        }
        xml_found
    }

    fn check_for_compute_string(session: &mut SessionInfo, eval: &Evaluation, file_symbol: SourceFileKey, call_expr: &Option<ExprCall>, offset: usize, sources: &mut Vec<GotoSource>) -> bool {
        let value = if let Some(eval_value) = eval.value.as_ref() {
            if let Some(expr) = eval_value.as_string_literal() {
                expr.value.to_str()
            } else {
                return false;
            }
        } else {
            return  false;
        };
        let Some(call_expr) = call_expr else { return false };
        let scope = session.st().get_scope_symbol(file_symbol, offset as u32, false);
        let module = session.st().find_module(file_symbol);
        let method_symbols = FeaturesUtils::find_kwarg_methods_symbols(
            session, scope, module, value, call_expr, &offset
        );
        let mut method_found = false;
        method_symbols.iter().for_each(|&field|{
            if let Some(file_sym) = session.st().get_file(field) {
                method_found = true;
                let path = session.st().path(file_sym).to_string();
                sources.push(GotoSource {
                    source: GotoSourceType::SymbolKey(field),
                    origin_selection_range: eval.range.map(|r| session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, &r))
                });
            }
        });
        method_found
    }

    pub fn add_display_name_compute_methods(session: &mut SessionInfo, sources: &mut Vec<GotoSource>, expr: &ExprOrIdent, file_symbol: SourceFileKey, offset: usize) {
        // now we want `_compute_display_name` definition(s)
        // we need the symbol of the model/ then we run get member symbol
        // to do that, we need the expr, match it to attribute, get the value, get its evals
        // with those evals, we run get_member_symbol on `_compute_display_name`
        let ExprOrIdent::Expr(Expr::Attribute(attr_expr)) = expr else {
            return;
        };
        let (analyse_ast_result, _range) = AstUtils::get_symbol_from_expr(session, file_symbol, &ExprOrIdent::Expr(&attr_expr.value), offset as u32);
        let eval_ptrs = analyse_ast_result.evaluations.iter().flat_map(|eval| SymbolTable::follow_ref(eval.symbol.get_symbol_ptr(), session, None, false, false, None, None)).collect::<Vec<_>>();
        let maybe_module = session.st().find_module(file_symbol);
        let symbols = eval_ptrs.iter().flat_map(|eval_ptr| {
            let Some(symbol) = eval_ptr.upgrade_weak(session.st()) else {
                return  vec![];
            };
            SymbolTable::get_member_symbol(session, symbol, "_compute_display_name", maybe_module, false, false, true, true, false).0
        }).collect::<Vec<_>>();
        for symbol in symbols {
            sources.push(GotoSource {
                source: GotoSourceType::SymbolKey(symbol),
                origin_selection_range: None,
            });
        }
    }

    pub fn get_symbols(session: &mut SessionInfo,
        goto_request: GotoRequest,
        file_symbol: SourceFileKey,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Vec<GotoSource> {
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let file_info_ast_clone = file_info.borrow().file_info_ast.clone();
        let file_info_ast_ref = file_info_ast_clone.borrow();
        let (analyse_ast_result, _range, expr, call_expr) = AstUtils::get_symbols(session, &file_info_ast_ref, file_symbol, offset as u32);
        if analyse_ast_result.evaluations.is_empty() {
            return vec![];
        }
        let mut definition_sources = vec![];
        let mut evaluations = analyse_ast_result.evaluations;
        // Filter out magic fields
        let mut dislay_name_found = false;
        evaluations.retain(|eval| {
            // Filter out, variables, whose parents are a class, whose name is one of the magic fields, and have the same range as their parent
            let eval_sym = eval.symbol.get_symbol(session, None, &mut vec![], None);
            let Some(eval_sym) = eval_sym.upgrade_weak(session.st()) else { return true; };
            let SymbolKey::Variable(variable_key) = eval_sym else {
                return true;
            };
            let eval_sym_name = session.st()[variable_key].name.clone();
            if !MAGIC_FIELDS.contains(&eval_sym_name.as_str()) || !SymbolTable::is_field(session, eval_sym) {
                return true;
            }
            if eval_sym_name == "display_name" {
                dislay_name_found = true;
            }
            let Some(parent_sym) = session.st().parent(eval_sym) else { return true; };
            let SymbolKey::Class(class_key) = parent_sym else {
                return true;
            };
            let st: &SymbolTable = session.st();
            st[variable_key].range != st[class_key].range
        });
        if let Some(expr) = expr && dislay_name_found {
            GotoUtils::add_display_name_compute_methods(session, &mut definition_sources, &expr, file_symbol, offset);
        }
        drop(file_info_ast_ref);
        let mut index = 0;
        while index < evaluations.len() {
            let eval = &evaluations[index];
            if GotoUtils::check_for_domain_field(session, eval, file_symbol, &call_expr, offset, &mut definition_sources) ||
              GotoUtils::check_for_compute_string(session, eval, file_symbol,&call_expr, offset, &mut definition_sources) ||
              GotoUtils::check_for_module_string(session, eval, file_symbol, &file_info.borrow().uri, &mut definition_sources) ||
              GotoUtils::check_for_model_string(session, eval, file_symbol, &mut definition_sources) ||
              GotoUtils::check_for_xml_id_string(session, eval, file_symbol, &mut definition_sources) {
                index += 1;
                continue;
            }
            if matches!(eval.value, Some(EvaluationValue::CONSTANT(_))) {
                // Skip go to definition on literals
                index += 1;
                continue;
            }
            if matches!(goto_request, GotoRequest::Definition) {
                let eval_ptr = eval.symbol.get_symbol(session, None, &mut vec![], None);
                let end_symbols = SymbolTable::follow_imported_ref(&eval_ptr, session, None);
                for end_symbol in end_symbols.iter() {
                    if let Some(symbol) = end_symbol.upgrade_weak(session.st()) {
                        definition_sources.push(GotoSource{
                            source: GotoSourceType::SymbolKey(symbol),
                            origin_selection_range: None
                        });
                    }
                }
            } else {
                let Some(symbol) = eval.symbol.get_symbol_as_weak(session, None, &mut vec![], None).weak.upgrade(session.st()) else {
                    index += 1;
                    continue;
                };
                definition_sources.push(GotoSource{
                    source: GotoSourceType::SymbolKey(symbol),
                    origin_selection_range: None
                });
            }
            index += 1;
        }
        definition_sources
    }

    pub fn get_symbols_xml(session: &mut SessionInfo,
        file_symbol: SourceFileKey,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Vec<GotoSource> {
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
        let document = roxmltree::Document::parse(&data);
        let mut sources = vec![];
        if let Ok(document) = document {
            let root = document.root_element();
            let (xml_ast_results, origin_range) = XmlAstUtils::get_symbols(session, file_symbol, root, offset, true);
            for xml_ast_result in xml_ast_results {
                sources.push({
                    let path = session.sync_odoo.symbol_table.path(file_symbol).to_string();
                    GotoSource {
                        source: GotoSourceType::SymbolKey(xml_ast_result),
                        origin_selection_range: Some(session.sync_odoo.get_file_mgr().borrow().std_range_to_range(session, &path, origin_range.as_ref().unwrap()))
                    }
                });
            }
        }
        sources
    }

    pub fn get_symbols_csv(session: &mut SessionInfo,
        file_symbol: SourceFileKey,
        file_info: &Rc<RefCell<FileInfo>>,
        line: u32,
        character: u32
    ) -> Vec<GotoSource> {
        let model_name_pb = Path::new(session.st().path(file_symbol));
        let model_name = Sy!(model_name_pb.file_stem().unwrap().to_str().unwrap().to_string());
        let offset = file_info.borrow().position_to_offset(line, character, session.sync_odoo.encoding);
        let data = file_info.borrow().file_info_ast.borrow().text_document.as_ref().unwrap().contents().to_string();
        let mut csv_reader = csv::ReaderBuilder::new().quoting(true).from_reader(data.as_bytes());

        CsvAstUtils::get_symbols(session, file_symbol.unwrap_csv_file_key(), &mut csv_reader, &model_name, offset, &data)
    }

    pub fn goto_source_to_location(session: &mut SessionInfo, def: &GotoSource) -> Vec<LocationLink> {
        let source = &def.source;
        match source {
            GotoSourceType::SymbolKey(symbol_key) => {
                let (path, range) = match symbol_key.typ() {
                    SymType::PACKAGE(PackageType::MODULE) => {
                        (
                            Some(Path::new(&session.st().path(symbol_key.as_source_file_key().unwrap())).join("__manifest__.py").sanitize()),
                            Range::default()
                        )
                    },
                    _ => {
                        if let Some(file) = session.st().get_file(*symbol_key) {
                            let path = session.st().path(file).to_string();
                            let range = if session.st().has_range(*symbol_key) {
                                let range = *session.st().range(*symbol_key);
                                session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, &path, &range)
                            } else {
                                Range::default()
                            };
                            (
                                Some(path),
                                range
                            )
                        } else {
                            (None, Range::default())
                        }
                    }
                };
                let Some(path) = path else {
                    return vec![];
                };
                vec![LocationLink{
                    origin_selection_range: def.origin_selection_range,
                    target_uri: FileMgr::pathname2uri(&path),
                    target_selection_range: range,
                    target_range: range,
                }]
            },
            GotoSourceType::Location { uri, range } => {
                vec![LocationLink{
                    origin_selection_range: def.origin_selection_range,
                    target_uri: FileMgr::pathname2uri(uri),
                    target_selection_range: session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, uri, range),
                    target_range: session.sync_odoo.get_file_mgr().borrow().text_range_to_range(session, uri, range),
                }]
            }
        }
    }
}
