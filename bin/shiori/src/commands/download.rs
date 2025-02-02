use std::{
    num::NonZeroU32,
    path::PathBuf,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use clap::{Args, Parser};
use clap_handler::handler;
use fake_user_agent::get_chrome_rua;
use iori::{
    cache::IoriCache, dash::archive::CommonDashArchiveSource, download::ParallelDownloader,
    hls::CommonM3u8LiveSource, merge::IoriMerger,
};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client,
};

use crate::inspect::{InspectData, PlaylistType};

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

    /// URL to download
    pub url: String,
}

impl DownloadCommand {
    pub async fn download(self) -> anyhow::Result<()> {
        let client = self.http.into_client();

        let merger = self.output.into_merger();
        let cache = self.cache.into_cache();

        if self.download.dash {
            let source =
                CommonDashArchiveSource::new(client, self.url, self.decrypt.key.as_deref())?;

            ParallelDownloader::new(
                source,
                merger,
                cache,
                self.download.concurrency,
                self.download.segment_retries,
            )
            .download()
            .await?;
        } else {
            let source = CommonM3u8LiveSource::new(
                client,
                self.url,
                self.decrypt.key.as_deref(),
                self.decrypt.shaka_packager_command,
            )
            .with_retry(self.download.manifest_retries);

            ParallelDownloader::new(
                source,
                merger,
                cache,
                self.download.concurrency,
                self.download.segment_retries,
            )
            .download()
            .await?;
        }

        Ok(())
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

    #[clap(long)]
    pub dash: bool,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            concurrency: NonZeroU32::new(5).unwrap(),
            segment_retries: 5,
            manifest_retries: 3,
            dash: false,
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
    pub fn into_merger(self) -> IoriMerger {
        if self.no_merge {
            IoriMerger::skip()
        } else if let Some(output) = self.output {
            IoriMerger::concat(output, false)
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

impl From<InspectData> for DownloadCommand {
    fn from(data: InspectData) -> Self {
        Self {
            http: HttpOptions {
                headers: data.headers,
                ..Default::default()
            },
            decrypt: DecryptOptions {
                key: data.key,
                ..Default::default()
            },
            download: DownloadOptions {
                dash: matches!(data.playlist_type, PlaylistType::DASH),
                ..Default::default()
            },
            url: data.playlist_url,

            ..Default::default()
        }
    }
}
