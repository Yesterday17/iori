use std::sync::Arc;
use tokio::{io::AsyncWrite, sync::mpsc};

use crate::{
    decrypt::IoriKey, HttpClient, IoriResult, SegmentFormat, SegmentType, StreamingSegment,
    StreamingSource,
};

pub struct HttpFileSource {
    url: String,
    ext: String,
    client: HttpClient,
}

impl HttpFileSource {
    pub fn new(client: HttpClient, url: String, ext: String) -> Self {
        Self { url, ext, client }
    }
}

pub struct HttpSegment {
    url: String,
    filename: String,
    ext: String,
}

impl HttpSegment {
    fn new(url: String, ext: String) -> Self {
        Self {
            url,
            filename: format!("01.{ext}"),
            ext,
        }
    }
}

impl StreamingSegment for HttpSegment {
    fn stream_id(&self) -> u64 {
        0
    }

    fn sequence(&self) -> u64 {
        0
    }

    fn file_name(&self) -> &str {
        &self.filename
    }

    fn key(&self) -> Option<Arc<IoriKey>> {
        None
    }

    fn r#type(&self) -> SegmentType {
        SegmentType::Video
    }

    fn format(&self) -> SegmentFormat {
        SegmentFormat::Raw(self.ext.clone())
    }
}

impl StreamingSource for HttpFileSource {
    type Segment = HttpSegment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(Ok(vec![HttpSegment::new(
            self.url.clone(),
            self.ext.clone(),
        )]))
        .unwrap();
        Ok(rx)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        use futures::stream::TryStreamExt;

        let response = self.client.get(segment.url.clone()).send().await?;
        let stream = response.bytes_stream().map_err(std::io::Error::other);
        let mut reader = tokio_util::io::StreamReader::new(stream);
        tokio::io::copy(&mut reader, writer).await?;

        Ok(())
    }
}
