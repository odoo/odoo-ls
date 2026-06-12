use std::{cell::RefCell, rc::Rc};

use lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensResult};
use ruff_python_ast::{
    Stmt,
    token::TokenKind,
};
use ruff_source_file::{LineIndex, PositionEncoding};
use ruff_text_size::{Ranged, TextSize};

use crate::core::file_mgr::FileInfo;

pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,    // 0
    SemanticTokenType::STRING,     // 1
    SemanticTokenType::NUMBER,     // 2
    SemanticTokenType::COMMENT,    // 3
    SemanticTokenType::CLASS,      // 4
    SemanticTokenType::FUNCTION,   // 5
    SemanticTokenType::VARIABLE,   // 6
    SemanticTokenType::PARAMETER,  // 7
    SemanticTokenType::PROPERTY,   // 8 - XML attributes
    SemanticTokenType::NAMESPACE,  // 9
    SemanticTokenType::DECORATOR,  // 10
    SemanticTokenType::TYPE,       // 11 - XML tag names
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION, // bit 0
    SemanticTokenModifier::DEFINITION,  // bit 1
    SemanticTokenModifier::READONLY,    // bit 2
    SemanticTokenModifier::STATIC,      // bit 3
    SemanticTokenModifier::DEPRECATED,  // bit 4
    SemanticTokenModifier::ASYNC,       // bit 5
];

const TT_KEYWORD: u32 = 0;
const TT_STRING: u32 = 1;
const TT_NUMBER: u32 = 2;
const TT_COMMENT: u32 = 3;
const TT_CLASS: u32 = 4;
const TT_FUNCTION: u32 = 5;
#[allow(dead_code)]
const TT_VARIABLE: u32 = 6;
const TT_PARAMETER: u32 = 7;
const TT_PROPERTY: u32 = 8;
#[allow(dead_code)]
const TT_NAMESPACE: u32 = 9;
const TT_DECORATOR: u32 = 10;
const TT_TYPE: u32 = 11;

const TM_DECLARATION: u32 = 1 << 0;
const TM_DEFINITION: u32 = 1 << 1;
#[allow(dead_code)]
const TM_READONLY: u32 = 1 << 2;
#[allow(dead_code)]
const TM_STATIC: u32 = 1 << 3;
#[allow(dead_code)]
const TM_DEPRECATED: u32 = 1 << 4;
const TM_ASYNC: u32 = 1 << 5;

#[derive(Debug)]
struct RawToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
    modifiers: u32,
}

pub struct SemanticTokensFeature;

