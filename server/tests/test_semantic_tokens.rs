use std::fs;
use std::path::Path;

use lsp_types::{
    PartialResultParams, SemanticTokenModifier, SemanticTokenType, SemanticTokensParams,
    SemanticTokensResult, TextDocumentIdentifier, WorkDoneProgressParams,
};
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::features::semantic_tokens::SemanticTokensFeature;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

use crate::setup::setup::{create_init_session, setup_server};

mod setup;

const CLASS: SemanticTokenType = SemanticTokenType::CLASS;
const FUNCTION: SemanticTokenType = SemanticTokenType::FUNCTION;
const METHOD: SemanticTokenType = SemanticTokenType::METHOD;
const NAMESPACE: SemanticTokenType = SemanticTokenType::NAMESPACE;
const PARAMETER: SemanticTokenType = SemanticTokenType::PARAMETER;
const PROPERTY: SemanticTokenType = SemanticTokenType::PROPERTY;

const DECLARATION: SemanticTokenModifier = SemanticTokenModifier::DECLARATION;
const STATIC: SemanticTokenModifier = SemanticTokenModifier::STATIC;

/// Semantic tokens of the Python files of `module_semantic_tokens`. Starting a server with Odoo
/// is expensive, so everything shares one setup: the `check_*` functions below are ordinary
/// functions, one per family of tokens, and each file is requested exactly once — like a client.
#[test]
fn test_semantic_tokens_python() {
    let (mut odoo, config) = setup_server(true);
    let mut session = create_init_session(&mut odoo, config);
    let module = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("data").join("addons").join("module_semantic_tokens");

    let main = TokenFile::load(&mut session, &module.join("models").join("sem_tokens_main.py").sanitize());
    let manifest = TokenFile::load(&mut session, &module.join("__manifest__.py").sanitize());

    check_parameter_declarations(&main);
    check_parameter_usages(&main);
    check_class_tokens(&main);
    check_function_and_method_tokens(&main);
    check_property_tokens(&main);
    check_namespace_tokens(&main);
    check_names_left_to_the_grammar(&main);
    check_decorators_left_to_the_grammar(&main);
    check_model_string_tokens(&main);
    check_field_path_string_tokens(&main);
    check_xml_id_string_tokens(&main);
    check_import_shim_tokens(&main);
    check_manifest_module_strings(&manifest);
}

/// Signature bindings are coloured directly, without resolution: one `parameter` token with the
/// `declaration` modifier per binding, whatever its kind. `self`/`cls` are left to the grammar,
/// which has a dedicated rule for them, and so is the function name itself.
fn check_parameter_declarations(main: &TokenFile) {
    check(main, 11, "alpha", Some((PARAMETER, &[DECLARATION])));
    check(main, 11, "beta", Some((PARAMETER, &[DECLARATION])));
    check(main, 11, "star_args", Some((PARAMETER, &[DECLARATION])));
    check(main, 11, "kw_only", Some((PARAMETER, &[DECLARATION])));
    check(main, 11, "star_kwargs", Some((PARAMETER, &[DECLARATION])));
    check(main, 11, "free_function", None);
    check(main, 15, "positional", Some((PARAMETER, &[DECLARATION])));
    check(main, 15, "standard", Some((PARAMETER, &[DECLARATION])));
    check(main, 19, "with_annotation", Some((PARAMETER, &[DECLARATION])));
    check(main, 23, "lambda_param", Some((PARAMETER, &[DECLARATION])));
    check(main, 57, "param", Some((PARAMETER, &[DECLARATION])));
    check(main, 57, "self", None);
    check(main, 54, "cls", None);
}

