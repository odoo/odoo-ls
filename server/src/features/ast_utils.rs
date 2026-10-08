use crate::constants::{BuildSteps, SymType};
use crate::core::build_scheduler::BuildScheduler;
use crate::core::evaluation::{AnalyzeAstResult, Evaluation, ExprOrIdent};
use crate::core::evaluation_context::{Context, ContextKey, ContextValue};
use crate::core::symbols::symbol_keys::{SourceFileKey, SymbolKey};
use crate::core::import_resolver::{resolve_from_stmt, resolve_import_stmt};
use crate::core::file_mgr::FileInfoAst;
use crate::threads::SessionInfo;
use crate::S;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt, walk_alias, walk_except_handler, walk_parameter, walk_keyword, walk_pattern_keyword, walk_type_param, walk_pattern};
use ruff_python_ast::{Alias, AtomicNodeIndex, ExceptHandler, Expr, ExprCall, ExprStringLiteral, Identifier, Keyword, Parameter, Pattern, PatternKeyword, Stmt, StringLiteral, TypeParam};
use ruff_text_size::{Ranged, TextRange, TextSize};
use tracing::warn;

pub struct AstUtils {}

impl AstUtils {


    pub fn get_symbols<'a>(session: &mut SessionInfo, file_info_ast: &'a FileInfoAst, file_symbol: SourceFileKey, offset: u32) -> (AnalyzeAstResult, Option<TextRange>, Option<ExprOrIdent<'a>>, Option<StringContext>) {
        let mut expr: Option<ExprOrIdent<'a>> = None;
        let mut string_ctx: Option<StringContext> = None;
        for stmt in file_info_ast.get_stmts().unwrap().iter() {
            //we have to handle imports differently as symbols are not visible in file.
            if let Some((result, range)) = Self::get_symbol_in_import(session, file_symbol, offset, stmt) {
                return (result, range, None, None);
            }
            (expr, string_ctx) = ExprFinderVisitor::find_expr_at(stmt, offset);
            if expr.is_some() {
                break;
            }
        }
        let Some(expr) = expr else {
            warn!("expr not found");
            return (AnalyzeAstResult::default(), None, None, None);
        };
        let (result, range) = Self::get_symbol_from_expr(session, file_symbol, &expr, offset);
        (result, range, Some(expr), string_ctx)
    }

    pub fn get_symbol_from_expr<'a>(session: &mut SessionInfo, file_symbol: SourceFileKey, expr: &ExprOrIdent<'a>, offset: u32) -> (AnalyzeAstResult, Option<TextRange>) {
        let parent_symbol = session.st().get_scope_symbol(file_symbol, offset, matches!(expr, ExprOrIdent::Parameter(_)));
        AstUtils::build_scope(session, parent_symbol);
        let from_module;
        if let Some(module) = session.st().find_module(file_symbol) {
            from_module = ContextValue::MODULE(module.into());
        } else {
            from_module = ContextValue::BOOLEAN(false);
        }
        let mut context = Context::from_iter([
            (ContextKey::Module, from_module),
            (ContextKey::Range, ContextValue::RANGE(expr.range()))
        ]);
        let analyse_ast_result: AnalyzeAstResult = Evaluation::analyze_ast(session, expr, parent_symbol, &expr.range().end(), &mut context, false, &mut vec![]);
        (analyse_ast_result, Some(expr.range()))
    }

    pub fn flatten_expr(expr: &Expr) -> String {
        match expr {
            Expr::Name(n) => {
                n.id.to_string()
            },
            Expr::Attribute(a) => {
                AstUtils::flatten_expr(&a.value) + &a.attr
            },
            _ => {S!("//Unhandled//")}
        }
    }

    pub fn build_scope(session: &mut SessionInfo<'_>, scope: SymbolKey) {
        let SymbolKey::Function(scope) = scope else {
            return;
        };
        let parent_func = session.st().get_in_parents(scope.into(), &[SymType::FUNCTION], true);
        let scope_to_test = parent_func.map(|p| p.unwrap_function_key()).unwrap_or(scope);
        BuildScheduler::build_now(session, scope_to_test, BuildSteps::ARCH);
        BuildScheduler::build_now(session, scope_to_test, BuildSteps::ARCH_EVAL);
    }

    /// Index in the string of the character at `offset` in the file, e.g. the cursor.
    ///
    /// `None` when `offset` is not inside the text of the string (on a quote, or between the
    /// parts of `'a' 'b'`), or when that text contains an escape like `\t`: the string then
    /// differs from what is written in the file.
    pub fn index_in_string(string: &ExprStringLiteral, offset: TextSize) -> Option<usize> {
        let mut index = 0;
        for part in string.value.iter() {
            let text = part.content_range();
            if text.contains_inclusive(offset) && Self::is_written_as_is(part) {
                return Some(index + (offset - text.start()).to_usize());
            }
            index += part.as_str().len();
        }
        None
    }

    /// Range in the file of `string[start..end]`, e.g. a field in a dotted path.
    ///
    /// `None` when that piece is split between the parts of `'a' 'b'`, or when its part contains
    /// an escape like `\t`.
    pub fn range_in_file(string: &ExprStringLiteral, start: usize, end: usize) -> Option<TextRange> {
        let mut index = 0;
        for part in string.value.iter() {
            let len = part.as_str().len();
            if index <= start && end <= index + len && Self::is_written_as_is(part) {
                let text_start = part.content_range().start();
                return Some(TextRange::new(text_start + TextSize::new((start - index) as u32), text_start + TextSize::new((end - index) as u32)));
            }
            index += len;
        }
        None
    }

    /// Whether `part` holds exactly the text written between its quotes. Escapes like `\t` are
    /// replaced by the character they stand for, which always makes the string shorter.
    fn is_written_as_is(part: &StringLiteral) -> bool {
        part.content_range().len().to_usize() == part.as_str().len()
    }

    /// Index in the name of the character at `offset` in the file, e.g. the cursor.
    ///
    /// `None` when `offset` is outside the name, or when the name is written differently in the
    /// file: Python normalizes some unicode letters, `import ｏｓ` imports `os`.
    pub fn index_in_name(name: &Identifier, offset: TextSize) -> Option<usize> {
        let range = name.range();
        if !range.contains_inclusive(offset) || range.len().to_usize() != name.id.len() {
            return None;
        }
        let index = (offset - range.start()).to_usize();
        name.id.is_char_boundary(index).then_some(index)
    }

    /// Dotted name up to the end of the segment under `offset`, with the range of that segment
    /// in the file. `None` on the last segment, or when the cursor is not found in the name.
    fn dotted_prefix_at(name: &Identifier, offset: u32) -> Option<(&str, TextRange)> {
        let cursor = Self::index_in_name(name, TextSize::new(offset))?;
        let end = cursor + name.id[cursor..].find('.')?;
        let text = &name.id[..end];
        let segment_start = text.rfind('.').map_or(0, |dot| dot + 1);
        let start = name.range().start();
        Some((text, TextRange::new(start + TextSize::new(segment_start as u32), start + TextSize::new(end as u32))))
    }

    fn get_symbol_in_import(session: &mut SessionInfo, file_symbol: SourceFileKey, offset: u32, stmt: &Stmt) -> Option<(AnalyzeAstResult, Option<TextRange>)> {
        match stmt {
            //for all imports, the idea will be to check if we are on the last name of the import (then it has been imported already and we can fallback on it),
            //or then take the full tree to the offset symbol and resolve_import on it as it was in a 'from' clause.
            Stmt::Import(stmt) => {
                for alias in stmt.names.iter() {
                    if alias.range().contains(TextSize::new(offset)) {
                        let mut is_last = false;
                        let (to_analyze, range) = if alias.name.range().contains(TextSize::new(offset)) {
                            if let Some(prefix) = Self::dotted_prefix_at(&alias.name, offset) {
                                prefix
                            } else {
                                is_last = true;
                                (alias.name.id.as_str(), alias.name.range())
                            }
                        } else if alias.asname.is_some() && alias.asname.as_ref().unwrap().range().contains(TextSize::new(offset)) {
                            is_last = true;
                            (alias.asname.as_ref().unwrap().id.as_str(), alias.asname.as_ref().unwrap().range())
                        } else {
                            return None;
                        };
                        if !is_last {
                            //we import as a from_stmt, to refuse import of variables, as the import stmt is not complete
                            let to_analyze = Identifier { id: Name::new(to_analyze), range: TextRange::default(), node_index: AtomicNodeIndex::default() };
                            let (from_symbol, _fallback_sym, _file_tree) = resolve_from_stmt(session, file_symbol.into(), Some(&to_analyze), 0);
                            if let Some(symbols) = from_symbol {
                                let st = &session.sync_odoo.symbol_table;
                                let result = AnalyzeAstResult {
                                    evaluations: symbols.iter().map(|&symbol| Evaluation::eval_from_symbol(st, symbol, None)).collect(),
                                    diagnostics: vec![],
                                };
                                return Some((result, Some(range)));
                            }
                        } else {
                            let res = resolve_import_stmt(session, file_symbol.into(), None, &[
                                Alias { //create a dummy alias with a asname to force full import
                                    name: Identifier { id: Name::new(to_analyze), range: TextRange::default(), node_index: AtomicNodeIndex::default() },
                                    asname: Some(Identifier { id: Name::new("fake_name"), range: alias.name.range(), node_index: AtomicNodeIndex::default() }),
                                    range: alias.range(),
                                    node_index: AtomicNodeIndex::default()
                                }], 0, &mut None);
                            let res = res.into_iter().filter(|s| s.found).collect::<Vec<_>>();
                            if !res.is_empty() {
                                let st = &session.sync_odoo.symbol_table;
                                let result = AnalyzeAstResult {
                                    evaluations: res.iter().flat_map(
                                        |s| s.symbols.iter().map(|&symbol| Evaluation::eval_from_symbol(st, symbol, None))
                                    ).collect(),
                                    diagnostics: vec![],
                                };
                                return Some((result, Some(range)));
                            }
                        }
                        return None;
                    }
                }
            },
            Stmt::ImportFrom(stmt) => {
                //only check module as names are already supported by default ast walking and name resolution
                if let Some(module) = stmt.module.as_ref() && module.range().contains(TextSize::new(offset)) {
                    let (to_analyze, range) = if module.range().contains(TextSize::new(offset)) {
                        Self::dotted_prefix_at(module, offset).unwrap_or((module.id.as_str(), module.range()))
                    } else {
                        return None;
                    };
                    let to_analyze = Identifier { id: Name::new(to_analyze), range: TextRange::default(), node_index: AtomicNodeIndex::default() };
                    let (from_symbol, _fallback_sym, _file_tree) = resolve_from_stmt(session, file_symbol.into(), Some(&to_analyze), 0);
                    if let Some(symbols) = from_symbol {
                        let st = &session.sync_odoo.symbol_table;
                        let result = AnalyzeAstResult {
                            evaluations: symbols.iter().map(|&symbol| Evaluation::eval_from_symbol(st, symbol, None)).collect(),
                            diagnostics: vec![],
                        };
                        return Some((result, Some(range)));
                    }
                }
            },
            _ => {
                return None;
            }
        }
        None
    }
}

