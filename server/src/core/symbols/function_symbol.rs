use tower_lsp::lsp_types::Diagnostic;

#[derive(Debug)]
pub struct FunctionSymbol {
    pub is_static: bool,
    pub is_property: bool,
    pub diagnostics: Vec<Diagnostic>, //only temporary used for CLASS and FUNCTION to be collected like others are stored on FileInfo
    //TODO ??
}