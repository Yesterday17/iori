use std::{
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
    downloader::ParallelDownloader,
    hls::{CommonM3u8ArchiveSource, CommonM3u8LiveSource, SegmentRange},
};
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

    /// [Unimplemented]
    /// Do not merge m3u8 chunks.
    #[clap(long)]
    no_merge: bool,

    /// [Unimplemented]
    /// Keep temporary files.
    ///
    /// Only takes effect in live mode with --pipe argument.
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
    let args = MinyamiArgs::parse();
    let client = args.client();

    let output_dir = if let Some(dir) = args.resume_dir {
        dir
    } else {
        let temp_path = args.temp_dir()?;
        let started_at = SystemTime::now();
        let started_at = started_at.duration_since(UNIX_EPOCH).unwrap().as_millis();
        temp_path.join(format!("minyami_{}", started_at))
    };

    if args.live {
        // Live Downloader
        let consumer = if args.pipe {
            Consumer::pipe(output_dir, args.keep)?
        } else {
            Consumer::file(output_dir)?
        };
        let source =
            CommonM3u8LiveSource::new(client, args.m3u8, args.key, consumer, args.shaka_packager);
        let mut downloader = ParallelDownloader::new(source, args.threads, args.retries);
        downloader.download().await?;
    } else {
        // Archive Downloader
        let consumer = Consumer::file(output_dir)?;
        let source = CommonM3u8ArchiveSource::new(
            client,
            args.m3u8,
            args.key,
            args.range,
            consumer,
            args.shaka_packager,
        );
        let mut downloader = ParallelDownloader::new(source, args.threads, args.retries);
        downloader.download().await?;
    }

    Ok(())
}
