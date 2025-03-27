use clap::{ArgAction, Parser, Subcommand};
use clap_handler::Handler;

pub mod download;
pub mod inspect;
pub mod merge;
pub mod update;

#[derive(Parser, Handler, Clone)]
#[clap(version = env!("SHIORI_VERSION"), author)]
pub struct ShioriArgs {
    /// Whether to skip update check
    #[clap(long = "skip-update", action = ArgAction::SetFalse)]
    update_check: bool,

    #[clap(subcommand)]
    command: ShioriCommand,
}

#[derive(Subcommand, Handler, Clone)]
pub enum ShioriCommand {
    Download(download::DownloadCommand),
    Inspect(inspect::InspectCommand),
    Merge(merge::MergeCommand),
    Update(update::UpdateCommand),
}
