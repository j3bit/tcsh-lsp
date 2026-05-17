use std::sync::Once;
use tracing_subscriber::{EnvFilter, fmt};

static INIT: Once = Once::new();

/// Initialize structured-ish logs on stderr only, preserving stdout for LSP frames.
pub fn init_tracing() {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("tcsh_lsp=info,tower_lsp=warn"));
        let _ = fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_target(true)
            .with_ansi(false)
            .try_init();
    });
}
