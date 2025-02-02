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
    cache::IoriCache, download::ParallelDownloader, hls::CommonM3u8LiveSource, merge::IoriMerger,
};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client,
};

#[derive(Parser, Clone)]
#[clap(name = "download", visible_alias = "dl", short_flag = 'd')]
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

        let source = CommonM3u8LiveSource::new(
            client,
            self.url,
            self.decrypt.key.as_deref(),
            self.decrypt.shaka_packager_command,
        )
        .with_retry(self.download.manifest_retries);
        let merger = self.output.into_merger();
        let cache = self.cache.into_cache();

        ParallelDownloader::new(
            source,
            merger,
            cache,
            self.download.concurrency,
            self.download.segment_retries,
        )
        .download()
        .await?;

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

#[derive(Args, Clone, Debug)]
pub struct CacheOptions {
    /// Use in-memory cache and do not write cache to disk while downloading
    #[clap(long)]
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
#[derive(Args, Clone, Debug)]
pub struct DecryptOptions {
    #[clap(long = "key")]
    pub key: Option<String>,

    #[clap(long = "shaka-packager", visible_alias = "shaka")]
    pub shaka_packager_command: Option<PathBuf>,
}

/// Output options
#[derive(Args, Clone, Debug)]
#[group(required = true, multiple = false)]
pub struct OutputOptions {
    /// Do not merge stream
    #[clap(long)]
    pub no_merge: bool,

    /// Write stream to a file
    #[clap(short, long)]
    pub output: Option<PathBuf>,

    /// Write to stdout, and optionally record to a file
    #[clap(long)]
    pub pipe: Option<Option<PathBuf>>,
}

impl OutputOptions {
    pub fn into_merger(self) -> IoriMerger {
        if self.no_merge {
            IoriMerger::skip()
        } else if let Some(output) = self.output {
            IoriMerger::concat(output, false)
        } else if let Some(Some(pipe)) = self.pipe {
            IoriMerger::pipe_to_file(true, pipe)
        } else if let Some(None) = self.pipe {
            IoriMerger::pipe(true)
        } else {
            unreachable!()
        }
    }
}

#[handler(DownloadCommand)]
pub async fn download(args: DownloadCommand) -> anyhow::Result<()> {
    args.download().await
}
