use tower_lsp::lsp_types::TextDocumentContentChangeEvent;


struct TextDocument {
    version: i32,
    rope: ropey::Rope
}

impl TextDocument {

    pub fn new(version: i32, source: String) -> Self {
        TextDocument {
            version,
            rope: ropey::Rope::from_str(&source)
        }
    }


}