/// Body references resolve through the engine and keep the `parameter` type, without the
/// `declaration` modifier. An attribute base is resolved too, so `record` is coloured even
/// though nothing is known about `record.name`.
fn check_parameter_usages(main: &TokenFile) {
    check(main, 12, "alpha", Some((PARAMETER, &[])));
    check(main, 16, "standard", Some((PARAMETER, &[])));
    check(main, 20, "with_annotation", Some((PARAMETER, &[])));
    check(main, 24, "lambda_param", Some((PARAMETER, &[])));
    check(main, 60, "param", Some((PARAMETER, &[])));
    check(main, 71, "record", Some((PARAMETER, &[])));
    check(main, 71, "name", None);
    check(main, 69, "self", None);
}

fn check_class_tokens(main: &TokenFile) {
    check(main, 19, "PlainClass", Some((CLASS, &[])));
    check(main, 63, "PlainClass", Some((CLASS, &[])));
    check(main, 28, "Model", Some((CLASS, &[])));
    check(main, 32, "Char", Some((CLASS, &[])));
    check(main, 33, "Many2one", Some((CLASS, &[])));
}

/// A `Function` reached bare is a `function`; reached as a member it is a `method`, and
/// `@staticmethod`/`@classmethod` add the `static` modifier.
fn check_function_and_method_tokens(main: &TokenFile) {
    check(main, 59, "module_level_func", Some((FUNCTION, &[])));
    check(main, 64, "instance_method", Some((METHOD, &[])));
    check(main, 65, "static_helper", Some((METHOD, &[STATIC])));
    check(main, 66, "class_helper", Some((METHOD, &[STATIC])));
}

/// Odoo fields are plain properties, and so are `@property` descriptors and class variables.
/// `other_id` is the guard for the rule that semantic tokens colour the symbol AT the cursor
/// and never follow it to its type: a `Many2one` is a `property`, not the comodel `class`.
fn check_property_tokens(main: &TokenFile) {
    check(main, 39, "total", Some((PROPERTY, &[])));
    check(main, 67, "computed_property", Some((PROPERTY, &[])));
    check(main, 68, "class_attribute", Some((PROPERTY, &[])));
    check(main, 69, "name", Some((PROPERTY, &[])));
    check(main, 70, "other_id", Some((PROPERTY, &[])));
}

fn check_namespace_tokens(main: &TokenFile) {
    check(main, 75, "sem_pkg", Some((NAMESPACE, &[])));
    check(main, 75, "SEM_PKG_CONSTANT", Some((PROPERTY, &[])));
}

/// A `Variable` reached as a name gets NO token: the grammar knows more there (constants, class
/// variables...), so emitting one is worse than staying out of the way. Same for a file symbol
/// and for a string that means nothing to the engine.
fn check_names_left_to_the_grammar(main: &TokenFile) {
    check(main, 32, "name", None);
    check(main, 58, "local_value", None);
    check(main, 58, "SEM_CONSTANT", None);
    check(main, 59, "local_value", None);
    check(main, 63, "instance", None);
    check(main, 67, "instance", None);
    check(main, 76, "sem_tokens_shim", None);
    check(main, 83, "'a value'", None);
    check(main, 84, "'not.a.model'", None);
}

/// Only a decorator's arguments are visited; its callee is grammar-owned, which keeps
/// `@api.depends` uniformly coloured instead of split into two unrelated kinds.
fn check_decorators_left_to_the_grammar(main: &TokenFile) {
    check(main, 37, "api", None);
    check(main, 37, "depends", None);
    check(main, 41, "api", None);
    check(main, 41, "model", None);
    check(main, 45, "property", None);
    check(main, 49, "staticmethod", None);
    check(main, 53, "classmethod", None);
}

/// A string naming a model is a `class` over the whole literal, quotes included.
fn check_model_string_tokens(main: &TokenFile) {
    check(main, 29, "'sem.tokens.usage'", Some((CLASS, &[])));
    check(main, 33, "'sem.tokens.other'", Some((CLASS, &[])));
    check(main, 80, "'sem.tokens.other'", Some((CLASS, &[])));
}

