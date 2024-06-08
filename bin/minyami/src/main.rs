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
    consumer::Consumer,
    dash::archive::CommonDashArchiveSource,
    download::ParallelDownloader,
    hls::{CommonM3u8ArchiveSource, CommonM3u8LiveSource, SegmentRange},
    merge::{merge, MergableSegmentInfo},
};
use iori_nicolive::source::NicoTimeshiftSource;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, ClientBuilder,
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

    /// [Unimplemented]
    /// Output file path
    #[clap(short, long, default_value = "./output.mkv")]
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
    /// Specify segment range to download in archive mode
    #[clap(long, default_value = "-")]
    range: SegmentRange,

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

        ClientBuilder::new()
            .default_headers(headers)
            .user_agent(get_chrome_rua())
            // TODO: verify whether this is the correct timeout for both live and archive
            .timeout(Duration::from_secs(60))
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args = MinyamiArgs::parse();
    let client = args.client();

    let output_dir = if let Some(dir) = args.resume_dir {
        dir
    } else {
        let temp_path = args.temp_dir()?;
        let started_at = SystemTime::now();
        let started_at = started_at.duration_since(UNIX_EPOCH).unwrap().as_millis();
        temp_path.join(format!("minyami_{started_at}"))
    };

    let segments: Option<Vec<_>> = if args.live {
        // Live Downloader
        let consumer = if args.pipe {
            Consumer::pipe(&output_dir, args.keep)?
        } else {
            Consumer::file(&output_dir)?
        };
        let source =
            CommonM3u8LiveSource::new(client, args.m3u8, args.key, consumer, args.shaka_packager);
        let mut downloader = ParallelDownloader::new(source, args.threads, args.retries);
        let segments = downloader.download().await?;
        Some(
            segments
                .into_iter()
                .map(|r| Box::new(r) as Box<dyn MergableSegmentInfo>)
                .collect(),
        )
    } else {
        // Archive Downloader
        let consumer = Consumer::file(&output_dir)?;

        if args.m3u8.contains("dmc.nico") {
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

            let source = NicoTimeshiftSource::new(client, wss_url, consumer).await?;
            let mut downloader = ParallelDownloader::new(source, args.threads, args.retries);
            downloader.download().await?;

            None
        } else if args.dash {
            let source = CommonDashArchiveSource::new(client, args.m3u8, args.key, consumer)?;
            let mut downloader = ParallelDownloader::new(source, args.threads, args.retries);
            let segments = downloader.download().await?;

            Some(
                segments
                    .into_iter()
                    .map(|r| Box::new(r) as Box<dyn MergableSegmentInfo>)
                    .collect(),
            )
        } else {
            let source = CommonM3u8ArchiveSource::new(
                client,
                args.m3u8,
                args.key,
                args.range,
                consumer,
                args.shaka_packager,
            );
            let mut downloader = ParallelDownloader::new(source, args.threads, args.retries);
            let segments = downloader.download().await?;
            Some(
                segments
                    .into_iter()
                    .map(|r| Box::new(r) as Box<dyn MergableSegmentInfo>)
                    .collect(),
            )
        }
    };

    let Some(segments) = segments else {
        log::warn!("Segments are not mergable. Skipping merge step.");
        return Ok(());
    };

    if args.no_merge {
        log::info!("Skip merging. Please merge video chunks manually.");
        log::info!("Temporary files are located at {}", output_dir.display());
        return Ok(());
    }

    log::info!("Merging chunks...");
    let target_file = current_dir()?.join(args.output);
    merge(segments, &output_dir, &target_file).await?;

    if !args.keep {
        log::info!("End of merging.");
        log::info!("Starting cleaning temporary files.");
        tokio::fs::remove_dir_all(&output_dir).await?;
    }

    log::info!(
        "All finished. Please checkout your files at {}",
        target_file.display()
    );
    Ok(())
}
