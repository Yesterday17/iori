use clap::Subcommand;
use clap_handler::Handler;

mod download;

#[derive(Subcommand, Clone, Handler)]
pub enum ShioriCommand {
    Download(download::DownloadCommand),
}