/// Field paths are coloured per dotted segment, past the quote; a method named by `compute=`
/// is coloured over the whole literal, quotes included.
fn check_field_path_string_tokens(main: &TokenFile) {
    check(main, 37, "other_id", Some((PROPERTY, &[])));
    check(main, 37, "other_name", Some((PROPERTY, &[])));
    check(main, 34, "'_compute_total'", Some((METHOD, &[])));
    check(main, 35, "other_id", Some((PROPERTY, &[])));
    check(main, 35, "other_name", Some((PROPERTY, &[])));
    check(main, 83, "name", Some((PROPERTY, &[])));
}

/// An xml id is coloured exactly where Definition would navigate from it — so a reference to a
/// record that does not exist stays grammar-coloured.
fn check_xml_id_string_tokens(main: &TokenFile) {
    check(main, 81, "'module_semantic_tokens.sem_token_record'", Some((CLASS, &[])));
    check(main, 82, "'module_semantic_tokens.no_such_record'", None);
}

/// `sem_tokens_shim` re-exports without defining, the shape `odoo/fields/__init__.py` has since
/// 18.1. The names it exposes are import variables, so without following them `PlainClass` would
/// be coloured `property` — this is the regression guard for that.
fn check_import_shim_tokens(main: &TokenFile) {
    check(main, 76, "PlainClass", Some((CLASS, &[])));
    check(main, 77, "module_level_func", Some((METHOD, &[])));
}

/// Module names resolve only inside a manifest: a dependency by directory name, and the module's
/// own name from the `name` key.
fn check_manifest_module_strings(manifest: &TokenFile) {
    check(manifest, 6, "'module_1'", Some((NAMESPACE, &[])));
    check(manifest, 3, "'Module for Semantic Tokens'", Some((NAMESPACE, &[])));
    check(manifest, 3, "'name'", None);
    check(manifest, 5, "'Test Module for Semantic Tokens'", None);
    check(manifest, 8, "'LGPL-3'", None);
    check(manifest, 10, "'data/sem_tokens_records.xml'", None);
    // A manifest dict KEY that happens to be a module's name is coloured as if it were a
    // module. `website` holds a URL and has nothing to do with the `website`
    // addon, and we should NOT emit a token for it. Definition has the same problem.
    check_todo(manifest, 12, "'website'", None);
    check(manifest, 12, "'https://www.example.com'", None);
}

/// One decoded semantic token: absolute position, and the legend entries its indices name.
struct Tok {
    line: u32,
    start: u32,
    len: u32,
    typ: SemanticTokenType,
    mods: Vec<SemanticTokenModifier>,
}

/// The semantic tokens of one file, next to its source lines so assertions can name an
/// identifier instead of a column.
struct TokenFile {
    path: String,
    lines: Vec<String>,
    tokens: Vec<Tok>,
}

