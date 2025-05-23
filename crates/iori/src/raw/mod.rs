pub use bytes::Bytes;
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    sync::mpsc,
};

use crate::{IoriResult, StreamingSegment, StreamingSource};

mod http;
pub use http::*;

pub struct RawDataSource {
    data: Bytes,
    ext: String,
}

impl RawDataSource {
    pub fn new(data: Bytes, ext: String) -> Self {
        Self { data, ext }
    }
}

pub struct RawSegment {
    data: Bytes,

    filename: String,
    ext: String,
}

impl RawSegment {
    pub fn new(data: Bytes, ext: String) -> Self {
        Self {
            data,
            filename: format!("01.{ext}"),
            ext,
        }
    }

    pub fn data(&self) -> &[u8] {
        self.data.as_ref()
    }
}

impl StreamingSegment for RawSegment {
    fn stream_id(&self) -> u64 {
        0
    }

    fn sequence(&self) -> u64 {
        0
    }

    fn file_name(&self) -> &str {
        &self.filename
    }

    fn key(&self) -> Option<std::sync::Arc<crate::decrypt::IoriKey>> {
        None
    }

    fn r#type(&self) -> crate::SegmentType {
        crate::SegmentType::Subtitle
    }

    fn format(&self) -> crate::SegmentFormat {
        crate::SegmentFormat::Raw(self.ext.clone())
    }
}

impl StreamingSource for RawDataSource {
    type Segment = RawSegment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(Ok(vec![RawSegment::new(
            self.data.clone(),
            self.ext.clone(),
        )]))
        .unwrap();
        Ok(rx)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        writer.write_all(segment.data()).await?;
        Ok(())
    }
}