/// Narrowest context around a string expression in the AST
/// Used for features that need to know info about the encapsuling expression or statement
pub enum StringContext {
    /// The innermost call whose arguments contain the string
    CallArgument(ExprCall),
    /// Is the current expression, a string expression under an assign statement i.e. `_order = "..."`
    ModelOrder,
}

impl StringContext {
    /// Context `stmt` gives to its direct string value, if any, along with that string
    pub fn from_stmt(stmt: &Stmt) -> Option<(Self, &ExprStringLiteral)> {
        if let Stmt::Assign(assign) = stmt
            && let [target] = assign.targets.as_slice()
            && let Expr::StringLiteral(expr) = assign.value.as_ref()
            && let Expr::Name(name) = target
            && name.id == "_order"
        {
            return Some((StringContext::ModelOrder, expr));
        }
        None
    }
}

pub struct ExprFinderVisitor<'a> {
    offset: TextSize,
    expr: Option<ExprOrIdent<'a>>,
    string_ctx: Option<StringContext>,
}

impl<'a> ExprFinderVisitor<'a> {
    /*
    Find expr from `stmt` at the given `offset`
    Returns: (expr, string_ctx)
        expr: the expr being searched for
        string_ctx: the narrowest context around the string at `offset`, see `StringContext`
     */
    pub fn find_expr_at(stmt: &'a Stmt, offset: u32) -> (Option<ExprOrIdent<'a>>, Option<StringContext>) {
        let mut visitor = Self {
            offset: TextSize::new(offset),
            expr: None,
            string_ctx: None,
        };
        visitor.visit_stmt(stmt);
        (visitor.expr, visitor.string_ctx)
    }

}

