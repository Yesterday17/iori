use crate::{
    decrypt::IoriKey, RemoteStreamingSegment, SegmentFormat, SegmentType, StreamingSegment,
};
use std::sync::Arc;

pub struct M3u8Segment {
    pub url: reqwest::Url,
    pub filename: String,

    pub key: Option<Arc<IoriKey>>,
    pub initial_segment: Option<Arc<Vec<u8>>>,

    pub byte_range: Option<m3u8_rs::ByteRange>,

    /// Stream id
    pub stream_id: u64,
    /// Sequence id allocated by the downloader, starts from 0
    pub sequence: u64,
    /// Media sequence id from the m3u8 file
    pub media_sequence: u64,

    pub segment_type: Option<SegmentType>,
    pub duration: f32,
    pub format: SegmentFormat,
}

impl StreamingSegment for M3u8Segment {
    fn stream_id(&self) -> u64 {
        self.stream_id
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
        self.segment_type.unwrap_or(SegmentType::Video)
    }

    fn format(&self) -> SegmentFormat {
        self.format.clone()
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
