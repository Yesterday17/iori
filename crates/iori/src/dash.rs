pub mod archive;

use serde::{Deserialize, Serialize};

use crate::{decrypt::IoriKey, RemoteStreamingSegment, StreamingSegment};
use std::sync::Arc;

pub struct DashSegment {
    pub url: reqwest::Url,
    pub filename: String,

    pub key: Option<Arc<IoriKey>>,
    pub initial_segment: Option<Arc<Vec<u8>>>,

    pub byte_range: Option<m3u8_rs::ByteRange>,

    /// Sequence id allocated by the downloader, starts from 0
    pub sequence: u64,

    pub r#type: DashSegmentType,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DashSegmentType {
    Video,
    Audio,
    Subtitle,
}

impl DashSegmentType {
    pub fn from_mime_type(mime_type: Option<&str>) -> Self {
        let mime_type = mime_type.unwrap_or("video");

        if mime_type.starts_with("video") {
            return Self::Video;
        } else if mime_type.starts_with("audio") {
            return Self::Audio;
        } else if mime_type.starts_with("text") {
            return Self::Subtitle;
        } else {
            panic!("Unknown mime type: {}", mime_type);
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DashSegmentInfo {
    pub url: reqwest::Url,
    pub filename: String,
    pub r#type: DashSegmentType,
    pub sequence: u64,
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
