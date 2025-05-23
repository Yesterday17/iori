use crate::{
    decrypt::IoriKey, ByteRange, InitialSegment, RemoteStreamingSegment, SegmentFormat,
    SegmentType, StreamingSegment,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct DashSegment {
    pub url: reqwest::Url,
    pub filename: String,

    pub key: Option<Arc<IoriKey>>,
    pub initial_segment: InitialSegment,

    pub byte_range: Option<ByteRange>,

    pub sequence: u64,
    pub stream_id: u64,

    pub r#type: SegmentType,

    /// $Time$
    pub time: Option<u64>,
}

impl StreamingSegment for DashSegment {
    fn stream_id(&self) -> u64 {
        self.stream_id
    }

    fn sequence(&self) -> u64 {
        self.time.unwrap_or(self.sequence)
    }

    fn file_name(&self) -> &str {
        self.filename.as_str()
    }

    fn initial_segment(&self) -> InitialSegment {
        self.initial_segment.clone()
    }

    fn key(&self) -> Option<Arc<IoriKey>> {
        self.key.clone()
    }

    fn r#type(&self) -> SegmentType {
        self.r#type
    }

    fn format(&self) -> SegmentFormat {
        SegmentFormat::Mp4
    }
}

impl RemoteStreamingSegment for DashSegment {
    fn url(&self) -> reqwest::Url {
        self.url.clone()
    }

    fn byte_range(&self) -> Option<ByteRange> {
        self.byte_range.clone()
    }
}
