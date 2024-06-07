use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use reqwest::Client;
use tokio::sync::mpsc;
use url::Url;

use super::{
    // core::{HlsSegmentFetcher, M3u8Source},
    DashSegment,
};
use crate::{common::CommonSegmentFetcher, consumer::Consumer, error::IoriResult, StreamingSource};
use once_cell::sync::Lazy;
use regex::Regex;

pub struct CommonDashArchiveSource {
    client: Arc<Client>,
    mpd: Url,
    key: Option<String>,
    sequence: AtomicU64,
    fetch: CommonSegmentFetcher,
}

impl CommonDashArchiveSource {
    pub fn new(
        client: Client,
        mpd: String,
        key: Option<String>,
        consumer: Consumer,
    ) -> IoriResult<Self> {
        let client = Arc::new(client);
        let fetch = CommonSegmentFetcher::new(client.clone(), consumer);
        Ok(Self {
            client,
            mpd: Url::parse(&mpd)?,
            key,
            sequence: AtomicU64::new(0),
            fetch,
        })
    }
}

impl StreamingSource for CommonDashArchiveSource {
    type Segment = DashSegment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let text = self
            .client
            .get(self.mpd.as_ref())
            .header("Accept", "application/dash+xml,video/vnd.mpeg.dash.mpd")
            .send()
            .await
            .expect("requesting MPD content")
            .text()
            .await
            .expect("fetching MPD content");
        let mpd = dash_mpd::parse(&text)?;

        // let Some("static") = mpd.mpdtype.map(|r| r.as_str()) else {
        //     panic!("only static MPD is supported");
        // };

        let mut base_url = self.mpd.clone();
        if let Some(mpd_base_url) = mpd.base_url.get(0) {
            base_url = merge_baseurls(&base_url, &mpd_base_url.base)?;
        }

        for period in mpd.periods {
            let base_url = if let Some(mpd_base_url) = period.BaseURL.get(0) {
                Cow::Owned(merge_baseurls(&base_url, &mpd_base_url.base)?)
            } else {
                Cow::Borrowed(&base_url)
            };

            for adaptation in period.adaptations {
                let base_url = if let Some(mpd_base_url) = adaptation.BaseURL.get(0) {
                    Cow::Owned(merge_baseurls(&base_url, &mpd_base_url.base)?)
                } else {
                    base_url.clone()
                };

                let mime_type = adaptation.contentType.or_else(|| adaptation.mimeType);
                let frame_rate = adaptation.frameRate; // TODO: GetFrameRate

                for representation in adaptation.representations {
                    let base_url = if let Some(mpd_base_url) = representation.BaseURL.get(0) {
                        Cow::Owned(merge_baseurls(&base_url, &mpd_base_url.base)?)
                    } else {
                        base_url.clone()
                    };

                    let mime_type = mime_type
                        .clone() // TODO: do not clone here
                        .or_else(|| representation.contentType)
                        .or_else(|| representation.mimeType);

                    let bandwidth = representation.bandwidth.unwrap_or(0);
                    let codecs = representation
                        .codecs
                        .as_deref()
                        .or_else(|| adaptation.codecs.as_deref());
                    let language = representation
                        .lang
                        .as_deref()
                        .or_else(|| adaptation.lang.as_deref());
                    let frame_rate = frame_rate
                        .as_deref()
                        .or_else(|| representation.frameRate.as_deref());
                    let resolution = representation
                        .width
                        .and_then(|w| representation.height.map(|h| (w, h)))
                        .map(|(w, h)| format!("{w}x{h}"));

                    let mut params = HashMap::new();
                    if let Some(representation_id) = representation.id.clone() {
                        params.insert("RepresentationID", representation_id);
                    }
                    params.insert("Bandwidth", bandwidth.to_string());

                    // 1. TODO: SegmentBase
                    if let Some(segment_base) = representation.SegmentBase {
                        if let Some(initialization) = segment_base.initialization {
                            if let Some(source_url) = initialization.sourceURL {
                                // let url = base_url;
                                let init_url = base_url.join(&source_url)?;
                                let init_range = initialization.range.as_deref();

                                // TODO: set init
                            } else {
                                //
                            }
                        }
                    }

                    let inner_segment_template = representation.SegmentTemplate.as_ref();
                    let outer_segment_template = adaptation.SegmentTemplate.as_ref();

                    if let Some(segment_template) =
                        inner_segment_template.or(outer_segment_template)
                    {
                        let time_scale = segment_template.timescale.unwrap_or(1);
                        let initial_segment = if let Some(ref initialization) =
                            segment_template.initialization
                        {
                            let initialization = resolve_url_template(&initialization, &params);
                            let url = merge_baseurls(&base_url, &initialization)?;
                            let bytes = self.client.get(url).send().await?.bytes().await?.to_vec();
                            Some(Arc::new(bytes))
                            // todo!("fetch initialization segment");
                        } else {
                            None
                        };

                        if let Some(ref media_template) = segment_template.media {
                            let mut current_time = 0;
                            let mut segment_number = 1;
                            if let Some(ref segment_timeline) = segment_template.SegmentTimeline {
                                for segment in segment_timeline.segments.iter() {
                                    if let Some(t) = segment.t {
                                        current_time = t;
                                    }

                                    let duration = segment.d;
                                    let repeat = segment.r.unwrap_or(0);
                                    for _ in 0..(repeat + 1) {
                                        params.insert("Time", current_time.to_string());
                                        params.insert("Number", segment_number.to_string());
                                        let filename =
                                            resolve_url_template(&media_template, &params);
                                        let url = merge_baseurls(&base_url, &filename)?;

                                        let segment = DashSegment {
                                            url,
                                            filename: filename.replace("/", "__"),
                                            initial_segment: initial_segment.clone(),
                                            byte_range: None,
                                            sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
                                        };
                                        sender.send(Ok(vec![segment])).unwrap();

                                        segment_number += 1;
                                        current_time += duration;
                                    }
                                }
                            }
                        } else {
                            todo!()
                        }
                    }

                    // segment.url =
                    //     merge_baseurls(&base_url, &resolve_url_template(&segment.url, &params))?;
                }
            }
        }

