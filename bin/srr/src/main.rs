use std::collections::HashMap;

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
use iori_showroom::ShowRoomClient;
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;

async fn update_config(
    sched: &mut JobScheduler,
    room_slugs: Vec<String>,
    map: &mut HashMap<String, Uuid>,
    operator: Operator,
) -> anyhow::Result<()> {
    // missing: exists in room_slugs, but does not exist in map
    let missing_slugs: Vec<_> = room_slugs
        .clone()
        .into_iter()
        .filter(|r| !map.contains_key(r))
        .collect();
    for slug in missing_slugs {
        if !map.contains_key(&slug) {
            let operator = operator.clone();
            let _slug = slug.clone();
            let uuid = sched
                .add(Job::new_async("1/10 * * * * *", move |_, _| {
                    let operator = operator.clone();
                    let slug = _slug.clone();
                    Box::pin(async move {
                        record_room(slug.clone(), operator).await.unwrap();
                    })
                })?)
                .await?;
            map.insert(slug, uuid);
        }
    }

    // removed: does not exist in room slugs, but in map
    let removed_slugs: Vec<_> = map
        .keys()
        .filter(|k| !room_slugs.contains(k))
        .map(ToString::to_string)
        .collect();
    for slug in removed_slugs {
        if let Some(uuid) = map.remove(&slug) {
            sched.remove(&uuid).await?;
        }
    }

    Ok(())
}

async fn record_room(room_slug: String, operator: Operator) -> anyhow::Result<()> {
    let client = ShowRoomClient::new(None);
    let room_id = client.get_id_by_room_name(&room_slug).await?;
    let stream = client.live_streaming_url(room_id).await?;
    let stream = stream.best(false);

    let room_info = client.live_info(room_id).await?;
    let prefix = format!("{room_slug}/{}", room_info.live_id);

    let client = HttpClient::default();
    let source = CommonM3u8LiveSource::new(client, stream.url.clone(), None, None);

    let cache = IoriCache::opendal(operator.clone(), prefix);
    let merger = IoriMerger::skip();
    ParallelDownloaderBuilder::new()
        .cache(cache)
        .merger(merger)
        .download(source)
        .await?;

    Ok(())
}

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

    let mut watchers = HashMap::<String, Uuid>::new();

    let mut sched = JobScheduler::new().await?;
    update_config(
        &mut sched,
        vec!["48_TAKAO_SAYAKA".to_string()],
        &mut watchers,
        operator.clone(),
    )
    .await?;

    sched.start().await?;

    Ok(())
}
