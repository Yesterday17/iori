use url::Url;

use crate::IoriResult;

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
