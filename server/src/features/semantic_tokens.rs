//! Python LSP semantic tokens — v1 (augment-only, usage sites only).
//!
//! This feature layers *additional* highlighting on top of the client's TextMate
//! grammar; it never tries to be a full tokenizer. We only emit tokens for symbol
//! *usage* sites (`Expr::Name` and the `.attr` of `Expr::Attribute`) and we resolve
//! every site through the existing evaluation engine
//! (`AstUtils::get_symbol_from_expr` → `Evaluation::analyze_ast`). Delegating to the
//! engine is deliberate: for attribute access the engine resolves the *base* via
//! `get_symbol`, which runs the Odoo hooks (e.g. `env['model'].method` resolves
//! `method` to the model's `Function` member transitively). We never walk an
//! attribute base by hand, so that behaviour keeps working for free.
//!
//! Deliberately deferred for a later iteration:
//! - definition-site tokens (`def`/`class` names, parameters at the signature,
//!   decorators highlighted as decorators),
//! - emitting tokens for subscript results or model-name string literals,
//! - dedicated Odoo-field detection (Odoo fields are surfaced as plain `property`),
//! - any scope/result caching.
//!
//! String/number/keyword literals are left entirely to the grammar.

use lsp_types::{Range, SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensLegend};
use ruff_python_ast::visitor::{walk_expr, Visitor};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr, ExprOrIdent};
use crate::core::file_mgr::FileInfo;
use crate::core::symbols::symbol_keys::{SourceFileKey, SymbolKey};
use crate::features::ast_utils::AstUtils;
use crate::threads::SessionInfo;
use std::cell::RefCell;
use std::rc::Rc;

/// Single source of truth for token *type* indices. The order here MUST match the
/// order of `legend().token_types` so the `u32` we send to the client lines up.
#[repr(u32)]
#[derive(Clone, Copy)]
#[allow(dead_code)] // Decorator / Keyword are reserved legend slots, not emitted in v1.
enum TokType {
    Namespace = 0,
    Class = 1,
    Function = 2,
    Method = 3,
    Parameter = 4,
    Variable = 5,
    Property = 6,
    Decorator = 7,
    Keyword = 8,
}

/// Single source of truth for token *modifier* bit positions. The modifier bitset
/// sent to the client is the OR of `(1 << index)` for each active modifier, so this
/// order MUST match `legend().token_modifiers`.
#[repr(u32)]
#[derive(Clone, Copy)]
#[allow(dead_code)] // Declaration / Definition / Readonly are reserved legend slots, not emitted in v1.
enum TokMod {
    Declaration = 0,
    Definition = 1,
    Readonly = 2,
    Static = 3,
    DefaultLibrary = 4,
}

impl TokMod {
    /// Bit mask for this modifier inside the per-token modifier bitset.
    fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// Where a usage site came from, used to disambiguate the few cases that color
/// differently depending on syntactic position (a `Function` used bare is a
/// `function`, but accessed as a member it is a `method`, etc.).
#[derive(Clone, Copy)]
enum TokenOrigin {
    Name,
    Attr,
}

pub struct SemanticTokensFeature {}

impl SemanticTokensFeature {

    /// Single source of truth for the token legend advertised to the client.
    /// The vec order here is mirrored by `TokType` / `TokMod` indices above.
    pub fn legend() -> SemanticTokensLegend {
        SemanticTokensLegend {
            token_types: vec![
                SemanticTokenType::NAMESPACE,   // TokType::Namespace
                SemanticTokenType::CLASS,       // TokType::Class
                SemanticTokenType::FUNCTION,    // TokType::Function
                SemanticTokenType::METHOD,      // TokType::Method
                SemanticTokenType::PARAMETER,   // TokType::Parameter
                SemanticTokenType::VARIABLE,    // TokType::Variable
                SemanticTokenType::PROPERTY,    // TokType::Property
                SemanticTokenType::DECORATOR,   // TokType::Decorator
                SemanticTokenType::KEYWORD,     // TokType::Keyword
            ],
            token_modifiers: vec![
                SemanticTokenModifier::DECLARATION,     // TokMod::Declaration
                SemanticTokenModifier::DEFINITION,      // TokMod::Definition
                SemanticTokenModifier::READONLY,        // TokMod::Readonly
                SemanticTokenModifier::STATIC,          // TokMod::Static
                SemanticTokenModifier::DEFAULT_LIBRARY, // TokMod::DefaultLibrary
            ],
        }
    }

