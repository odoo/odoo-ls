//! Python LSP semantic tokens

use lsp_types::{Position, Range, SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensLegend};
use ruff_python_ast::visitor::{walk_expr, walk_parameter, Visitor};
use ruff_python_ast::{Decorator, Expr, ExprCall, Parameter};
use ruff_text_size::{Ranged, TextRange};

use crate::core::evaluation::{Evaluation, EvaluationSymbolPtr, ExprOrIdent};
use crate::core::file_mgr::FileInfo;
use crate::core::symbols::storage::SymbolTable;
use crate::core::symbols::symbol_keys::{SourceFileKey, SymbolKey};
use crate::features::ast_utils::AstUtils;
use crate::features::features_utils::{FeaturesUtils, SegmentPick, StringResolution};
use crate::threads::SessionInfo;
use std::cell::RefCell;
use std::rc::Rc;

/// Single source of truth for token *type* indices. The order here MUST match the
/// order of `legend().token_types` so the `u32` we send to the client lines up.
///
/// Indices 0..=11 are laid out to exactly mirror tsserver's `ClassificationType`
/// numbering (class, enum, interface, namespace, typeParameter, type, parameter,
/// variable, enumMember, property, function, member). This lets the JS path forward
/// tsserver's decoded type index through *verbatim* — no translation table. tsserver's
/// `member` is LSP's `method` (slot 11). Slots 12+ are Python-only extras tsserver
/// never emits.
#[repr(u32)]
#[derive(Clone, Copy)]
#[allow(dead_code)] // Several slots exist only for the JS (tsserver) legend, not emitted by the Python path.
enum TokType {
    Class = 0,
    Enum = 1,
    Interface = 2,
    Namespace = 3,
    TypeParameter = 4,
    Type = 5,
    Parameter = 6,
    Variable = 7,
    EnumMember = 8,
    Property = 9,
    Function = 10,
    Method = 11, // tsserver "member"
    Decorator = 12,
    Keyword = 13,
}