impl SemanticTokensFeature {
    pub fn get_semantic_tokens(
        encoding: PositionEncoding,
        file_info: &Rc<RefCell<FileInfo>>,
    ) -> Option<SemanticTokensResult> {
        let uri = file_info.borrow().uri.clone();
        let raw_tokens = if uri.ends_with(".py") || uri.ends_with(".pyi") {
            Self::collect_python_tokens(file_info, encoding)
        } else if uri.ends_with(".xml") {
            Self::collect_xml_tokens(file_info, encoding)
        } else {
            return None;
        };
        Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: Self::encode_tokens(raw_tokens),
        }))
    }

    pub fn get_js_semantic_tokens(content: &str, encoding: PositionEncoding) -> SemanticTokensResult {
        let raw_tokens = Self::collect_js_tokens(content, encoding);
        SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: Self::encode_tokens(raw_tokens),
        })
    }

    fn encode_tokens(mut raw: Vec<RawToken>) -> Vec<SemanticToken> {
        raw.sort_unstable_by(|a, b| a.line.cmp(&b.line).then(a.start.cmp(&b.start)));
        let mut result = Vec::with_capacity(raw.len());
        let mut prev_line = 0u32;
        let mut prev_start = 0u32;
        for token in raw {
            if token.length == 0 {
                continue;
            }
            let delta_line = token.line - prev_line;
            let delta_start = if delta_line == 0 { token.start - prev_start } else { token.start };
            result.push(SemanticToken {
                delta_line,
                delta_start,
                length: token.length,
                token_type: token.token_type,
                token_modifiers_bitset: token.modifiers,
            });
            prev_line = token.line;
            prev_start = token.start;
        }
        result
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn str_encoding_len(s: &str, encoding: PositionEncoding) -> u32 {
        match encoding {
            PositionEncoding::Utf8 => s.len() as u32,
            PositionEncoding::Utf16 => s.encode_utf16().count() as u32,
            PositionEncoding::Utf32 => s.chars().count() as u32,
        }
    }

    /// Emit one or more RawTokens for a byte range in `content`.
    /// Multi-line ranges are split into per-line tokens.
    fn emit_from_content(
        content: &str,
        byte_start: usize,
        byte_end: usize,
        token_type: u32,
        modifiers: u32,
        encoding: PositionEncoding,
        tokens: &mut Vec<RawToken>,
    ) {
        if byte_start >= byte_end || byte_end > content.len() {
            return;
        }
        // Compute (line, char) for byte_start by scanning backwards to line boundary
        let prefix = &content[..byte_start];
        let (start_line, line_byte_start) = {
            let mut line = 0u32;
            let mut line_start = 0usize;
            for (i, b) in prefix.bytes().enumerate() {
                if b == b'\n' {
                    line += 1;
                    line_start = i + 1;
                }
            }
            (line, line_start)
        };
        let start_char = Self::str_encoding_len(&content[line_byte_start..byte_start], encoding);

        let text = &content[byte_start..byte_end];
        let mut current_line = start_line;
        let mut current_start = start_char;
        let mut chunk_start = 0usize;

        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                let chunk_end = if i > 0 && text.as_bytes()[i - 1] == b'\r' { i - 1 } else { i };
                let chunk = &text[chunk_start..chunk_end];
                let length = Self::str_encoding_len(chunk, encoding);
                if length > 0 {
                    tokens.push(RawToken { line: current_line, start: current_start, length, token_type, modifiers });
                }
                current_line += 1;
                current_start = 0;
                chunk_start = i + 1;
            }
        }
        // Last (or only) chunk
        let chunk = text[chunk_start..].trim_end_matches('\r');
        let length = Self::str_encoding_len(chunk, encoding);
        if length > 0 {
            tokens.push(RawToken { line: current_line, start: current_start, length, token_type, modifiers });
        }
    }

    /// Emit token using FileInfo for byte-offset → position conversion.
    fn emit_from_file_info(
        file_info: &Rc<RefCell<FileInfo>>,
        byte_start: usize,
        byte_end: usize,
        token_type: u32,
        modifiers: u32,
        encoding: PositionEncoding,
        tokens: &mut Vec<RawToken>,
    ) {
        let start_pos = file_info.borrow().offset_to_position(byte_start as u32, encoding);
        let end_pos = file_info.borrow().offset_to_position(byte_end as u32, encoding);
        if start_pos.line == end_pos.line {
            let length = end_pos.character.saturating_sub(start_pos.character);
            if length > 0 {
                tokens.push(RawToken { line: start_pos.line, start: start_pos.character, length, token_type, modifiers });
            }
        } else {
            // Multi-line: get raw content and split
            let content = {
                let fi = file_info.borrow();
                let fia = fi.file_info_ast.borrow();
                fia.text_document.as_ref().map(|td| td.contents().to_string()).unwrap_or_default()
            };
            Self::emit_from_content(&content, byte_start, byte_end, token_type, modifiers, encoding, tokens);
        }
    }

    // -----------------------------------------------------------------------
    // Python
    // -----------------------------------------------------------------------

    fn collect_python_tokens(file_info: &Rc<RefCell<FileInfo>>, encoding: PositionEncoding) -> Vec<RawToken> {
        let mut tokens = Vec::new();
        let fi = file_info.borrow();
        let fia = fi.file_info_ast.borrow();
        let Some(indexed_module) = &fia.indexed_module else { return tokens; };
        let Some(text_doc) = &fia.text_document else { return tokens; };
        let content = text_doc.contents().to_string();

        // --- Token stream: keywords, literals, comments ---
        let parsed = &indexed_module.parsed;
        for token in parsed.tokens().iter() {
            let range = token.range();
            let byte_start = range.start().to_usize();
            let byte_end = range.end().to_usize();
            match token.kind() {
                // Keywords
                TokenKind::False
                | TokenKind::None
                | TokenKind::True
                | TokenKind::And
                | TokenKind::As
                | TokenKind::Assert
                | TokenKind::Async
                | TokenKind::Await
                | TokenKind::Break
                | TokenKind::Class
                | TokenKind::Continue
                | TokenKind::Def
                | TokenKind::Del
                | TokenKind::Elif
                | TokenKind::Else
                | TokenKind::Except
                | TokenKind::Finally
                | TokenKind::For
                | TokenKind::From
                | TokenKind::Global
                | TokenKind::If
                | TokenKind::Import
                | TokenKind::In
                | TokenKind::Is
                | TokenKind::Lambda
                | TokenKind::Nonlocal
                | TokenKind::Not
                | TokenKind::Or
                | TokenKind::Pass
                | TokenKind::Raise
                | TokenKind::Return
                | TokenKind::Try
                | TokenKind::While
                | TokenKind::With
                | TokenKind::Yield => {
                    Self::emit_from_content(&content, byte_start, byte_end, TT_KEYWORD, 0, encoding, &mut tokens);
                }
                // Soft keywords (match, case, type) - only emit if they actually appear as keywords
                TokenKind::Match | TokenKind::Case | TokenKind::Type => {
                    Self::emit_from_content(&content, byte_start, byte_end, TT_KEYWORD, 0, encoding, &mut tokens);
                }
                // Number literals
                TokenKind::Int | TokenKind::Float | TokenKind::Complex => {
                    Self::emit_from_content(&content, byte_start, byte_end, TT_NUMBER, 0, encoding, &mut tokens);
                }
                // Comments
                TokenKind::Comment => {
                    Self::emit_from_content(&content, byte_start, byte_end, TT_COMMENT, 0, encoding, &mut tokens);
                }
                // Regular strings (not f-strings)
                TokenKind::String => {
                    Self::emit_from_content(&content, byte_start, byte_end, TT_STRING, 0, encoding, &mut tokens);
                }
                // F-string components
                TokenKind::FStringStart | TokenKind::FStringMiddle | TokenKind::FStringEnd => {
                    Self::emit_from_content(&content, byte_start, byte_end, TT_STRING, 0, encoding, &mut tokens);
                }
                _ => {}
            }
        }

        // --- AST walk: class names, function names, parameters, decorators ---
        let stmts = &parsed.syntax().body;
        Self::collect_python_ast_tokens(stmts, file_info, encoding, &mut tokens);

        tokens
    }

    fn collect_python_ast_tokens(
        stmts: &[Stmt],
        file_info: &Rc<RefCell<FileInfo>>,
        encoding: PositionEncoding,
        tokens: &mut Vec<RawToken>,
    ) {
        for stmt in stmts {
            match stmt {
                Stmt::FunctionDef(func) => {
                    let modifiers = TM_DECLARATION | TM_DEFINITION | if func.is_async { TM_ASYNC } else { 0 };
                    Self::emit_from_file_info(
                        file_info,
                        func.name.range().start().to_usize(),
                        func.name.range().end().to_usize(),
                        TT_FUNCTION,
                        modifiers,
                        encoding,
                        tokens,
                    );
                    // Parameters
                    for param in func.parameters.args.iter()
                        .chain(func.parameters.posonlyargs.iter())
                        .chain(func.parameters.kwonlyargs.iter())
                        .map(|p| &p.parameter)
                    {
                        Self::emit_from_file_info(
                            file_info,
                            param.name.range().start().to_usize(),
                            param.name.range().end().to_usize(),
                            TT_PARAMETER,
                            TM_DECLARATION,
                            encoding,
                            tokens,
                        );
                    }
                    if let Some(vararg) = &func.parameters.vararg {
                        Self::emit_from_file_info(
                            file_info,
                            vararg.name.range().start().to_usize(),
                            vararg.name.range().end().to_usize(),
                            TT_PARAMETER,
                            TM_DECLARATION,
                            encoding,
                            tokens,
                        );
                    }
                    if let Some(kwarg) = &func.parameters.kwarg {
                        Self::emit_from_file_info(
                            file_info,
                            kwarg.name.range().start().to_usize(),
                            kwarg.name.range().end().to_usize(),
                            TT_PARAMETER,
                            TM_DECLARATION,
                            encoding,
                            tokens,
                        );
                    }
                    // Decorators
                    for dec in &func.decorator_list {
                        Self::emit_from_file_info(
                            file_info,
                            dec.expression.range().start().to_usize(),
                            dec.expression.range().end().to_usize(),
                            TT_DECORATOR,
                            0,
                            encoding,
                            tokens,
                        );
                    }
                    Self::collect_python_ast_tokens(&func.body, file_info, encoding, tokens);
                }
                Stmt::ClassDef(class) => {
                    Self::emit_from_file_info(
                        file_info,
                        class.name.range().start().to_usize(),
                        class.name.range().end().to_usize(),
                        TT_CLASS,
                        TM_DECLARATION | TM_DEFINITION,
                        encoding,
                        tokens,
                    );
                    for dec in &class.decorator_list {
                        Self::emit_from_file_info(
                            file_info,
                            dec.expression.range().start().to_usize(),
                            dec.expression.range().end().to_usize(),
                            TT_DECORATOR,
                            0,
                            encoding,
                            tokens,
                        );
                    }
                    Self::collect_python_ast_tokens(&class.body, file_info, encoding, tokens);
                }
                Stmt::For(s) => {
                    Self::collect_python_ast_tokens(&s.body, file_info, encoding, tokens);
                    Self::collect_python_ast_tokens(&s.orelse, file_info, encoding, tokens);
                }
                Stmt::While(s) => {
                    Self::collect_python_ast_tokens(&s.body, file_info, encoding, tokens);
                    Self::collect_python_ast_tokens(&s.orelse, file_info, encoding, tokens);
                }
                Stmt::If(s) => {
                    Self::collect_python_ast_tokens(&s.body, file_info, encoding, tokens);
                    for clause in &s.elif_else_clauses {
                        Self::collect_python_ast_tokens(&clause.body, file_info, encoding, tokens);
                    }
                }
                Stmt::With(s) => {
                    Self::collect_python_ast_tokens(&s.body, file_info, encoding, tokens);
                }
                Stmt::Match(s) => {
                    for case in &s.cases {
                        Self::collect_python_ast_tokens(&case.body, file_info, encoding, tokens);
                    }
                }
                Stmt::Try(s) => {
                    Self::collect_python_ast_tokens(&s.body, file_info, encoding, tokens);
                    for handler in &s.handlers {
                        if let Some(h) = handler.as_except_handler() {
                            Self::collect_python_ast_tokens(&h.body, file_info, encoding, tokens);
                        }
                    }
                    Self::collect_python_ast_tokens(&s.orelse, file_info, encoding, tokens);
                    Self::collect_python_ast_tokens(&s.finalbody, file_info, encoding, tokens);
                }
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // XML
    // -----------------------------------------------------------------------

    fn collect_xml_tokens(file_info: &Rc<RefCell<FileInfo>>, encoding: PositionEncoding) -> Vec<RawToken> {
        let mut tokens = Vec::new();
        let content = {
            let fi = file_info.borrow();
            let fia = fi.file_info_ast.borrow();
            fia.text_document.as_ref().map(|td| td.contents().to_string()).unwrap_or_default()
        };
        if content.is_empty() {
            return tokens;
        }
        Self::tokenize_xml(&content, encoding, &mut tokens);
        tokens
    }

    /// Scan XML source and emit semantic tokens.
    fn tokenize_xml(content: &str, encoding: PositionEncoding, tokens: &mut Vec<RawToken>) {
        let bytes = content.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            // Comment: <!-- ... -->
            if i + 3 < len && bytes[i] == b'<' && bytes[i + 1] == b'!' && bytes[i + 2] == b'-' && bytes[i + 3] == b'-' {
                let start = i;
                i += 4;
                while i + 2 < len && !(bytes[i] == b'-' && bytes[i + 1] == b'-' && bytes[i + 2] == b'>') {
                    i += 1;
                }
                i += 3; // consume -->
                Self::emit_from_content(content, start, i, TT_COMMENT, 0, encoding, tokens);
                continue;
            }
            // CDATA: <![CDATA[ ... ]]>
            if i + 8 < len && &bytes[i..i + 9] == b"<![CDATA[" {
                i += 9;
                while i + 2 < len && !(bytes[i] == b']' && bytes[i + 1] == b']' && bytes[i + 2] == b'>') {
                    i += 1;
                }
                i += 3;
                continue;
            }
            // Processing instruction: <?...?>
            if i + 1 < len && bytes[i] == b'<' && bytes[i + 1] == b'?' {
                i += 2;
                while i + 1 < len && !(bytes[i] == b'?' && bytes[i + 1] == b'>') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            // Opening or closing tag: < or </
            if bytes[i] == b'<' {
                i += 1;
                // Skip closing slash
                let is_closing = i < len && bytes[i] == b'/';
                if is_closing { i += 1; }

                // Tag name
                let name_start = i;
                while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/' {
                    i += 1;
                }
                if i > name_start {
                    Self::emit_from_content(content, name_start, i, TT_TYPE, 0, encoding, tokens);
                }

                if is_closing {
                    // Skip to >
                    while i < len && bytes[i] != b'>' { i += 1; }
                    if i < len { i += 1; }
                    continue;
                }

                // Attributes until > or />
                loop {
                    // Skip whitespace
                    while i < len && bytes[i].is_ascii_whitespace() { i += 1; }
                    if i >= len || bytes[i] == b'>' || (bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'>') {
                        break;
                    }
                    // Attribute name
                    let attr_name_start = i;
                    while i < len && bytes[i] != b'=' && bytes[i] != b'>' && bytes[i] != b'/' && !bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if i > attr_name_start {
                        Self::emit_from_content(content, attr_name_start, i, TT_PROPERTY, 0, encoding, tokens);
                    }
                    // Skip whitespace
                    while i < len && bytes[i].is_ascii_whitespace() { i += 1; }
                    // '='
                    if i < len && bytes[i] == b'=' {
                        i += 1;
                        // Skip whitespace
                        while i < len && bytes[i].is_ascii_whitespace() { i += 1; }
                        // Attribute value
                        if i < len && (bytes[i] == b'"' || bytes[i] == b'\'') {
                            let quote = bytes[i];
                            let val_start = i;
                            i += 1;
                            while i < len && bytes[i] != quote { i += 1; }
                            if i < len { i += 1; } // closing quote
                            Self::emit_from_content(content, val_start, i, TT_STRING, 0, encoding, tokens);
                        }
                    }
                }
                // Consume > or />
                if i < len && bytes[i] == b'/' { i += 1; }
                if i < len && bytes[i] == b'>' { i += 1; }
                continue;
            }
            i += 1;
        }
    }

    // -----------------------------------------------------------------------
    // JavaScript
    // -----------------------------------------------------------------------

    pub fn collect_js_tokens(content: &str, encoding: PositionEncoding) -> Vec<RawToken> {
        let mut tokens = Vec::new();
        let index = LineIndex::from_source_text(content);
        let bytes = content.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        // Track whether we're after an operator/keyword (for regex disambiguation)
        let mut after_operator = true;

        while i < len {
            let c = bytes[i];

            // Whitespace
            if c.is_ascii_whitespace() {
                if c != b'\n' {
                    // keep after_operator state on newlines only
                }
                i += 1;
                continue;
            }

            // Single-line comment //
            if c == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
                let start = i;
                while i < len && bytes[i] != b'\n' { i += 1; }
                Self::emit_js_from_index(content, &index, start, i, TT_COMMENT, 0, encoding, &mut tokens);
                after_operator = true;
                continue;
            }

            // Multi-line comment /* ... */
            if c == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
                let start = i;
                i += 2;
                while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') { i += 1; }
                i += 2;
                Self::emit_js_from_index(content, &index, start, i, TT_COMMENT, 0, encoding, &mut tokens);
                after_operator = true;
                continue;
            }

            // String literals " and '
            if c == b'"' || c == b'\'' {
                let quote = c;
                let start = i;
                i += 1;
                while i < len && bytes[i] != quote {
                    if bytes[i] == b'\\' { i += 1; }
                    if i < len { i += 1; }
                }
                if i < len { i += 1; }
                Self::emit_js_from_index(content, &index, start, i, TT_STRING, 0, encoding, &mut tokens);
                after_operator = false;
                continue;
            }

            // Template literals `...`
            if c == b'`' {
                let start = i;
                i += 1;
                let mut depth = 0i32;
                while i < len {
                    if bytes[i] == b'\\' { i += 2; continue; }
                    if bytes[i] == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
                        depth += 1;
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'{' && depth > 0 { depth += 1; i += 1; continue; }
                    if bytes[i] == b'}' && depth > 0 { depth -= 1; i += 1; continue; }
                    if bytes[i] == b'`' && depth == 0 { i += 1; break; }
                    i += 1;
                }
                Self::emit_js_from_index(content, &index, start, i, TT_STRING, 0, encoding, &mut tokens);
                after_operator = false;
                continue;
            }

            // Regex literals /pattern/flags — only when after operator/keyword
            if c == b'/' && after_operator {
                let start = i;
                i += 1;
                let mut in_class = false;
                while i < len {
                    if bytes[i] == b'\\' { i += 2; continue; }
                    if bytes[i] == b'[' { in_class = true; i += 1; continue; }
                    if bytes[i] == b']' { in_class = false; i += 1; continue; }
                    if bytes[i] == b'/' && !in_class { i += 1; break; }
                    if bytes[i] == b'\n' { break; }
                    i += 1;
                }
                // Consume flags (letters after closing /)
                while i < len && bytes[i].is_ascii_alphabetic() { i += 1; }
                Self::emit_js_from_index(content, &index, start, i, TT_STRING, 0, encoding, &mut tokens);
                after_operator = false;
                continue;
            }

            // Number literals
            if c.is_ascii_digit() || (c == b'.' && i + 1 < len && bytes[i + 1].is_ascii_digit()) {
                let start = i;
                // Hex/octal/binary prefix
                if c == b'0' && i + 1 < len && matches!(bytes[i + 1], b'x' | b'X' | b'o' | b'O' | b'b' | b'B') {
                    i += 2;
                    while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') { i += 1; }
                } else {
                    while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.' || bytes[i] == b'_') { i += 1; }
                    // Exponent
                    if i < len && (bytes[i] == b'e' || bytes[i] == b'E') {
                        i += 1;
                        if i < len && (bytes[i] == b'+' || bytes[i] == b'-') { i += 1; }
                        while i < len && bytes[i].is_ascii_digit() { i += 1; }
                    }
                    // BigInt suffix
                    if i < len && bytes[i] == b'n' { i += 1; }
                }
                Self::emit_js_from_index(content, &index, start, i, TT_NUMBER, 0, encoding, &mut tokens);
                after_operator = false;
                continue;
            }

            // Identifiers and keywords
            if c.is_ascii_alphabetic() || c == b'_' || c == b'$' {
                let start = i;
                while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$') {
                    i += 1;
                }
                let word = &content[start..i];
                if let Some((tt, mods)) = Self::js_keyword_type(word) {
                    Self::emit_js_from_index(content, &index, start, i, tt, mods, encoding, &mut tokens);
                    after_operator = true;
                } else {
                    after_operator = false;
                }
                continue;
            }

            // Operators that allow regex after them
            if matches!(c, b'=' | b'!' | b'<' | b'>' | b'&' | b'|' | b'^' | b'~' | b'+' | b'-' | b'*' | b'%'
                | b'(' | b'[' | b'{' | b',' | b';' | b':' | b'?' | b'@') {
                after_operator = true;
            } else {
                after_operator = false;
            }
            i += 1;
        }
        tokens
    }

    fn js_keyword_type(word: &str) -> Option<(u32, u32)> {
        match word {
            "break" | "case" | "catch" | "continue" | "debugger" | "default" | "delete"
            | "do" | "else" | "export" | "extends" | "finally" | "for" | "from" | "function"
            | "if" | "import" | "in" | "instanceof" | "let" | "new" | "of" | "return"
            | "static" | "super" | "switch" | "this" | "throw" | "try" | "typeof" | "var"
            | "void" | "while" | "with" | "yield" | "null" | "true" | "false"
            | "const" | "enum" | "implements" | "interface" | "package" | "private"
            | "protected" | "public" | "async" | "await" | "get" | "set" => {
                Some((TT_KEYWORD, 0))
            }
            "class" => Some((TT_KEYWORD, 0)),
            _ => None,
        }
    }

    /// Emit JS tokens using LineIndex for accurate line/column.
    fn emit_js_from_index(
        content: &str,
        index: &LineIndex,
        byte_start: usize,
        byte_end: usize,
        token_type: u32,
        modifiers: u32,
        encoding: PositionEncoding,
        tokens: &mut Vec<RawToken>,
    ) {
        if byte_start >= byte_end {
            return;
        }
        let start_loc = index.source_location(TextSize::from(byte_start as u32), content, encoding);
        let end_loc = index.source_location(TextSize::from(byte_end as u32), content, encoding);
        let start_line = start_loc.line.to_zero_indexed() as u32;
        let end_line = end_loc.line.to_zero_indexed() as u32;

        if start_line == end_line {
            let start_char = start_loc.character_offset.to_zero_indexed() as u32;
            let end_char = end_loc.character_offset.to_zero_indexed() as u32;
            let length = end_char.saturating_sub(start_char);
            if length > 0 {
                tokens.push(RawToken { line: start_line, start: start_char, length, token_type, modifiers });
            }
        } else {
            // Multi-line: split by newlines
            Self::emit_from_content(content, byte_start, byte_end, token_type, modifiers, encoding, tokens);
        }
    }
}
