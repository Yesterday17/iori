use clap::{ArgAction, Parser, Subcommand};
use clap_handler::Handler;

pub mod download;
pub mod inspect;
pub mod merge;
pub mod update;

#[derive(Parser, clap_handler::Handler, Clone)]
#[clap(version = env!("SHIORI_VERSION"), author)]
pub struct ShioriArgs {
    /// Whether to skip update check
    #[clap(long = "skip-update", action = ArgAction::SetFalse)]
    update_check: bool,

    #[clap(subcommand)]
    command: ShioriCommand,
}

#[derive(Subcommand, Clone, Handler)]
pub(crate) enum ShioriCommand {
    Download(download::DownloadCommand),
    Inspect(inspect::InspectCommand),
    Merge(merge::MergeCommand),
    Update(update::UpdateCommand),
}
