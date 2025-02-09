use super::inspect::get_default_external_inspector;
use crate::inspect::{InspectPlaylist, PlaylistType};
use clap::{Args, Parser};
use clap_handler::{handler, Handler};
use fake_user_agent::get_chrome_rua;
use iori::{
    cache::IoriCache, dash::archive::CommonDashArchiveSource, detect_manifest_type,
    download::ParallelDownloaderBuilder, hls::CommonM3u8LiveSource, merge::IoriMerger,
};
use iori_nicolive::source::NicoTimeshiftSource;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client,
};
use std::{
    num::NonZeroU32,
    path::PathBuf,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Parser, Clone, Default)]
#[clap(name = "download", visible_alias = "dl", short_flag = 'D')]
pub struct DownloadCommand {
    #[clap(flatten)]
    pub http: HttpOptions,

    #[clap(flatten)]
    pub download: DownloadOptions,

    #[clap(flatten)]
    pub cache: CacheOptions,

    #[clap(flatten)]
    pub output: OutputOptions,

    #[clap(flatten)]
    pub decrypt: DecryptOptions,

    #[clap(skip)]
    pub extra: ExtraOptions,

    #[clap(short, long)]
    wait: bool,

    /// URL to download
    pub url: String,
}

impl DownloadCommand {
    pub async fn download(self) -> anyhow::Result<()> {
        if !self.extra.skip_inspector {
            let inspectors = get_default_external_inspector()?;
            let (_, data) = crate::inspect::inspect(
                &self.url,
                inspectors,
                |c| c.into_iter().next().unwrap(),
                self.wait,
            )
            .await?;
            for playlist in data {
                let command: Self = playlist.into();
                self.clone().merge(command).run().await?;
            }
            return Ok(());
        }

        let client = self.http.into_client();

        let (is_m3u8, initial_playlist_data) = match self.extra.playlist_type {
            Some(PlaylistType::HLS) => (true, self.extra.initial_playlist_data),
            Some(PlaylistType::DASH) => (false, self.extra.initial_playlist_data),
            None => detect_manifest_type(&self.url, client.clone())
                .await
                .unwrap_or((true, None)),
        };

        let downloader = ParallelDownloaderBuilder::new()
            .concurrency(self.download.concurrency)
            .retries(self.download.segment_retries)
            .cache(self.cache.into_cache())
            .merger(self.output.into_merger(!is_m3u8));

        if self.url.contains("dmc.nico") {
            log::info!("Enhanced mode for Nico-TS enabled");

            let key = self
                .decrypt
                .key
                .as_deref()
                .expect("Key is required for Nico-TS");
            let (audience_token, quality) =
                key.split_once(',').unwrap_or_else(|| (&key, "super_high"));
            log::debug!("audience_token: {audience_token}, quality: {quality}");

            let (live_id, _) = audience_token
                .split_once('_')
                .unwrap_or((audience_token, ""));
            let is_channel_live = !live_id.starts_with("lv");
            let wss_url = if is_channel_live {
                format!("wss://a.live2.nicovideo.jp/unama/wsapi/v2/watch/{live_id}/timeshift?audience_token={audience_token}")
            } else {
                format!("wss://a.live2.nicovideo.jp/wsapi/v2/watch/{live_id}/timeshift?audience_token={audience_token}")
            };

            let source = NicoTimeshiftSource::new(client, wss_url)
                .await?
                .with_retry(self.download.manifest_retries);
            downloader.download(source).await?;
        } else if is_m3u8 {
            let source = CommonM3u8LiveSource::new(
                client,
                self.url,
                initial_playlist_data,
                self.decrypt.key.as_deref(),
                self.decrypt.shaka_packager_command,
            )
            .with_retry(self.download.manifest_retries);
            downloader.download(source).await?;
        } else {
            let source =
                CommonDashArchiveSource::new(client, self.url, self.decrypt.key.as_deref())?;
            downloader.download(source).await?;
        }

        Ok(())
    }

    fn merge(mut self, from: Self) -> Self {
        self.url = from.url;
        self.http.headers.extend(from.http.headers);
        if self.decrypt.key.is_none() {
            self.decrypt.key = from.decrypt.key;
        }

        self.extra.skip_inspector = true;
        self.extra.initial_playlist_data = None;

        self
    }
}

#[derive(Args, Clone, Debug)]
pub struct HttpOptions {
    /// Additional HTTP headers
    #[clap(short = 'H', long = "header")]
    pub headers: Vec<String>,

    /// HTTP timeout, in seconds
    #[clap(short, long, default_value = "10")]
    pub timeout: u64,
}

