#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let code = intl_lens::cli_app::run_from_env().await?;
    std::process::exit(code);
}
