use serde::{Deserialize, Serialize};

use crate::{
    common::SegmentType, decrypt::IoriKey, merge::MergableSegmentInfo, RemoteStreamingSegment,
    StreamingSegment,
};
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

impl RemoteStreamingSegment for DashSegment {
    fn url(&self) -> reqwest::Url {
        self.url.clone()
    }

    fn byte_range(&self) -> Option<m3u8_rs::ByteRange> {
        self.byte_range.clone()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DashSegmentInfo {
    pub url: reqwest::Url,
    pub filename: String,
    pub r#type: SegmentType,
    pub sequence: u64,
}

impl MergableSegmentInfo for DashSegmentInfo {
    fn sequence(&self) -> u64 {
        self.sequence
    }

    fn file_name(&self) -> &str {
        self.filename.as_str()
    }

    fn r#type(&self) -> SegmentType {
        self.r#type
    }
}