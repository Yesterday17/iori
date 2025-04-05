use clap::{builder::styling, ArgAction, Parser, Subcommand};
use clap_handler::Handler;

pub mod download;
pub mod inspect;
pub mod merge;
pub mod update;

pub const STYLES: styling::Styles = styling::Styles::styled()
    .header(styling::AnsiColor::Green.on_default().bold().underline())
    .usage(styling::AnsiColor::Green.on_default().bold())
    .literal(styling::AnsiColor::Blue.on_default().bold())
    .placeholder(styling::AnsiColor::Cyan.on_default());

#[derive(Parser, Handler, Clone)]
#[clap(version = env!("SHIORI_VERSION"), author, styles = STYLES)]
pub struct ShioriArgs {
    /// Whether to skip update check
    #[clap(long = "skip-update", action = ArgAction::SetFalse)]
    update_check: bool,

    #[clap(subcommand)]
    command: ShioriCommand,
}

#[derive(Subcommand, Handler, Clone)]
pub enum ShioriCommand {
    #[clap(after_long_help = inspect::get_default_external_inspector().help())]
    Download(download::DownloadCommand),
    #[clap(after_long_help = inspect::get_default_external_inspector().help())]
    Inspect(inspect::InspectCommand),
    Merge(merge::MergeCommand),
    Update(update::UpdateCommand),
}