impl TokenFile {
    fn load(session: &mut SessionInfo, path: &str) -> TokenFile {
        let params = SemanticTokensParams {
            text_document: TextDocumentIdentifier { uri: FileMgr::pathname2uri(path) },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let tokens = match Odoo::handle_semantic_tokens(session, params) {
            Ok(Some(SemanticTokensResult::Tokens(tokens))) => tokens,
            other => panic!("expected semantic tokens for {path}, got {other:?}"),
        };
        // The wire format is delta-encoded and indexes into the legend; decoding through
        // `legend()` is also what keeps the indices and the legend in sync.
        let legend = SemanticTokensFeature::legend();
        let (mut line, mut start) = (0, 0);
        let decoded = tokens.data.iter().map(|token| {
            line += token.delta_line;
            start = if token.delta_line == 0 { start + token.delta_start } else { token.delta_start };
            Tok {
                line,
                start,
                len: token.length,
                typ: legend.token_types.get(token.token_type as usize)
                    .unwrap_or_else(|| panic!("token type {} is not in the legend", token.token_type)).clone(),
                mods: legend.token_modifiers.iter().enumerate()
                    .filter(|(bit, _)| token.token_modifiers_bitset & (1u32 << *bit) != 0)
                    .map(|(_, modifier)| modifier.clone())
                    .collect(),
            }
        }).collect();
        let content = fs::read_to_string(path).unwrap_or_else(|error| panic!("unable to read {path}: {error}"));
        TokenFile { path: path.to_string(), lines: content.lines().map(str::to_string).collect(), tokens: decoded }
    }
}

/// Assert what covers `needle` on `line` (1-based, as displayed in an editor): `None` when the
/// identifier must be left to the client grammar, `Some((type, modifiers))` for an exact match —
/// modifiers included, in legend order. A `None` expectation fails on any *overlapping* token, so
/// a wider one (a whole quoted string) cannot slip through.
fn check(file: &TokenFile, line: u32, needle: &str, expected: Option<(SemanticTokenType, &[SemanticTokenModifier])>) {
    let (line_index, column) = locate(file, line, needle);
    let (start, end) = (column, column + needle.len() as u32);
    let overlapping: Vec<&Tok> = file.tokens.iter()
        .filter(|tok| tok.line == line_index && tok.start < end && start < tok.start + tok.len)
        .collect();
    let found = describe(&overlapping);
    match expected {
        None => assert!(overlapping.is_empty(), "{}:{line}: expected no token on `{needle}`, found {found}", file.path),
        Some((typ, mods)) => {
            assert_eq!(overlapping.len(), 1, "{}:{line}: expected one token on `{needle}`, found {found}", file.path);
            let tok = overlapping[0];
            assert!(tok.start == start && tok.len == needle.len() as u32,
                "{}:{line}: expected a token on columns {start}..{end} of `{needle}`, found {found}", file.path);
            assert_eq!(tok.typ, typ, "{}:{line}: wrong token type on `{needle}`, found {found}", file.path);
            assert_eq!(tok.mods.as_slice(), mods, "{}:{line}: wrong token modifiers on `{needle}`, found {found}", file.path);
        }
    }
}

/// Inverts the result of `check`.
/// Useful for a bug detected but not yet fixed: it will fail when the bug is
/// fixed, so `check_todo` can be toggled to `check`.
fn check_todo(file: &TokenFile, line: u32, needle: &str, expected: Option<(SemanticTokenType, &[SemanticTokenModifier])>) {
    // Outside the catch on purpose: a fixture that drifted must fail in both cases (check and check_todo)
    locate(file, line, needle);
    if std::panic::catch_unwind(|| check(file, line, needle, expected)).is_ok() {
        panic!("{}:{line}: `{needle}` gets its expected token — toggle check_todo -> check", file.path);
    }
}

/// Line index and start column of `needle`, which must appear exactly once on the line — that
/// constraint is what lets the assertions above stay readable.
fn locate(file: &TokenFile, line: u32, needle: &str) -> (u32, u32) {
    let line_index = line.checked_sub(1).expect("line numbers are 1-based");
    let text = file.lines.get(line_index as usize)
        .unwrap_or_else(|| panic!("{}: line {line} is past the end of the file", file.path));
    // Columns are LSP encoding units, byte offsets are what `find` returns: equal only in ASCII.
    assert!(text.is_ascii(), "{}:{line}: test fixtures must stay ASCII, got `{text}`", file.path);
    let occurrences = text.match_indices(needle).count();
    assert_eq!(occurrences, 1, "{}:{line}: `{needle}` must appear exactly once on `{text}`", file.path);
    (line_index, text.find(needle).unwrap() as u32)
}

fn describe(tokens: &[&Tok]) -> String {
    if tokens.is_empty() {
        return "no token".to_string();
    }
    tokens.iter().map(|tok| {
        let mods: Vec<&str> = tok.mods.iter().map(|modifier| modifier.as_str()).collect();
        format!("{}..{} {}{}", tok.start, tok.start + tok.len, tok.typ.as_str(),
            if mods.is_empty() { String::new() } else { format!("+{}", mods.join("+")) })
    }).collect::<Vec<String>>().join(", ")
}
