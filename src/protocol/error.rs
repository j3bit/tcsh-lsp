use tower_lsp::jsonrpc::{Error, ErrorCode};

/// Create a JSON-RPC/LSP-compatible error. M0 keeps this small but centralizes mapping.
#[allow(dead_code)]
pub fn protocol_error(code: ErrorCode, message: impl Into<String>) -> Error {
    Error {
        code,
        message: message.into().into(),
        data: None,
    }
}
