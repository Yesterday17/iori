use clap::Parser;
use clap_handler::Handler;

mod commands;

#[derive(Parser, clap_handler::Handler, Clone)]
struct ShioriArgs {
    #[clap(subcommand)]
    command: commands::ShioriCommand,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = ShioriArgs::parse();
    args.run().await
}
