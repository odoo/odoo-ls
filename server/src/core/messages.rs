use tower_lsp::lsp_types::Diagnostic;
use url::Url;

#[derive(Debug)]
pub enum Msg {
    LOG_INFO(String), //send a log INFO to client
    LOG_WARNING(String), //send a log WARNING to client
    LOG_ERROR(String), //send a log ERROR to client
    DIAGNOSTIC(MsgDiagnostic), //send a diagnostic to client
    MPSC_SHUTDOWN(), //Shutdown the mpsc channel. No message can be sent afterthat
}

#[derive(Debug)]
pub struct MsgDiagnostic {
    pub uri: Url,
    pub diags: Vec<Diagnostic>,
    pub version: Option<i32>,
}