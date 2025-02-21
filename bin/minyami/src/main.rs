use std::{
    env::current_dir,
    ffi::OsString,
    num::NonZeroU32,
    path::PathBuf,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::bail;
use clap::Parser;
use fake_user_agent::get_chrome_rua;
use iori::{
    cache::IoriCache,
    dash::archive::CommonDashArchiveSource,
    download::ParallelDownloader,
    hls::{CommonM3u8ArchiveSource, CommonM3u8LiveSource, SegmentRange},
    merge::IoriMerger,
    StreamingSource,
};
use iori_nicolive::source::NicoTimeshiftSource;
use pretty_env_logger::env_logger::Builder;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, Proxy,
};

#[derive(clap::Parser, Debug, Clone)]
#[clap(version = env!("IORI_MINYAMI_VERSION"), author)]
pub struct MinyamiArgs {
    #[clap(short, long, hide = true)]
    pub download: bool,

    #[clap(long, hide = true)]
    pub shaka_packager: Option<PathBuf>,

    /// Debug output
    #[clap(long, alias = "debug")]
    pub verbose: bool,

    /// Threads limit
    #[clap(long, default_value = "5")]
    pub threads: NonZeroU32,

    /// Retry limit
    #[clap(long, default_value = "5")]
    pub retries: u32,

    #[clap(long, default_value = "3")]
    pub manifest_retries: u32,

    /// Output file path
    ///
    /// Default: output.ts
    #[clap(short, long)]
    pub output: Option<PathBuf>,

    /// Temporary file path
    #[clap(long, env = "TEMP")]
    pub temp_dir: Option<PathBuf>,

    /// Set key manually (Internal use)
    ///
    /// (Optional) Key for decrypt video.
    #[clap(long)]
    pub key: Option<String>,

    /// Cookies used to download
    #[clap(long)]
    pub cookies: Option<String>,

    /// HTTP Header used to download
    ///
    /// Custom header. eg. "User-Agent: xxxxx". This option will override --cookies.
    #[clap(short = 'H', long)]
    pub headers: Vec<String>,

    /// Download live
    #[clap(long)]
    pub live: bool,

    /// [Unimplemented]
    /// (Optional) Set output format. default: ts
    /// Format name. ts or mkv.
    #[clap(long)]
    pub format: Option<String>,

    /// [Unimplemented]
    /// Use the specified HTTP/HTTPS/SOCKS5 proxy
    ///
    /// Set proxy in [protocol://<host>:<port>] format. eg. --proxy "http://127.0.0.1:1080".
    #[clap(long)]
    pub proxy: Option<String>,

    /// [Unimplemented]
    /// Download specified part of the stream
    ///
    /// Set time range in [<hh:mm:ss>-<hh:mm:ss> format]. eg. --slice "45:00-53:00"
    #[clap(long)]
    pub slice: Option<String>,

    /// Do not merge m3u8 chunks.
    #[clap(long)]
    pub no_merge: bool,

    /// Keep temporary files.
    #[clap(short, long)]
    pub keep: bool,

    /// [Unimplemented]
    /// Do not delete encrypted chunks after decryption.
    #[clap(long)]
    pub keep_encrypted_chunks: bool,

    /// [Unimplemented]
    /// Temporary file naming strategy. Defaults to 1.
    ///
    /// MIXED = 0,
    /// USE_FILE_SEQUENCE = 1,
    /// USE_FILE_PATH = 2,
    #[clap(long, default_value = "1")]
    pub chunk_naming_strategy: u8,

    /// [Iori Argument]
    /// Specify segment range to download in archive mode.
    #[clap(long, default_value = "-")]
    pub range: SegmentRange,

    /// [Iori Argument]
    /// Timeout seconds for each manifest/segment request.
    /// Defaults to 10 seconds.
    #[clap(long, default_value = "10")]
    pub timeout: u64,

    /// [Iori Argument]
    /// Specify the resume folder path
    #[clap(long)]
    pub resume_dir: Option<PathBuf>,

    /// [Iori Argument]
    /// If set, the program will try to work in a pipe mode.
    ///
    /// The pipe mode will pipe the downloaded segments to a specified file or stdout,
    /// depending on whether the --output option is explicitly set.
    #[clap(long)]
    pub pipe: bool,

    /// [Iori Argument]
    /// Download with dash format
    #[clap(long)]
    pub dash: bool,

    /// m3u8 file path
    pub m3u8: String,
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

        let mut client = Client::builder()
            .default_headers(headers)
            .user_agent(get_chrome_rua())
            .timeout(Duration::from_secs(self.timeout));
        if let Some(proxy) = &self.proxy {
            client = client.proxy(Proxy::all(proxy).expect("Invalid proxy"));
        }

        client.build().unwrap()
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

    fn final_temp_dir(&self) -> anyhow::Result<PathBuf> {
        let output_dir = if let Some(ref dir) = self.resume_dir {
            dir.clone()
        } else {
            let temp_path = self.temp_dir()?;
            let started_at = SystemTime::now();
            let started_at = started_at.duration_since(UNIX_EPOCH).unwrap().as_millis();
            temp_path.join(format!("minyami_{started_at}"))
        };
        Ok(output_dir)
    }