    pub fn tokens_python(session: &mut SessionInfo, file_symbol: SourceFileKey, file_info: &Rc<RefCell<FileInfo>>) -> SemanticTokens {
        // Raw, unsorted tokens: (range, token type index, modifier bitset). We
        // collect first, then sort by (line, char) and delta-encode at the end.
        let mut raw: Vec<(Range, u32, u32)> = vec![];

        // Borrow the AST for the whole walk. `session` is a separate object, and
        // `get_symbol_from_expr` does not need this borrow, so there is no conflict.
        let file_info_ast = file_info.borrow().file_info_ast.clone();
        let file_info_ast_ref = file_info_ast.borrow();
        if let Some(stmts) = file_info_ast_ref.get_stmts() {
            let mut visitor = SemanticTokenVisitor {
                session,
                file_symbol,
                file_info,
                raw: &mut raw,
            };
            for stmt in stmts.iter() {
                visitor.visit_stmt(stmt);
            }
        }
        drop(file_info_ast_ref);

        Self::encode(raw)
    }

    /// Sort raw tokens by (line, char) and delta-encode into the flat LSP form.
    fn encode(mut raw: Vec<(Range, u32, u32)>) -> SemanticTokens {
        raw.sort_by_key(|(range, _, _)| (range.start.line, range.start.character));
        let mut data: Vec<SemanticToken> = Vec::with_capacity(raw.len());
        let mut prev_line: u32 = 0;
        let mut prev_start: u32 = 0;
        for (range, token_type, token_modifiers_bitset) in raw {
            // Identifiers are single-line; defensively skip anything multi-line.
            if range.start.line != range.end.line {
                continue;
            }
            let delta_line = range.start.line - prev_line;
            let delta_start = if delta_line == 0 {
                range.start.character - prev_start
            } else {
                range.start.character
            };
            data.push(SemanticToken {
                delta_line,
                delta_start,
                length: range.end.character - range.start.character,
                token_type,
                token_modifiers_bitset,
            });
            prev_line = range.start.line;
            prev_start = range.start.character;
        }
        SemanticTokens { result_id: None, data }
    }
}

/// Source-order traversal that resolves each usage site through the engine and
/// pushes one raw token per coloured site. Holds `&mut SessionInfo` directly; the
/// AST nodes are borrowed with lifetime `'a` from the file's `file_info_ast`.
struct SemanticTokenVisitor<'a, 'b, 's> {
    session: &'s mut SessionInfo<'b>,
    file_symbol: SourceFileKey,
    file_info: &'a Rc<RefCell<FileInfo>>,
    raw: &'a mut Vec<(Range, u32, u32)>,
}

impl<'a, 'b, 's> SemanticTokenVisitor<'a, 'b, 's> {

    /// Resolve `expr` at `offset` through the engine and, if it resolves to exactly
    /// one live symbol, push a token at `segment_range` classified per `origin`.
    fn resolve_and_push(&mut self, expr: &ExprOrIdent, offset: u32, segment_range: ruff_text_size::TextRange, origin: TokenOrigin) {
        let (analyze_result, _range) = AstUtils::get_symbol_from_expr(self.session, self.file_symbol, expr, offset);
        let Some(key) = Self::unambiguous_key(self.session, &analyze_result.evaluations) else {
            return;
        };
        let Some((token_type, modifiers)) = classify(self.session, key, origin) else {
            return;
        };
        let range = self.file_info.borrow().text_range_to_range(&segment_range, self.session.sync_odoo.encoding);
        self.raw.push((range, token_type, modifiers));
    }