        Ok(receiver)
    }

    async fn fetch_segment(&self, segment: &Self::Segment, will_retry: bool) -> IoriResult<()> {
        self.fetch.fetch(segment, will_retry).await
    }
}

fn is_absolute_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("file://")
        || s.starts_with("ftp://")
}

fn merge_baseurls(current: &Url, new: &str) -> IoriResult<Url> {
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

// From https://dashif.org/docs/DASH-IF-IOP-v4.3.pdf:
// "For the avoidance of doubt, only %0[width]d is permitted and no other identifiers. The reason
// is that such a string replacement can be easily implemented without requiring a specific library."
//
// Instead of pulling in C printf() or a reimplementation such as the printf_compat crate, we reimplement
// this functionality directly.
//
// Example template: "$RepresentationID$/$Number%06d$.m4s"
static URL_TEMPLATE_IDS: Lazy<Vec<(&'static str, String, Regex)>> = Lazy::new(|| {
    vec!["RepresentationID", "Number", "Time", "Bandwidth"]
        .into_iter()
        .map(|k| {
            (
                k,
                format!("${k}$"),
                Regex::new(&format!("\\${k}%0([\\d])d\\$")).unwrap(),
            )
        })
        .collect()
});

fn resolve_url_template(template: &str, params: &HashMap<&str, String>) -> String {
    let mut result = template.to_string();
    for (k, ident, rx) in URL_TEMPLATE_IDS.iter() {
        // first check for simple cases such as $Number$
        if result.contains(ident) {
            if let Some(value) = params.get(k as &str) {
                result = result.replace(ident, value);
            }
        }
        // now check for complex cases such as $Number%06d$
        if let Some(cap) = rx.captures(&result) {
            if let Some(value) = params.get(k as &str) {
                let width: usize = cap[1].parse::<usize>().unwrap();
                let count = format!("{value:0>width$}");
                let m = rx.find(&result).unwrap();
                result = result[..m.start()].to_owned() + &count + &result[m.end()..];
            }
        }
    }
    result
}
