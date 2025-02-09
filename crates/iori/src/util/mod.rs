use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use reqwest::Client;

use crate::IoriResult;

pub mod ordered_stream;

pub fn file_name_add_suffix<T: AsRef<OsStr>>(path: &mut PathBuf, suffix: T) {
    let mut filename = OsString::new();

    // {file_stem}_{suffix}.{ext}
    if let Some(file_stem) = path.file_stem() {
        filename.push(file_stem);
    }
    filename.push("_");
    filename.push(suffix);

    if let Some(ext) = path.extension() {
        filename.push(".");
        filename.push(ext);
    }

    path.set_file_name(filename);
}

pub async fn detect_manifest_type(
    url: &str,
    client: Client,
) -> IoriResult<(
    bool,           /* is m3u8 */
    Option<String>, /* initial_playlist_data */
)> {
    // 1. chcek extension
    let url = reqwest::Url::parse(url)?;
    if url.path().to_lowercase().ends_with(".m3u8") {
        return Ok((true, None));
    } else if url.path().to_lowercase().ends_with(".mpd") {
        return Ok((false, None));
    }

    // 2. check content type
    let response = client.get(url).send().await?;
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|s| s.to_str().ok())
        .map(|r| r.to_lowercase());
    let initial_playlist_data = response.text().await.ok();
    match content_type.as_deref() {
        Some("application/x-mpegurl" | "application/vnd.apple.mpegurl") => {
            return Ok((true, initial_playlist_data))
        }
        Some("application/dash+xml") => return Ok((false, initial_playlist_data)),
        _ => {}
    }

    // 3. check by parsing
    if let Some(initial_playlist_data) = initial_playlist_data {
        let is_valid_m3u8 = m3u8_rs::parse_playlist_res(initial_playlist_data.as_bytes()).is_ok();
        if is_valid_m3u8 {
            return Ok((is_valid_m3u8, Some(initial_playlist_data)));
        }
    }

    Ok((false, None))
}