impl HttpOptions {
    pub fn into_client(self) -> Client {
        let mut headers = HeaderMap::new();

        for header in &self.headers {
            let (key, value) = header.split_once(':').expect("Invalid header");
            headers.insert(
                HeaderName::from_str(key).expect("Invalid header name"),
                HeaderValue::from_str(value).expect("Invalid header value"),
            );
        }

        Client::builder()
            .default_headers(headers)
            .user_agent(get_chrome_rua())
            .timeout(Duration::from_secs(self.timeout))
            .build()
            .unwrap()
    }
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            headers: Vec::new(),
            timeout: 10,
        }
    }
}

#[derive(Args, Clone, Debug)]
pub struct DownloadOptions {
    /// Threads limit
    #[clap(long, alias = "threads", default_value = "5")]
    pub concurrency: NonZeroU32,

    /// Segment retry limit
    #[clap(long, default_value = "5")]
    pub segment_retries: u32,

    /// Manifest retry limit
    #[clap(long, default_value = "3")]
    pub manifest_retries: u32,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            concurrency: NonZeroU32::new(5).unwrap(),
            segment_retries: 5,
            manifest_retries: 3,
        }
    }
}

#[derive(Args, Clone, Debug, Default)]
pub struct CacheOptions {
    /// Use in-memory cache and do not write cache to disk while downloading
    #[clap(short = 'm', long)]
    pub in_memory_cache: bool,

    /// Temporary directory
    #[clap(long, env = "TEMP")]
    pub temp_dir: Option<PathBuf>,

    /// Cache directory
    #[clap(long)]
    pub cache_dir: Option<PathBuf>,
}

impl CacheOptions {
    pub fn into_cache(self) -> IoriCache {
        if self.in_memory_cache {
            IoriCache::memory()
        } else if let Some(cache_dir) = self.cache_dir {
            IoriCache::file(cache_dir)
        } else {
            let mut cache_dir = self.temp_dir.unwrap_or_else(|| std::env::temp_dir());

            let started_at = SystemTime::now();
            let started_at = started_at.duration_since(UNIX_EPOCH).unwrap().as_millis();
            cache_dir.push(format!("shiori_{started_at}_{}", rand::random::<u8>()));

            IoriCache::file(cache_dir)
        }
    }
}

/// Decrypt related arguments
#[derive(Args, Clone, Debug, Default)]
pub struct DecryptOptions {
    #[clap(long = "key")]
    pub key: Option<String>,

    #[clap(long = "shaka-packager", visible_alias = "shaka")]
    pub shaka_packager_command: Option<PathBuf>,
}

#[derive(Clone, Default)]
pub struct ExtraOptions {
    /// Force Dash mode
    pub playlist_type: Option<PlaylistType>,

    /// Initial playlist data
    pub initial_playlist_data: Option<String>,

    pub skip_inspector: bool,
}

/// Output options
#[derive(Args, Clone, Debug, Default)]
#[group(required = true, multiple = false)]
pub struct OutputOptions {
    /// Do not merge stream
    #[clap(long)]
    pub no_merge: bool,

    /// Write stream to a file
    #[clap(short, long)]
    pub output: Option<PathBuf>,

    /// Write to stdout
    #[clap(short = 'P', long)]
    pub pipe: bool,

    /// Pipe to a file
    #[clap(long)]
    pub pipe_to: Option<PathBuf>,
}

impl OutputOptions {
    pub fn into_merger(self, is_dash: bool) -> IoriMerger {
        if self.no_merge {
            IoriMerger::skip()
        } else if let Some(output) = self.output {
            if is_dash {
                IoriMerger::mkvmerge(output, false)
            } else {
                IoriMerger::concat(output, false)
            }
        } else if self.pipe {
            IoriMerger::pipe(true)
        } else if let Some(pipe) = self.pipe_to {
            IoriMerger::pipe_to_file(true, pipe)
        } else {
            unreachable!()
        }
    }
}

#[handler(DownloadCommand)]
pub async fn download(args: DownloadCommand) -> anyhow::Result<()> {
    args.download().await
}

impl From<InspectPlaylist> for DownloadCommand {
    fn from(data: InspectPlaylist) -> Self {
        Self {
            http: HttpOptions {
                headers: data.headers,
                ..Default::default()
            },
            decrypt: DecryptOptions {
                key: data.key,
                ..Default::default()
            },
            extra: ExtraOptions {
                playlist_type: Some(data.playlist_type),
                initial_playlist_data: data.initial_playlist_data,
                skip_inspector: true,
            },
            url: data.playlist_url,

            ..Default::default()
        }
    }
}
