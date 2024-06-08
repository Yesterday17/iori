use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    decrypt::IoriKey, merge::MergableSegmentInfo, RemoteStreamingSegment, StreamingSegment,
};

pub struct M3u8Segment {
    pub url: reqwest::Url,
    pub filename: String,

    pub key: Option<Arc<IoriKey>>,
    pub initial_segment: Option<Arc<Vec<u8>>>,

    pub byte_range: Option<m3u8_rs::ByteRange>,

    /// Sequence id allocated by the downloader, starts from 0
    pub sequence: u64,
    /// Media sequence id from the m3u8 file
    pub media_sequence: u64,
}

impl StreamingSegment for M3u8Segment {
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
}

impl RemoteStreamingSegment for M3u8Segment {
    fn url(&self) -> reqwest::Url {
        self.url.clone()
    }

    fn byte_range(&self) -> Option<m3u8_rs::ByteRange> {
        self.byte_range.clone()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct M3u8SegmentInfo {
    pub url: reqwest::Url,
    pub filename: String,
    pub sequence: u64,
}

impl MergableSegmentInfo for M3u8SegmentInfo {
    fn sequence(&self) -> u64 {
        self.sequence
    }

    fn file_name(&self) -> &str {
        self.filename.as_str()
    }

    fn r#type(&self) -> crate::common::SegmentType {
        crate::common::SegmentType::Video
    }
}
