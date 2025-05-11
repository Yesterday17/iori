use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
mod config;

use config::Config;
use iori::{
    cache::{
        opendal::{Configurator, Operator},
        IoriCache,
    },
    download::ParallelDownloaderBuilder,
    hls::CommonM3u8LiveSource,
    merge::IoriMerger,
    HttpClient,
};
use iori_showroom::ShowRoomClient;
use tokio::signal::unix::{signal, SignalKind};
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;

async fn update_config(
    sched: &mut JobScheduler,
    room_slugs: Vec<String>,
    map: &mut HashMap<String, Uuid>,
    operator: Operator,
) -> anyhow::Result<()> {
    let mut lock = HashMap::<String, AtomicBool>::new();
    for room_slug in room_slugs.iter() {
        lock.insert(room_slug.clone(), AtomicBool::new(false));
    }
    let lock = Arc::new(lock);

    // missing: exists in room_slugs, but does not exist in map
    let missing_slugs: Vec<_> = room_slugs
        .clone()
        .into_iter()
        .filter(|r| !map.contains_key(r))
        .collect();
    for slug in missing_slugs {
        if !map.contains_key(&slug) {
            let operator = operator.clone();
            let client = ShowRoomClient::new(None).await?;
            let client_backup = ShowRoomClient::new(None).await?;
            let room_id = client.get_id_by_room_slug(&slug).await?;
            let room_slug = slug.clone();
            let lock = lock.clone();
            let uuid = sched
                .add(Job::new_async("1/30 * * * * *", move |_, _| {
                    let operator = operator.clone();

                    let clients = vec![client.clone(), client_backup.clone()];
                    let index = AtomicUsize::new(0);

                    let room_slug = room_slug.clone();
                    let lock = lock.clone();
                    Box::pin(async move {
                        let lock = lock.get(&room_slug).unwrap();
                        let client = clients[index.load(Ordering::Relaxed) % clients.len()].clone();
                        let was_locked = lock.fetch_or(true, Ordering::Relaxed);

                        if !was_locked {
                            if let Err(e) =
                                record_room(client.clone(), &room_slug, room_id, operator).await
                            {
                                log::error!("Failed to record room {room_slug}: {e}");

                                index.fetch_add(1, Ordering::Relaxed);
                                tokio::time::sleep(Duration::from_secs(20)).await;
                            }
                            lock.fetch_and(false, Ordering::Relaxed);
                        }
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

async fn record_room(
    client: ShowRoomClient,
    room_slug: &str,
    room_id: u64,
    operator: Operator,
) -> anyhow::Result<()> {
    log::debug!("Attempt to record room {room_slug}, id = {room_id}");

    let room_info = client.room_profile(room_id).await?;
    if !room_info.is_live() {
        log::debug!("Room {room_slug} is not live, skipping...");
        return Ok(());
    }

    let stream = client.live_streaming_url(room_id).await?;
    let Some(stream) = stream.best(false) else {
        log::debug!("Room {room_slug} is not live, skipping...");
        return Ok(());
    };

    let live_id = room_info.live_id;
    let live_started_at = chrono::DateTime::from_timestamp(room_info.current_live_started_at, 0)
        .unwrap()
        .with_timezone(&chrono_tz::Asia::Tokyo)
        .to_rfc3339();
    let prefix = format!("{room_slug}/{live_id}_{live_started_at}");

    let client = HttpClient::default();
    let source = CommonM3u8LiveSource::new(client, stream.url.clone(), None, None);

    let cache = IoriCache::opendal(operator.clone(), prefix, false);
    let merger = IoriMerger::skip();

    log::info!("Start recording room {room_slug}, id = {room_id}, live_id = {live_id}");
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
                .unwrap_or_else(|_| {
                    "info,tokio_cron_scheduler=warn,iori::hls=warn,iori::download=warn".into()
                }),
        )
        .with_writer(std::io::stderr)
        .init();

    let config = Config::load()?;

    let operator = Operator::new(config.s3.into_builder())?.finish();
    let mut watchers = HashMap::<String, Uuid>::new();
    let mut sched = JobScheduler::new().await?;
    update_config(
        &mut sched,
        config.showroom.rooms,
        &mut watchers,
        operator.clone(),
    )
    .await?;

    let mut sigusr1_stream = signal(SignalKind::user_defined1())?;
    let mut sigint_stream = signal(SignalKind::interrupt())?;

    sched.start().await?;

    loop {
        tokio::select! {
            _ = sigusr1_stream.recv() => {
                log::warn!("SIGUSR1 received. Reloading config...");
                // SIGUSR1 received, reload config
                let config = Config::load()?;
                update_config(
                    &mut sched,
                    config.showroom.rooms,
                    &mut watchers,
                    operator.clone(),
                )
                .await?;
                log::warn!("Config reloaded.");
            }
            _ = sigint_stream.recv() => {
                // SIGINT received, break the loop for graceful shutdown
                log::warn!("SIGINT received. Shutting down...");
                sched.shutdown().await?;
                break;
            }
        }
    }

    Ok(())
}