/// Single source of truth for token *modifier* bit positions. The modifier bitset
/// sent to the client is the OR of `(1 << index)` for each active modifier, so this
/// order MUST match `legend().token_modifiers`.
///
/// Bits 0..=5 mirror tsserver's `TokenModifier` numbering (declaration, static, async,
/// readonly, defaultLibrary, local), so the JS path forwards tsserver's modifier bitset
/// through verbatim. `Definition` (bit 6) is Python-only reserved.
#[repr(u32)]
#[derive(Clone, Copy)]
#[allow(dead_code)] // Several bits exist only for the JS (tsserver) legend, not emitted by the Python path.
enum TokMod {
    Declaration = 0,
    Static = 1,
    Async = 2,
    Readonly = 3,
    DefaultLibrary = 4,
    Local = 5,
    Definition = 6,
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
                SemanticTokenType::CLASS,          // TokType::Class (ts 0)
                SemanticTokenType::ENUM,           // TokType::Enum (ts 1)
                SemanticTokenType::INTERFACE,      // TokType::Interface (ts 2)
                SemanticTokenType::NAMESPACE,      // TokType::Namespace (ts 3)
                SemanticTokenType::TYPE_PARAMETER, // TokType::TypeParameter (ts 4)
                SemanticTokenType::TYPE,           // TokType::Type (ts 5)
                SemanticTokenType::PARAMETER,      // TokType::Parameter (ts 6)
                SemanticTokenType::VARIABLE,       // TokType::Variable (ts 7)
                SemanticTokenType::ENUM_MEMBER,    // TokType::EnumMember (ts 8)
                SemanticTokenType::PROPERTY,       // TokType::Property (ts 9)
                SemanticTokenType::FUNCTION,       // TokType::Function (ts 10)
                SemanticTokenType::METHOD,         // TokType::Method (ts 11 "member")
                SemanticTokenType::DECORATOR,      // TokType::Decorator (python-only)
                SemanticTokenType::KEYWORD,        // TokType::Keyword (python-only)
            ],
            token_modifiers: vec![
                SemanticTokenModifier::DECLARATION,     // TokMod::Declaration (ts 0)
                SemanticTokenModifier::STATIC,          // TokMod::Static (ts 1)
                SemanticTokenModifier::ASYNC,           // TokMod::Async (ts 2)
                SemanticTokenModifier::READONLY,        // TokMod::Readonly (ts 3)
                SemanticTokenModifier::DEFAULT_LIBRARY, // TokMod::DefaultLibrary (ts 4)
                SemanticTokenModifier::new("local"),    // TokMod::Local (ts 5)
                SemanticTokenModifier::DEFINITION,      // TokMod::Definition (python-only)
            ],
        }
    }

    pub fn tokens_python(session: &mut SessionInfo, file_symbol: SourceFileKey, file_info: &Rc<RefCell<FileInfo>>) -> SemanticTokens {
        // Raw, unsorted tokens: (range, token type index, modifier bitset). We
        // collect first, then sort by (line, char) and delta-encode at the end.
        let mut raw: Vec<(Range, u32, u32)> = vec![];

        let file_info_ast = file_info.borrow().file_info_ast.clone();
        let file_info_ast_ref = file_info_ast.borrow();
        if let Some(stmts) = file_info_ast_ref.get_stmts() {
            let mut visitor = SemanticTokenVisitor {
                session,
                file_symbol,
                file_info,
                raw: &mut raw,
                file_path: file_info.borrow().uri.clone(),
                enclosing_call: None,
            };
            for stmt in stmts.iter() {
                visitor.visit_stmt(stmt);
            }
        }
        drop(file_info_ast_ref);

        Self::encode(raw)
    }

    /// JavaScript/TypeScript semantic tokens, delegated wholesale to tsserver.
    ///
    /// Unlike the Python path (which resolves every site through our own evaluation
    /// engine), JS semantics live in tsserver, which already ships a native semantic
    /// classifier. Our legend is laid out to mirror tsserver's type/modifier numbering,
    /// so the classifications come back already aligned — the only work here is turning
    /// tsserver's UTF-16 offsets into LSP positions and reusing `encode`.
    pub fn tokens_javascript(session: &mut SessionInfo, file_path: &str, file_info: &Rc<RefCell<FileInfo>>) -> SemanticTokens {
        // Snapshot the content first: we need it to map offsets to positions, and we
        // must not hold the borrow across the `&mut session` bridge call below.
        let content = {
            let fi = file_info.borrow();
            let fia = fi.file_info_ast.borrow();
            match fia.text_document.as_ref() {
                Some(td) => td.contents().to_string(),
                None => return SemanticTokens { result_id: None, data: vec![] },
            }
        };

        // tsserver is the only source of JS semantics. Without it (CLI / most tests)
        // we emit nothing and let the client's grammar handle highlighting.
        let spans = match session.sync_odoo.tsserver_bridge.as_mut() {
            Some(bridge) => bridge.get_semantic_tokens(file_path),
            None => return SemanticTokens { result_id: None, data: vec![] },
        };

        Self::encode(utf16_spans_to_raw_tokens(&content, spans))
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

/// Convert tsserver's UTF-16-offset spans into `Range`-tagged raw tokens ready for
/// `encode`. tsserver reports each token as `(start, length, token_type, modifiers)`
/// where `start`/`length` are UTF-16 code-unit offsets from the file start; LSP wants
/// line/character positions (also UTF-16 by default, which is what the rest of the
/// tsserver bridge assumes). Pure: depends only on `content` and `spans`.
fn utf16_spans_to_raw_tokens(content: &str, spans: Vec<(u32, u32, u32, u32)>) -> Vec<(Range, u32, u32)> {
    // Line-start offsets in UTF-16 code units — tsserver's offset space.
    let mut line_starts: Vec<u32> = vec![0];
    let mut u16_offset: u32 = 0;
    for ch in content.chars() {
        u16_offset += ch.len_utf16() as u32;
        if ch == '\n' {
            line_starts.push(u16_offset);
        }
    }
    let to_pos = |offset: u32| -> Position {
        let line = line_starts.partition_point(|&s| s <= offset).saturating_sub(1);
        Position { line: line as u32, character: offset - line_starts[line] }
    };

    spans
        .into_iter()
        .map(|(start, length, token_type, modifiers)| {
            let range = Range { start: to_pos(start), end: to_pos(start + length) };
            (range, token_type, modifiers)
        })
        .collect()
}

/// `self`/`cls` are left entirely to the client grammar's special-self/cls rule —
/// at both their signature binding and their body uses — so we never emit a token
/// for them.
fn is_grammar_owned_self(name: &str) -> bool {
    matches!(name, "self" | "cls")
}

/// Source-order traversal that resolves each usage site through the engine and
/// pushes one raw token per coloured site. Holds `&mut SessionInfo` directly; the
/// AST nodes are borrowed with lifetime `'a` from the file's `file_info_ast`.
struct SemanticTokenVisitor<'a, 'b, 's> {
    session: &'s mut SessionInfo<'b>,
    file_symbol: SourceFileKey,
    file_info: &'a Rc<RefCell<FileInfo>>,
    raw: &'a mut Vec<(Range, u32, u32)>,
    file_path: String,
    /// Nearest enclosing call, so string args resolve to fields/methods.
    enclosing_call: Option<&'a ExprCall>,
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

    /// Convert `range` to LSP and push one raw token.
    fn push_raw(&mut self, range: TextRange, token_type: u32, modifiers: u32) {
        let lsp_range = self.file_info.borrow().text_range_to_range(&range, self.session.sync_odoo.encoding);
        self.raw.push((lsp_range, token_type, modifiers));
    }

    /// Apply the strict ambiguity rule: drop `UNBOUND` sentinels, follow import
    /// aliases to what they re-export, then require exactly one remaining
    /// `WEAK`/`SELF` pointer that upgrades to a live key. Anything else (zero,
    /// multiple, or a real eval alongside `ANY`/`NONE`/...) is ambiguous and yields
    /// `None` (no token).
    fn unambiguous_key(session: &mut SessionInfo, evaluations: &[Evaluation]) -> Option<SymbolKey> {
        let mut found: Option<SymbolKey> = None;
        for eval in evaluations.iter() {
            let ptr = eval.symbol.get_symbol_ptr();
            // Drop UNBOUND sentinels — they do not count toward ambiguity.
            if matches!(ptr, EvaluationSymbolPtr::UNBOUND(_)) {
                continue;
            }
            for followed in SymbolTable::follow_imported_ref(ptr, session, None) {
                if matches!(followed, EvaluationSymbolPtr::UNBOUND(_)) {
                    continue;
                }
                match followed.upgrade_weak(session.st()) {
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
        }
        found
    }
}

impl<'a, 'b, 's> Visitor<'a> for SemanticTokenVisitor<'a, 'b, 's> {

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => {
                // Skip `self` and `cls`
                if !is_grammar_owned_self(name.id.as_str()) {
                    let offset = name.range().start().to_u32();
                    self.resolve_and_push(&ExprOrIdent::Expr(expr), offset, name.range(), TokenOrigin::Name);
                }
            }
            Expr::Attribute(attribute) => {
                // CRITICAL: resolve on the WHOLE attribute expr so the engine resolves the
                // base via `get_symbol` (running the Odoo hooks). Only the `.attr` range is
                // tokenized; the base is resolved independently when we descend below.
                // TODO: this is ineficient, as this resolves the base for each attribute in a chain.
                let offset = attribute.range().start().to_u32();
                self.resolve_and_push(&ExprOrIdent::Expr(expr), offset, attribute.attr.range(), TokenOrigin::Attr);
            }
            Expr::StringLiteral(string_literal) => {
                // Tokenize a model-meaning string by what it resolves to:
                // - model/module names as class/namespace over the whole string
                // - field/method paths as property/method per dotted segment
                let range = string_literal.range;
                match FeaturesUtils::resolve_string_symbols(self.session, self.file_symbol, &self.file_path, string_literal.value.to_str(), range, self.enclosing_call, SegmentPick::All) {
                    Some(StringResolution::Members(members)) => {
                        for (sym, segment_range) in members {
                            if let Some((token_type, modifiers)) = classify(self.session, sym, TokenOrigin::Attr) {
                                self.push_raw(segment_range, token_type, modifiers);
                            }
                        }
                    }
                    Some(StringResolution::Model(_)) => self.push_raw(range, TokType::Class as u32, 0),
                    Some(StringResolution::Module(_)) => self.push_raw(range, TokType::Namespace as u32, 0),
                    Some(StringResolution::XmlId(records)) => {
                        // Color as class only where definition can also jump (record has a file).
                        if records.iter().any(|&record| self.session.st().get_file(record.into()).is_some()) {
                            self.push_raw(range, TokType::Class as u32, 0);
                        }
                    }
                    None => {}
                }
            }
            Expr::Call(call) => {
                // Expose the enclosing call so string args resolve to fields/methods.
                let prev = self.enclosing_call.replace(call);
                walk_expr(self, expr);
                self.enclosing_call = prev;
                return;
            }
            _ => {}
        }
        // Always descend fully (never stop early) so nested segments are visited.
        walk_expr(self, expr);
    }

    fn visit_parameter(&mut self, parameter: &'a Parameter) {
        let name = &parameter.name;
        // Tokenize parameter (name only), skip if `self` or `cls`.
        if !is_grammar_owned_self(name.as_str()) {
            let range = self.file_info.borrow().text_range_to_range(&name.range(), self.session.sync_odoo.encoding);
            self.raw.push((range, TokType::Parameter as u32, TokMod::Declaration.bit()));
        }
        // Keep descending so the annotation expr still gets its own token.
        walk_parameter(self, parameter);
    }

    fn visit_decorator(&mut self, decorator: &'a Decorator) {
        // Visit only its args, leave the callee to the grammar.
        if let Expr::Call(call) = &decorator.expression {
            let prev = self.enclosing_call.replace(call);
            self.visit_arguments(&call.arguments);
            self.enclosing_call = prev;
        }
    }
}

/// Map a resolved `SymbolKey` to a (token type index, modifier bitset), taking the
/// syntactic `origin` into account. Returns `None` for symbol kinds we do not color
/// (files, roots, XML/CSV, etc.).
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
                    // Leave to the grammar, it has more complete info (e.g. constants, class vars...)
                    TokenOrigin::Name => None,
                }
            }
        }
        SymbolKey::Module(_) | SymbolKey::PythonPackage(_) | SymbolKey::Namespace(_) => {
            Some((TokType::Namespace as u32, 0))
        }
        _ => None,
    }
}
