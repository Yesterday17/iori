use super::inspect::get_default_external_inspector;
use crate::{
    commands::{update::check_update, ShioriArgs},
    inspect::{InspectPlaylist, PlaylistType},
};
use clap::{Args, Parser};
use clap_handler::handler;
use fake_user_agent::get_chrome_rua;
use iori::{
    cache::IoriCache, dash::archive::CommonDashArchiveSource, detect_manifest_type,
    download::ParallelDownloaderBuilder, hls::CommonM3u8LiveSource, merge::IoriMerger, HttpClient,
};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, IntoUrl,
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
    pub wait: bool,

    /// Additional arguments
    ///
    /// Format: key=value
    #[clap(short = 'e', long = "arg")]
    pub extra_args: Vec<String>,

    /// URL to download
    pub url: String,
}

impl DownloadCommand {
    pub async fn download(self) -> anyhow::Result<()> {
        let client = self.http.into_client(&self.url);

        let is_m3u8 = match self.extra.playlist_type {
            Some(PlaylistType::HLS) => true,
            Some(PlaylistType::DASH) => false,
            None => detect_manifest_type(&self.url, client.clone())
                .await
                .unwrap_or(true),
        };

        let downloader = ParallelDownloaderBuilder::new()
            .concurrency(self.download.concurrency)
            .retries(self.download.segment_retries)
            .cache(self.cache.into_cache()?)
            .merger(self.output.into_merger());

        if is_m3u8 {
            let source = CommonM3u8LiveSource::new(
                client,
                self.url,
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
        self.http.cookies.extend(from.http.cookies);
        if self.decrypt.key.is_none() {
            self.decrypt.key = from.decrypt.key;
        }

        self
    }
}

#[derive(Args, Clone, Debug)]
pub struct HttpOptions {
    /// Additional HTTP headers
    #[clap(short = 'H', long = "header")]
    pub headers: Vec<String>,

    /// Advanced: Additional HTTP cookies
    ///
    /// Will not take effect if `Cookies` is set in [headers].
    ///
    /// Do not use this option unless you know what you are doing.
    #[clap(long = "cookie")]
    pub cookies: Vec<String>,

    /// HTTP timeout, in seconds
    #[clap(short, long, default_value = "10")]
    pub timeout: u64,
}

impl HttpOptions {
    pub fn into_client(self, url: impl IntoUrl) -> HttpClient {
        let mut headers = HeaderMap::new();

        for header in &self.headers {
            let (key, value) = header.split_once(':').expect("Invalid header");
            headers.insert(
                HeaderName::from_str(key).expect("Invalid header name"),
                HeaderValue::from_str(value).expect("Invalid header value"),
            );
        }

        let client = HttpClient::new(
            Client::builder()
                .default_headers(headers)
                .user_agent(get_chrome_rua())
                .timeout(Duration::from_secs(self.timeout))
                .danger_accept_invalid_certs(true),
        );
        client.add_cookies(self.cookies, url);
        client
    }
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            headers: Vec::new(),
            cookies: Vec::new(),
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
    pub fn into_cache(self) -> anyhow::Result<IoriCache> {
        Ok(if self.in_memory_cache {
            IoriCache::memory()
        } else if let Some(cache_dir) = self.cache_dir {
            IoriCache::file(cache_dir)?
        } else {
            let mut cache_dir = self.temp_dir.unwrap_or_else(|| std::env::temp_dir());

            let started_at = SystemTime::now();
            let started_at = started_at.duration_since(UNIX_EPOCH).unwrap().as_millis();
            cache_dir.push(format!("shiori_{started_at}_{}", rand::random::<u8>()));

            IoriCache::file(cache_dir)?
        })
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
}

/// Output options
#[derive(Args, Clone, Debug, Default)]
#[group(required = true, multiple = false)]
pub struct OutputOptions {
    /// Do not merge stream
    #[clap(long)]
    pub no_merge: bool,

    #[clap(long)]
    pub concat: bool,

    /// Write stream to a file
    #[clap(short, long)]
    pub output: Option<PathBuf>,

    /// Write to stdout
    #[clap(short = 'P', long)]
    pub pipe: bool,

    /// Pipe to a file and mux with ffmpeg
    #[clap(short = 'M', long)]
    pub pipe_mux: bool,

    /// Pipe to a file
    #[clap(long)]
    pub pipe_to: Option<PathBuf>,
}

impl OutputOptions {
    pub fn into_merger(self) -> IoriMerger {
        if self.no_merge {
            IoriMerger::skip()
        } else if let Some(output) = self.output {
            if self.concat {
                IoriMerger::concat(output, false)
            } else {
                IoriMerger::auto(output, false)
            }
        } else if self.pipe {
            IoriMerger::pipe(true)
        } else if self.pipe_mux {
            IoriMerger::pipe_mux(true, "-".into(), None)
        } else if let Some(pipe) = self.pipe_to {
            IoriMerger::pipe_to_file(true, pipe)
        } else {
            unreachable!()
        }
    }
}

#[handler(DownloadCommand)]
pub async fn download(me: DownloadCommand, shiori_args: ShioriArgs) -> anyhow::Result<()> {
    let inspectors = get_default_external_inspector(&me.extra_args)?;
    let (_, data) = crate::inspect::inspect(
        &me.url,
        inspectors,
        |c| c.into_iter().next().unwrap(),
        me.wait,
    )
    .await?;
    for playlist in data {
        let command: DownloadCommand = playlist.into();
        me.clone().merge(command).download().await?;
    }

    // Check for update, but do not throw error if failed
    if shiori_args.update_check {
        _ = check_update().await;
    }
    Ok(())
}

impl From<InspectPlaylist> for DownloadCommand {
    fn from(data: InspectPlaylist) -> Self {
        Self {
            http: HttpOptions {
                headers: data.headers,
                cookies: data.cookies,
                ..Default::default()
            },
            decrypt: DecryptOptions {
                key: data.key,
                ..Default::default()
            },
            extra: ExtraOptions {
                playlist_type: Some(data.playlist_type),
            },
            url: data.playlist_url,

            ..Default::default()
        }
    }
}
