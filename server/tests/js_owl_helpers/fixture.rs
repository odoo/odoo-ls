use std::path::PathBuf;

use lsp_types::{
    Position,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Uri,
};
use odoo_ls_server::core::file_mgr::FileMgr;
use odoo_ls_server::core::odoo::Odoo;
use odoo_ls_server::threads::SessionInfo;
use odoo_ls_server::utils::PathSanitizer;

pub struct Fixtures {
    // Opened fixtures: the test opens them, so tsserver holds them as roots
    /// greeting.js, home of Greeting component
    pub js: FixtureFile,
    /// greeting.xml, home of Greeting template
    pub xml: FixtureFile,
    /// loud_greeting.js, home of LoudGreeting component, which extends Greeting
    pub sub_js: FixtureFile,
    /// loud_greeting.xml, home of LoudGreeting template
    pub sub_xml: FixtureFile,
    
    // Unopened fixtures: the test never opens them, so they are never a tsserver root of their own.
    /// unopened: quiet_greeting.js, home of QuietGreeting component, which extends Greeting
    pub quiet_js: FixtureFile,
    /// unopened: greeting_report.js, no component, imports Greeting
    pub report_js: FixtureFile,
    /// unopened: greeting_ext.xml, templates that t-calls and t-inherits Greeting template
    pub ext_xml: FixtureFile,
}

impl Fixtures {
    pub fn init(session: &mut SessionInfo) -> Self {
        let js = FixtureFile::open(session, &["greeting", "greeting.js"]);
        let xml = FixtureFile::open(session, &["greeting", "greeting.xml"]);
        let sub_js = FixtureFile::open(session, &["greeting", "loud_greeting.js"]);
        let sub_xml = FixtureFile::open(session, &["greeting", "loud_greeting.xml"]);
        let quiet_js = FixtureFile::unopened(&["greeting", "quiet_greeting.js"]);
        let report_js = FixtureFile::unopened(&["greeting", "greeting_report.js"]);
        let ext_xml = FixtureFile::unopened(&["greeting", "greeting_ext.xml"]);
        Fixtures { js, xml, sub_js, sub_xml, quiet_js, report_js, ext_xml }
    }
}


/// A fixture file of `module_owl`
pub struct FixtureFile {
    pub path: String,
    uri: Uri,
    content: String,
}

impl FixtureFile {
    /// `didOpen` the file the way a client would: tsserver then holds its content and pins it
    /// as a project root
    fn open(session: &mut SessionInfo, relative: &[&str]) -> Self {
        let fixture = FixtureFile::unopened(relative);
        let language_id = if fixture.path.ends_with(".xml") { "xml" } else { "javascript" };
        Odoo::handle_did_open(session, lsp_types::DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: fixture.uri.clone(),
                language_id: language_id.to_string(),
                version: 1,
                text: fixture.content.clone(),
            },
        });
        fixture
    }

    /// A fixture the test never opens, so it is never a tsserver root of its own: only the
    /// reference-root expansion can reach it.
    fn unopened(relative: &[&str]) -> Self {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests").join("data").join("addons").join("module_owl").join("static").join("src");
        for part in relative {
            path = path.join(part);
        }
        let path = path.sanitize();
        let content = std::fs::read_to_string(&path).expect("fixture file should be readable");
        let uri = FileMgr::pathname2uri(&path);
        FixtureFile { path, uri, content }
    }

    /// Position of the `|` caret in `snippet`, which must occur exactly once in the file.
    pub fn caret(&self, snippet: &str) -> Position {
        let caret = snippet.find('|').expect("snippet should carry a `|` caret");
        let text = snippet.replace('|', "");
        let mut occurrences = self.content.match_indices(&text);
        let Some((start, _)) = occurrences.next() else {
            panic!("{snippet:?} not found in {}", self.path)
        };
        assert!(
            occurrences.next().is_none(),
            "{snippet:?} occurs more than once in {}",
            self.path
        );
        // `text[..caret]` is `snippet[..caret]` byte for byte, so the caret offset carries over.
        let offset = start + caret;
        let line_start = self.content[..offset].rfind('\n').map_or(0, |nl| nl + 1);
        Position::new(
            self.content[..offset].matches('\n').count() as u32,
            self.content[line_start..offset].encode_utf16().count() as u32,
        )
    }

    pub fn position_params(&self, snippet: &str) -> TextDocumentPositionParams {
        TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: self.uri.clone() },
            position: self.caret(snippet),
        }
    }
}
