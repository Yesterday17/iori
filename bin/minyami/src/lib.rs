mod types;

use std::{
    env::current_dir,
    ffi::OsString,
    num::NonZeroU32,
    path::PathBuf,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::bail;
use fake_user_agent::get_chrome_rua;
use iori::{
    cache::{file::FileCacheSource, memory::MemoryCacheSource},
    dash::archive::CommonDashArchiveSource,
    download::ParallelDownloader,
    hls::{CommonM3u8ArchiveSource, CommonM3u8LiveSource, SegmentRange},
    merge::IoriMerger,
    StreamingSource,
};
use iori_nicolive::source::NicoTimeshiftSource;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client,
};
use types::MinyamiCache;

#[derive(clap::Parser, Debug, Clone)]
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

    fn final_temp_dir(&self) -> anyhow::Result<PathBuf> {
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
        if self.live && self.pipe && self.output.is_none() {
            IoriMerger::pipe(!self.keep)
        } else if self.no_merge {
            IoriMerger::skip()
        } else {
            let target_file = self.final_output_file();
            if self.pipe {
                IoriMerger::pipe_to_file(!self.keep, target_file)
            } else {
                IoriMerger::concat(target_file, self.keep)
            }
        }
    }

    async fn download<S>(&self, source: S, cache: MinyamiCache) -> anyhow::Result<()>
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

        let cache: MinyamiCache = match (self.live, self.pipe) {
            (_, true) => MinyamiCache::Memory(MemoryCacheSource::new()),
            (true, false) => MinyamiCache::File(FileCacheSource::new(final_temp_dir.clone())),
            _ => MinyamiCache::File(FileCacheSource::new(final_temp_dir.clone())),
        };

        if self.live {
            let source = CommonM3u8LiveSource::new(
                client,
                self.m3u8.clone(),
                self.key.as_deref(),
                self.shaka_packager.clone(),
            )
            .with_retry(self.manifest_retries);
            self.download(source, cache).await?;
        } else if self.m3u8.contains("dmc.nico") {
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
        } else {
            // Archive Downloader
            if self.dash {
                let source =
                    CommonDashArchiveSource::new(client, self.m3u8.clone(), self.key.as_deref())?;
                self.download(source, cache).await?;
            } else {
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
        };

        Ok(())
    }
}
