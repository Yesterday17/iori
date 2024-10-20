use std::{
    env::current_dir,
    num::NonZeroU32,
    path::PathBuf,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::bail;
use clap::Parser;
use fake_user_agent::get_chrome_rua;
use iori::{
    cache::{file::FileCacheSource, memory::MemoryCacheSource},
    dash::archive::CommonDashArchiveSource,
    download::ParallelDownloader,
    hls::{CommonM3u8ArchiveSource, CommonM3u8LiveSource, SegmentRange},
    merge::IoriMerger,
    StreamingSegment,
};
use iori_nicolive::source::NicoTimeshiftSource;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client,
};

#[derive(Parser, Debug, Clone)]
pub struct MinyamiArgs {
    #[clap(short, long, hide = true)]
    download: bool,

    #[clap(long, hide = true)]
    shaka_packager: Option<PathBuf>,

    /// Debug output
    #[clap(long, alias = "debug")]
    verbose: bool,

    /// Threads limit
    #[clap(long, default_value = "5")]
    threads: NonZeroU32,

    /// Retry limit
    #[clap(long, default_value = "5")]
    retries: u32,

    #[clap(long, default_value = "3")]
    manifest_retries: u32,

    /// [Unimplemented]
    /// Output file path
    #[clap(short, long, default_value = "./output.ts")]
    output: PathBuf,

    /// Temporary file path
    #[clap(long, env = "TEMP")]
    temp_dir: Option<PathBuf>,

    /// Set key manually (Internal use)
    ///
    /// (Optional) Key for decrypt video.
    #[clap(long)]
    key: Option<String>,

    /// Cookies used to download
    #[clap(long)]
    cookies: Option<String>,

    /// HTTP Header used to download
    ///
    /// Custom header. eg. "User-Agent: xxxxx". This option will override --cookies.
    #[clap(short = 'H', long)]
    headers: Vec<String>,

    /// Download live
    #[clap(long)]
    live: bool,

    /// [Unimplemented]
    /// (Optional) Set output format. default: ts
    /// Format name. ts or mkv.
    #[clap(long)]
    format: Option<String>,

    /// [Unimplemented]
    /// Use the specified HTTP/HTTPS/SOCKS5 proxy
    ///
    /// Set proxy in [protocol://<host>:<port>] format. eg. --proxy "http://127.0.0.1:1080".
    #[clap(long)]
    proxy: Option<String>,

    /// [Unimplemented]
    /// Download specified part of the stream
    ///
    /// Set time range in [<hh:mm:ss>-<hh:mm:ss> format]. eg. --slice "45:00-53:00"
    #[clap(long)]
    slice: Option<String>,

    /// Do not merge m3u8 chunks.
    #[clap(long)]
    no_merge: bool,

    /// Keep temporary files.
    #[clap(short, long)]
    keep: bool,

    /// [Unimplemented]
    /// Do not delete encrypted chunks after decryption.
    #[clap(long)]
    keep_encrypted_chunks: bool,

    /// [Unimplemented]
    /// Temporary file naming strategy. Defaults to 1.
    ///
    /// MIXED = 0,
    /// USE_FILE_SEQUENCE = 1,
    /// USE_FILE_PATH = 2,
    #[clap(long, default_value = "1")]
    chunk_naming_strategy: u8,

    /// [Iori Argument]
    /// Specify segment range to download in archive mode.
    #[clap(long, default_value = "-")]
    range: SegmentRange,

    /// [Iori Argument]
    /// Timeout seconds for each manifest/segment request.
    /// Defaults to 10 seconds.
    #[clap(long, default_value = "10")]
    timeout: u64,

    /// [Iori Argument]
    /// Specify the resume folder path
    #[clap(long)]
    resume_dir: Option<PathBuf>,

    /// [Iori Argument]
    /// Pipe live streaming to stdout. Only takes effect in live mode.
    #[clap(long)]
    pipe: bool,

    /// [Iori Argument]
    /// Download with dash format
    #[clap(long)]
    dash: bool,

    /// m3u8 file path
    m3u8: String,
}

impl MinyamiArgs {
    fn client(&self) -> Client {
        let mut headers = HeaderMap::new();
        if let Some(cookies) = &self.cookies {
            headers.insert(
                reqwest::header::COOKIE,
                HeaderValue::from_str(&cookies).expect("Invalid cookie"),
            );
        }

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

    fn temp_dir(&self) -> anyhow::Result<PathBuf> {
        Ok(match &self.temp_dir {
            Some(temp_dir) => {
                if !temp_dir.exists() {
                    log::error!("Temporary path directory does not exist.");
                    bail!("Temporary path directory does not exist.");
                } else {
                    let temp_dir = temp_dir.canonicalize()?;
                    log::info!(
                        "Temporary path sets to ${temp_dir}",
                        temp_dir = temp_dir.display()
                    );
                    temp_dir
                }
            }
            None => std::env::temp_dir(),
        })
    }

    fn output_dir(&self) -> anyhow::Result<PathBuf> {
        let output_dir = if let Some(ref dir) = self.resume_dir {
            dir.clone()
        } else {
            let temp_path = self.temp_dir()?;
            let started_at = SystemTime::now();
            let started_at = started_at.duration_since(UNIX_EPOCH).unwrap().as_millis();
            temp_path.join(format!("minyami_{started_at}"))
        };
        std::fs::create_dir_all(&output_dir)?;
        Ok(output_dir)
    }

    fn merger<S>(&self) -> IoriMerger<S>
    where
        S: StreamingSegment + Send + 'static,
    {
        if self.live && self.pipe {
            IoriMerger::pipe(!self.keep)
        } else if self.no_merge {
            IoriMerger::skip()
        } else {
            let target_file = current_dir().unwrap().join(&self.output);
            IoriMerger::concat(target_file, self.keep)
        }
    }
}

#[tokio::main(worker_threads = 8)]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let args = MinyamiArgs::parse();
    let client = args.client();
    let output_dir = args.output_dir()?;
    let cache = FileCacheSource::new(output_dir.clone());

    if args.live {
        let merger = args.merger();
        // Live Downloader
        let source = CommonM3u8LiveSource::new(client, args.m3u8, args.key, args.shaka_packager)
            .with_retry(args.manifest_retries);
        if args.pipe {
            let cache = MemoryCacheSource::new();
            let downloader =
                ParallelDownloader::new(source, merger, cache, args.threads, args.retries);
            downloader.download().await?;
        } else {
            let downloader =
                ParallelDownloader::new(source, merger, cache, args.threads, args.retries);
            downloader.download().await?;
        }
    } else {
        // Archive Downloader
        if args.m3u8.contains("dmc.nico") {
            let merger = args.merger();
            log::info!("Enhanced mode for Nico-TS enabled");

            let key = args.key.expect("Key is required for Nico-TS");
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
                .with_retry(args.manifest_retries);
            let downloader =
                ParallelDownloader::new(source, merger, cache, args.threads, args.retries);
            downloader.download().await?;
        } else if args.dash {
            let merger = args.merger();
            let source = CommonDashArchiveSource::new(client, args.m3u8, args.key)?;
            let downloader =
                ParallelDownloader::new(source, merger, cache, args.threads, args.retries);
            downloader.download().await?;
        } else {
            let merger = args.merger();
            let source = CommonM3u8ArchiveSource::new(
                client,
                args.m3u8,
                args.key,
                args.range,
                args.shaka_packager,
            )
            .with_retry(args.manifest_retries);
            let downloader =
                ParallelDownloader::new(source, merger, cache, args.threads, args.retries);
            downloader.download().await?;
        }
    };

    Ok(())
}
