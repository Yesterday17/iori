use clap::Parser;
use clap_handler::Handler;
use shiori::commands::ShioriArgs;
use tracing_subscriber::filter::LevelFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .try_from_env()
                .unwrap_or_else(|_| "i18n_embed::requester=off".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    ShioriArgs::parse().run().await
}