    /// Apply the strict ambiguity rule: drop `UNBOUND` sentinels, then require
    /// exactly one remaining `WEAK`/`SELF` evaluation that upgrades to a live key.
    /// Anything else (zero, multiple, or a real eval alongside `ANY`/`NONE`/...) is
    /// ambiguous and yields `None` (no token).
    fn unambiguous_key(session: &mut SessionInfo, evaluations: &[Evaluation]) -> Option<SymbolKey> {
        let mut found: Option<SymbolKey> = None;
        for eval in evaluations.iter() {
            let ptr = eval.symbol.get_symbol_ptr();
            // Drop UNBOUND sentinels — they do not count toward ambiguity.
            if matches!(ptr, EvaluationSymbolPtr::UNBOUND(_)) {
                continue;
            }
            match ptr.upgrade_weak(session.st()) {
                Some(key) => {
                    if found.is_some() {
                        // More than one real evaluation → ambiguous.
                        return None;
                    }
                    found = Some(key);
                }
                // ARG / DOMAIN / NONE / ANY, or an expired weak → ambiguous/unresolvable.
                None => return None,
            }
        }
        found
    }
}

impl<'a, 'b, 's> Visitor<'a> for SemanticTokenVisitor<'a, 'b, 's> {

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => {
                if matches!(name.id.as_str(), "self" | "cls") {
                    // Leave to the grammar's special-self/cls rule for consistency
                    // with the (currently un-tokenized) signature parameter.
                } else {
                    // Resolve the name through the engine and color it at its own range.
                    let offset = name.range().start().to_u32();
                    self.resolve_and_push(&ExprOrIdent::Expr(expr), offset, name.range(), TokenOrigin::Name);
                }
            }
            Expr::Attribute(attribute) => {
                // CRITICAL: resolve on the WHOLE attribute expr so the engine resolves
                // the base via `get_symbol` (running the Odoo hooks). We color the
                // `.attr` identifier range only; the base (and any nested segments) is
                // resolved independently when we descend below.
                let offset = attribute.range().start().to_u32();
                self.resolve_and_push(&ExprOrIdent::Expr(expr), offset, attribute.attr.range(), TokenOrigin::Attr);
            }
            _ => {}
        }
        // Always descend fully (never stop early) so nested segments are visited.
        walk_expr(self, expr);
    }
}

/// Map a resolved `SymbolKey` to a (token type index, modifier bitset), taking the
/// syntactic `origin` into account. Returns `None` for symbol kinds we do not color
/// (files, roots, XML/CSV, etc.). Odoo fields are surfaced as plain `property`.
fn classify(session: &mut SessionInfo, key: SymbolKey, origin: TokenOrigin) -> Option<(u32, u32)> {
    let external_mod = |is_external: bool| if is_external { TokMod::DefaultLibrary.bit() } else { 0 };

    match key {
        SymbolKey::Class(class_key) => {
            let is_external = session.st()[class_key].is_external;
            Some((TokType::Class as u32, external_mod(is_external)))
        }
        SymbolKey::Function(function_key) => {
            let func = &session.st()[function_key];
            let is_external = func.is_external;
            let is_static = func.is_static;
            let is_property = func.is_property;
            let is_class_method = func.is_class_method;
            match origin {
                TokenOrigin::Name => {
                    // Bare function reference.
                    let mut modifiers = external_mod(is_external);
                    if is_static {
                        modifiers |= TokMod::Static.bit();
                    }
                    Some((TokType::Function as u32, modifiers))
                }
                TokenOrigin::Attr => {
                    if is_property {
                        // Property descriptors read as plain properties.
                        Some((TokType::Property as u32, external_mod(is_external)))
                    } else {
                        let mut modifiers = external_mod(is_external);
                        if is_static || is_class_method {
                            modifiers |= TokMod::Static.bit();
                        }
                        Some((TokType::Method as u32, modifiers))
                    }
                }
            }
        }
        SymbolKey::Variable(variable_key) => {
            let var = &session.st()[variable_key];
            let is_external = var.is_external;
            let is_parameter = var.is_parameter;
            if is_parameter {
                // Parameters are `parameter` regardless of origin.
                Some((TokType::Parameter as u32, external_mod(is_external)))
            } else {
                match origin {
                    // A variable accessed as a member reads as a property.
                    TokenOrigin::Attr => Some((TokType::Property as u32, external_mod(is_external))),
                    TokenOrigin::Name => Some((TokType::Variable as u32, external_mod(is_external))),
                }
            }
        }
        SymbolKey::Module(_) | SymbolKey::PythonPackage(_) | SymbolKey::Namespace(_) => {
            Some((TokType::Namespace as u32, 0))
        }
        // File / Root / DiskDir / Compiled / Xml* / Csv* are not coloured in v1.
        _ => None,
    }
}
