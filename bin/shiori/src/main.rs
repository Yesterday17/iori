use clap::Parser;
use clap_handler::Handler;
use shiori::commands::ShioriCommand;

#[derive(Parser, clap_handler::Handler, Clone)]
struct ShioriArgs {
    #[clap(subcommand)]
    command: ShioriCommand,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = ShioriArgs::parse();
    args.run().await
}
