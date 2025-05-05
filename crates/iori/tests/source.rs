use futures::executor::block_on;
use iori::{
    InitialSegment, IoriResult, SegmentFormat, SegmentType, StreamingSegment, StreamingSource,
};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

#[derive(Clone)]
struct TestSegment {
    stream_id: u64,
    sequence: u64,
    file_name: String,
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
        InitialSegment::None
    }

    fn key(&self) -> Option<Arc<iori::decrypt::IoriKey>> {
        None
    }

    fn r#type(&self) -> SegmentType {
        SegmentType::Video
    }

    fn format(&self) -> SegmentFormat {
        SegmentFormat::Mpeg2TS
    }
}

#[derive(Clone)]
struct TestSource {
    segments: Vec<TestSegment>,
}

impl TestSource {
    fn new(segments: Vec<TestSegment>) -> Self {
        Self { segments }
    }
}

impl StreamingSource for TestSource {
    type Segment = TestSegment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(Ok(self.segments.clone())).unwrap();
        Ok(rx)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        let data = format!(
            "Segment {} from stream {}",
            segment.sequence(),
            segment.stream_id()
        );
        writer.write_all(data.as_bytes()).await?;
        Ok(())
    }
}

#[test]
fn test_streaming_source_implementation() {
    let segments = vec![
        TestSegment {
            stream_id: 1,
            sequence: 0,
            file_name: "segment0.ts".to_string(),
        },
        TestSegment {
            stream_id: 1,
            sequence: 1,
            file_name: "segment1.ts".to_string(),
        },
    ];

    let source = TestSource::new(segments.clone());
    let mut rx = block_on(source.fetch_info()).unwrap();

    let received_segments: Vec<TestSegment> = block_on(async {
        let mut all_segments = Vec::new();
        while let Some(result) = rx.recv().await {
            all_segments.extend(result.unwrap());
        }
        all_segments
    });

    assert_eq!(received_segments.len(), segments.len());
    for (received, expected) in received_segments.iter().zip(segments.iter()) {
        assert_eq!(received.stream_id(), expected.stream_id());
        assert_eq!(received.sequence(), expected.sequence());
        assert_eq!(received.file_name(), expected.file_name());
    }
}

#[test]
fn test_streaming_source_fetch_segment() {
    let segment = TestSegment {
        stream_id: 1,
        sequence: 0,
        file_name: "segment0.ts".to_string(),
    };

    let source = TestSource::new(vec![segment.clone()]);
    let mut writer = Vec::new();
    block_on(source.fetch_segment(&segment, &mut writer)).unwrap();

    let data = String::from_utf8(writer).unwrap();
    assert_eq!(data, "Segment 0 from stream 1");
}
