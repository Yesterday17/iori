use clap::{Parser, Subcommand};
use clap_handler::Handler;
use shiori::commands;
use tracing_subscriber::filter::LevelFilter;

#[derive(Parser, clap_handler::Handler, Clone)]
#[clap(version = env!("SHIORI_VERSION"), author)]
struct ShioriArgs {
    #[clap(subcommand)]
    command: ShioriCommand,
}

#[derive(Subcommand, Clone, Handler)]
pub enum ShioriCommand {
    Download(commands::download::DownloadCommand),
    Inspect(commands::inspect::InspectCommand),
    Merge(commands::merge::MergeCommand),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_writer(std::io::stderr)
        .init();
    let args = ShioriArgs::parse();
    args.run().await
}
