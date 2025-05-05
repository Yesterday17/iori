use iori::decrypt::IoriKey;
use iori::{InitialSegment, SegmentFormat, SegmentType, StreamingSegment};
use std::sync::Arc;

struct TestSegment {
    stream_id: u64,
    sequence: u64,
    file_name: String,
    initial_segment: InitialSegment,
    key: Option<Arc<IoriKey>>,
    segment_type: SegmentType,
    format: SegmentFormat,
}

impl TestSegment {
    fn new(
        stream_id: u64,
        sequence: u64,
        file_name: String,
        initial_segment: InitialSegment,
        key: Option<Arc<IoriKey>>,
        segment_type: SegmentType,
        format: SegmentFormat,
    ) -> Self {
        Self {
            stream_id,
            sequence,
            file_name,
            initial_segment,
            key,
            segment_type,
            format,
        }
    }
}

impl StreamingSegment for TestSegment {
    fn stream_id(&self) -> u64 {
        self.stream_id
    }

    fn sequence(&self) -> u64 {
        self.sequence
    }

    fn file_name(&self) -> &str {
        &self.file_name
    }

    fn initial_segment(&self) -> InitialSegment {
        self.initial_segment.clone()
    }

    fn key(&self) -> Option<Arc<IoriKey>> {
        self.key.clone()
    }

    fn r#type(&self) -> SegmentType {
        self.segment_type
    }

    fn format(&self) -> SegmentFormat {
        self.format.clone()
    }
}

#[test]
fn test_streaming_segment_implementation() {
    let segment = TestSegment::new(
        1,
        0,
        "test.ts".to_string(),
        InitialSegment::None,
        None,
        SegmentType::Video,
        SegmentFormat::Mpeg2TS,
    );

    assert_eq!(segment.stream_id(), 1);
    assert_eq!(segment.sequence(), 0);
    assert_eq!(segment.file_name(), "test.ts");
    assert!(matches!(segment.initial_segment(), InitialSegment::None));
    assert!(segment.key().is_none());
    assert_eq!(segment.r#type(), SegmentType::Video);
    assert!(matches!(segment.format(), SegmentFormat::Mpeg2TS));
}

#[test]
fn test_streaming_segment_with_initial_segment() {
    let initial_data = vec![1, 2, 3, 4];
    let segment = TestSegment::new(
        1,
        0,
        "test.ts".to_string(),
        InitialSegment::Clear(Arc::new(initial_data.clone())),
        None,
        SegmentType::Video,
        SegmentFormat::Mpeg2TS,
    );

    match segment.initial_segment() {
        InitialSegment::Clear(data) => assert_eq!(&*data, &initial_data),
        _ => panic!("Expected Clear initial segment"),
    }
}

#[test]
fn test_streaming_segment_with_key() {
    let key = Arc::new(IoriKey::Aes128 {
        key: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        iv: [0; 16],
    });
    let segment = TestSegment::new(
        1,
        0,
        "test.ts".to_string(),
        InitialSegment::None,
        Some(key.clone()),
        SegmentType::Video,
        SegmentFormat::Mpeg2TS,
    );

    assert!(segment.key().is_some());
    let segment_key = segment.key().unwrap();
    match &*segment_key {
        IoriKey::Aes128 { key, .. } => {
            assert_eq!(
                key,
                &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
            );
        }
        _ => panic!("Expected Aes128 key"),
    }
}
