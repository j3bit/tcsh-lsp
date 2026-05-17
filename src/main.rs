use clap::Parser;
use tcsh_lsp::protocol::{Backend, init_tracing};
use tokio::io::{stdin, stdout};
use tower_lsp::{LspService, Server};

#[derive(Debug, Parser)]
#[command(name = "tcsh-lsp", version, about = "tcsh/csh language server")]
struct Args {
    /// Run the language server over stdio. This is the default when no mode is supplied.
    #[arg(long, default_value_t = true)]
    stdio: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    init_tracing();

    if args.stdio {
        let (service, socket) = LspService::new(Backend::new);
        Server::new(stdin(), stdout(), socket).serve(service).await;
    }
}
