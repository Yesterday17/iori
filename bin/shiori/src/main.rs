use clap::{Parser, Subcommand};
use clap_handler::Handler;
use shiori::commands;

#[derive(Parser, clap_handler::Handler, Clone)]
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
    let args = ShioriArgs::parse();
    args.run().await
}
