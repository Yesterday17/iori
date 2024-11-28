use clap::Parser;
use iori_minyami::MinyamiArgs;

#[tokio::main(worker_threads = 8)]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    MinyamiArgs::parse().run().await?;

    Ok(())
}