    fn final_output_file(&self) -> PathBuf {
        let current_dir = current_dir().unwrap();
        let mut output_file = current_dir.join(self.output.clone().unwrap_or("output.ts".into()));

        while output_file.exists() {
            let mut filename = OsString::new();

            // {file_stem}_{timestamp}.{ext}
            if let Some(file_stem) = output_file.file_stem() {
                filename.push(file_stem);
            }
            filename.push("_");
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();
            filename.push(now.to_string());

            if let Some(ext) = output_file.extension() {
                filename.push(".");
                filename.push(ext);
            }

            output_file.set_file_name(filename);
        }

        output_file
    }

    fn merger(&self) -> IoriMerger {
        if self.dash {
            let target_file = self.final_output_file();
            if self.pipe {
                IoriMerger::pipe_mux(
                    !self.keep,
                    target_file,
                    std::env::var("RE_LIVE_PIPE_OPTIONS").ok(),
                )
            } else {
                IoriMerger::mkvmerge(target_file, self.keep)
            }
        } else if self.pipe && self.output.is_none() {
            IoriMerger::pipe(!self.keep)
        } else if self.no_merge {
            IoriMerger::skip()
        } else {
            let target_file = self.final_output_file();
            if self.pipe {
                IoriMerger::pipe_to_file(!self.keep, target_file)
            } else {
                IoriMerger::mkvmerge(target_file, self.keep)
            }
        }
    }

    async fn download<S>(&self, source: S, cache: IoriCache) -> anyhow::Result<()>
    where
        S: StreamingSource + Send + Sync + 'static,
    {
        let downloader =
            ParallelDownloader::new(source, self.merger(), cache, self.threads, self.retries);
        downloader.download().await?;
        Ok(())
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let client = self.client();
        let final_temp_dir = self.final_temp_dir()?;

        let cache: IoriCache = match (self.live, self.pipe, self.dash) {
            (_, true, false) => IoriCache::memory(),
            (true, false, _) => IoriCache::file(final_temp_dir),
            _ => IoriCache::file(final_temp_dir),
        };

        if self.m3u8.contains("dmc.nico") {
            log::info!("Enhanced mode for Nico-TS enabled");

            let key = self.key.as_deref().expect("Key is required for Nico-TS");
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
                .with_retry(self.manifest_retries);
            self.download(source, cache).await?;
            return Ok(());
        }

        match (self.dash, self.live) {
            // DASH Live
            (true, true) => unimplemented!(),
            // DASH Archive
            (true, false) => {
                let source =
                    CommonDashArchiveSource::new(client, self.m3u8.clone(), self.key.as_deref())?;
                self.download(source, cache).await?;
            }
            // HLS Live
            (false, true) => {
                let source = CommonM3u8LiveSource::new(
                    client,
                    self.m3u8.clone(),
                    self.key.as_deref(),
                    self.shaka_packager.clone(),
                )
                .with_retry(self.manifest_retries);
                self.download(source, cache).await?;
            }
            // HLS Archive
            (false, false) => {
                let source = CommonM3u8ArchiveSource::new(
                    client,
                    self.m3u8.clone(),
                    self.key.as_deref(),
                    self.range,
                    self.shaka_packager.clone(),
                )
                .with_retry(self.manifest_retries);
                self.download(source, cache).await?;
            }
        }

        Ok(())
    }
}

/// Logger modified from pretty-env-logger
///
/// Copyright (c) 2017 Sean McArthur
///
/// Permission is hereby granted, free of charge, to any person obtaining a copy
/// of this software and associated documentation files (the "Software"), to deal
/// in the Software without restriction, including without limitation the rights
/// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
/// copies of the Software, and to permit persons to whom the Software is
/// furnished to do so, subject to the following conditions:
///
/// The above copyright notice and this permission notice shall be included in all
/// copies or substantial portions of the Software.
///
/// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
/// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
/// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
/// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
/// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
/// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
/// SOFTWARE.
pub(crate) fn logger() -> Builder {
    use std::{
        fmt,
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct Padded<T> {
        value: T,
        width: usize,
    }

    impl<T: fmt::Display> fmt::Display for Padded<T> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{: <width$}", self.value, width = self.width)
        }
    }

    static MAX_MODULE_WIDTH: AtomicUsize = AtomicUsize::new(0);

    fn max_target_width(target: &str) -> usize {
        let max_width = MAX_MODULE_WIDTH.load(Ordering::Relaxed);
        if max_width < target.len() {
            MAX_MODULE_WIDTH.store(target.len(), Ordering::Relaxed);
            target.len()
        } else {
            max_width
        }
    }

    let instance_id = std::env::var("INSTANCE_ID")
        .map(|i| format!("{i} "))
        .unwrap_or_default();
    let mut builder = Builder::new();

    builder
        .format(move |f, record| {
            use pretty_env_logger::env_logger::fmt::Color;
            use std::io::Write;

            let target = record.target();
            let max_width = max_target_width(target);

            let mut style = f.style();
            let color = match record.level() {
                log::Level::Trace => Color::Magenta,
                log::Level::Debug => Color::Blue,
                log::Level::Info => Color::Green,
                log::Level::Warn => Color::Yellow,
                log::Level::Error => Color::Red,
            };
            let level = style.set_color(color).value(record.level());

            let mut style = f.style();
            let target = style.set_bold(true).value(Padded {
                value: target,
                width: max_width,
            });

            writeln!(f, " {level} {instance_id}{target} > {}", record.args())
        })
        .filter_level(log::LevelFilter::Info)
        .parse_default_env();

    builder
}

#[tokio::main(worker_threads = 8)]
async fn main() -> anyhow::Result<()> {
    logger().init();

    MinyamiArgs::parse().run().await?;

    Ok(())
}
