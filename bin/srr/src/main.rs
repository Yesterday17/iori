use iori::{
    cache::{
        opendal::{
            services::{self},
            Operator,
        },
        IoriCache,
    },
    download::ParallelDownloaderBuilder,
    hls::CommonM3u8LiveSource,
    merge::IoriMerger,
    HttpClient,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
                .try_from_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let client = iori_showroom::ShowRoomClient::new(None);
    let room_id = client.get_id_by_room_name("plusnewidol").await?;
    let stream = client.live_streaming_url(room_id).await?;
    let stream = stream.best(false);

    let operator = Operator::new(
        services::S3::default()
            .bucket("showroom")
            .endpoint("https://<account_id>.r2.cloudflarestorage.com")
            .access_key_id("")
            .secret_access_key("")
            .root("/")
            .region("auto")
            .delete_max_size(700)
            .disable_stat_with_override(),
    )?
    .finish();

    let client = HttpClient::default();
    let source = CommonM3u8LiveSource::new(client, stream.url.clone(), None, None);

    let cache = IoriCache::opendal(operator, "test");
    let merger = IoriMerger::skip();
    ParallelDownloaderBuilder::new()
        .cache(cache)
        .merger(merger)
        .download(source)
        .await?;

    Ok(())
}
