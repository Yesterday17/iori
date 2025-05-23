use url::Url;

use crate::{ByteRange, IoriError, IoriResult};

pub(crate) fn is_absolute_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("file://")
        || s.starts_with("ftp://")
}

pub(crate) fn merge_baseurls(current: &Url, new: &str) -> IoriResult<Url> {
    if is_absolute_url(new) {
        Ok(Url::parse(new)?)
    } else {
        // We are careful to merge the query portion of the current URL (which is either the
        // original manifest URL, or the URL that it redirected to, or the value of a BaseURL
        // element in the manifest) with the new URL. But if the new URL already has a query string,
        // it takes precedence.
        //
        // Examples
        //
        // merge_baseurls(https://example.com/manifest.mpd?auth=secret, /video42.mp4) =>
        //   https://example.com/video42.mp4?auth=secret
        //
        // merge_baseurls(https://example.com/manifest.mpd?auth=old, /video42.mp4?auth=new) =>
        //   https://example.com/video42.mp4?auth=new
        let mut merged = current.join(new)?;
        if merged.query().is_none() {
            merged.set_query(current.query());
        }
        Ok(merged)
    }
}

/// The byte range shall be expressed and formatted as a byte-range-spec as defined in
/// IETF RFC 7233:2014, subclause 2.1. It is restricted to a single expression identifying
/// a contiguous range of bytes.
pub(crate) fn parse_media_range<S>(s: S) -> IoriResult<ByteRange>
where
    S: AsRef<str>,
{
    let (start, end) = s
        .as_ref()
        .split_once('-')
        .ok_or_else(|| IoriError::MpdParsing("Invalid media range".to_string()))?;

    let first_byte_pos = start
        .parse::<u64>()
        .map_err(|_| IoriError::MpdParsing("Invalid media range".to_string()))?;
    let last_byte_pos = end.parse::<u64>().ok();

    Ok(ByteRange {
        offset: first_byte_pos,
        // 0 - 500 means 501 bytes
        // So length = end - start + 1
        length: last_byte_pos.map(|last_byte_pos| last_byte_pos - first_byte_pos + 1),
    })
}
