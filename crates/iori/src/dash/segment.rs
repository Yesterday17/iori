use crate::{decrypt::IoriKey, RemoteStreamingSegment, SegmentType, StreamingSegment};
use std::sync::Arc;

pub struct DashSegment {
    pub url: reqwest::Url,
    pub filename: String,

    pub key: Option<Arc<IoriKey>>,
    pub initial_segment: Option<Arc<Vec<u8>>>,

    pub byte_range: Option<m3u8_rs::ByteRange>,

    /// Sequence id allocated by the downloader, starts from 0
    pub sequence: u64,

    pub r#type: SegmentType,
}

impl StreamingSegment for DashSegment {
    fn stream(&self) -> u64 {
        0
    }

    fn sequence(&self) -> u64 {
        self.sequence
    }

    fn file_name(&self) -> &str {
        self.filename.as_str()
    }

    fn initial_segment(&self) -> Option<Arc<Vec<u8>>> {
        self.initial_segment.clone()
    }

    fn key(&self) -> Option<Arc<IoriKey>> {
        self.key.clone()
    }

    fn r#type(&self) -> SegmentType {
        self.r#type
    }
}

impl RemoteStreamingSegment for DashSegment {
    fn url(&self) -> reqwest::Url {
        self.url.clone()
    }

    fn byte_range(&self) -> Option<m3u8_rs::ByteRange> {
        self.byte_range.clone()
    }
}
