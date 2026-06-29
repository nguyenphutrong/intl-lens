use anyhow::Result;
use intl_lens::backend::I18nBackend;
use tower_lsp::{LspService, Server};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args_os().len() > 1 {
        let code = intl_lens::cli_app::run_from_env().await?;
        std::process::exit(code);
    }

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive("intl_lens=debug".parse()?))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(I18nBackend::new);
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