impl<'a> Visitor<'a> for ExprFinderVisitor<'a> {

    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.range().contains(self.offset) {
            if let Expr::Call(expr_call) = expr
                && expr_call.arguments.range().contains(self.offset){
                    self.string_ctx = Some(StringContext::CallArgument(expr_call.clone()));
                }
            walk_expr(self, expr);
            if self.expr.is_none() {
                self.expr = Some(ExprOrIdent::Expr(expr));
            }
        } else {
            walk_expr(self, expr);
        }
    }

    fn visit_alias(&mut self, alias: &'a Alias) {
        walk_alias(self, alias);
        if self.expr.is_none() {
            if alias.name.range().contains(self.offset) {
                self.expr = Some(ExprOrIdent::Ident(&alias.name));
            } else if let Some(ref asname) = alias.asname
                && asname.range().contains(self.offset) {
                    self.expr = Some(ExprOrIdent::Ident(asname))
                }
        }
    }

    fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler) {
        walk_except_handler(self, except_handler);
        if self.expr.is_none() {
            let ExceptHandler::ExceptHandler(ref handler) = *except_handler;
            if let Some(ref ident) = handler.name
                && ident.clone().range().contains(self.offset) {
                    self.expr = Some(ExprOrIdent::Ident(ident));
                }
        } else {
            walk_except_handler(self, except_handler);
        }
    }

    fn visit_parameter(&mut self, parameter: &'a Parameter) {
        walk_parameter(self, parameter);
        if self.expr.is_none() && parameter.name.range().contains(self.offset) {
            self.expr = Some(ExprOrIdent::Parameter(parameter));
        }
    }

    fn visit_keyword(&mut self, keyword: &'a Keyword) {
        walk_keyword(self, keyword);

        if self.expr.is_none() {
            if let Some(ref ident) = keyword.arg
                && ident.range().contains(self.offset) {
                    self.expr = Some(ExprOrIdent::Ident(ident));
                }
        } else {
            walk_keyword(self, keyword)
        }
    }

    fn visit_pattern_keyword(&mut self, pattern_keyword: &'a PatternKeyword) {
        walk_pattern_keyword(self, pattern_keyword);

        if self.expr.is_none() && pattern_keyword.clone().attr.range().contains(self.offset) {
            self.expr = Some(ExprOrIdent::Ident(&pattern_keyword.attr));
        } else {
            walk_pattern_keyword(self, pattern_keyword);
        }
    }

    fn visit_type_param(&mut self, type_param: &'a TypeParam) {
        if type_param.range().contains(self.offset) {
            if self.expr.is_none() {
                walk_type_param(self, type_param);
                let ident = match type_param {
                    TypeParam::TypeVar(t) => Some(&t.name),
                    TypeParam::ParamSpec(t) => Some(&t.name),
                    TypeParam::TypeVarTuple(t) => Some(&t.name),
                };

                if let Some(ident) = ident
                    && ident.range().contains(self.offset) {
                        self.expr = Some(ExprOrIdent::Ident(ident));
                    }

            }
        } else {
            walk_type_param(self, type_param);
        }
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        if pattern.range().contains(self.offset)
            && self.expr.is_none() {
                walk_pattern(self, pattern);
                let ident  = match pattern {
                    Pattern::MatchMapping(mapping) => &mapping.rest,
                    Pattern::MatchStar(mapping) => &mapping.name,
                    Pattern::MatchAs(mapping) => &mapping.name,
                    _ => &None
                };

                if let Some(ident) = ident
                    && ident.range().contains(self.offset) {
                        self.expr = Some(ExprOrIdent::Ident(ident));
                    }
            }
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Some((ctx, expr)) = StringContext::from_stmt(stmt)
            && expr.range().contains(self.offset)
        {
            self.string_ctx = Some(ctx);
        }
        walk_stmt(self, stmt);
        if self.expr.is_none() {
            let idents = match stmt {
                Stmt::FunctionDef(stmt) => vec![&stmt.name],
                Stmt::ClassDef(stmt) => vec![&stmt.name],
                Stmt::Global(stmt) => stmt.names.iter().collect(),
                Stmt::Nonlocal(stmt) => stmt.names.iter().collect(),
                _ => vec![],
            };

            for ident in idents {
                if ident.range().contains(self.offset) {
                    self.expr = Some(ExprOrIdent::Ident(ident));
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AstUtils;
    use ruff_python_ast::{Expr, ExprStringLiteral, Identifier};
    use ruff_text_size::{TextRange, TextSize};

    fn parse_string(code: &str) -> ExprStringLiteral {
        match ruff_python_parser::parse_expression(code).unwrap().into_expr() {
            Expr::StringLiteral(string) => string,
            other => panic!("not a string: {other:?}"),
        }
    }

    /// Text found in the file for `needle`, a piece of the string written in `code`
    fn text_in_file<'a>(code: &'a str, needle: &str) -> Option<&'a str> {
        let string = parse_string(code);
        let start = string.value.to_str().find(needle).unwrap();
        AstUtils::range_in_file(&string, start, start + needle.len()).map(|range| &code[range])
    }

    #[test]
    fn test_range_in_file() {
        assert_eq!(text_in_file("'other_id.other_name'", "other_name"), Some("other_name"));
        assert_eq!(text_in_file("r'other_id.other_name'", "other_name"), Some("other_name"));
        assert_eq!(text_in_file("'''other_id.other_name'''", "other_name"), Some("other_name"));
        assert_eq!(text_in_file("('other_id' # é\n '.other_name')", "other_name"), Some("other_name"));
        assert_eq!(text_in_file("'other_id' '.other_name'", "id.other"), None, "split between parts");
        assert_eq!(text_in_file(r"'other_id\x2eother_name'", "other_name"), None, "escape");
        assert_eq!(text_in_file(r"'other\qid'", "id"), Some("id"), "unknown escapes stay as written");
    }

    #[test]
    fn test_index_in_string() {
        let code = "('other_id' # é\n '.other_name')";
        let string = parse_string(code);
        let index_at = |needle: &str| AstUtils::index_in_string(&string, TextSize::new(code.find(needle).unwrap() as u32));
        assert_eq!(index_at("other_name"), Some(9));
        assert_eq!(index_at("'other_id'"), None, "on a quote");
        assert_eq!(index_at("#"), None, "between the parts");

        let code = r"'name,\tid'";
        let string = parse_string(code);
        assert_eq!(AstUtils::index_in_string(&string, TextSize::new(code.find("id").unwrap() as u32)), None, "escape");
    }

    #[test]
    fn test_index_in_name() {
        let name = |id: &str, start: u32, end: u32| Identifier::new(id, TextRange::new(TextSize::new(start), TextSize::new(end)));
        // `import os`
        assert_eq!(AstUtils::index_in_name(&name("os", 7, 9), TextSize::new(8)), Some(1));
        // `import ｏｓ`: written with 6 bytes in the file, for the 2 bytes of the name `os`
        assert_eq!(AstUtils::index_in_name(&name("os", 7, 13), TextSize::new(10)), None);
    }
}
