use reqwest::header::{ACCEPT, ACCEPT_RANGES, CONTENT_LENGTH, RANGE};
use std::sync::Arc;
use tokio::{io::AsyncWrite, sync::mpsc};

use crate::{
    decrypt::IoriKey, HttpClient, IoriResult, SegmentFormat, SegmentType, StreamingSegment,
    StreamingSource,
};

pub struct HttpFileSource {
    url: Arc<String>,
    client: HttpClient,
}

impl HttpFileSource {
    pub fn new(client: HttpClient, url: String, ext: String) -> Self {
        Self {
            url: Arc::new(url),
            client,
        }
    }
}

pub struct HttpRange {
    start: u64,
    end: Option<u64>,
}

impl ToString for HttpRange {
    fn to_string(&self) -> String {
        if let Some(end) = self.end {
            format!("bytes={}-{}", self.start, end)
        } else {
            format!("bytes={}", self.start)
        }
    }
}

pub struct HttpSegment {
    url: Arc<String>,
    filename: String,
    ext: String,

    sequence: u64,
    range: Option<HttpRange>,
}

impl StreamingSegment for HttpSegment {
    fn stream_id(&self) -> u64 {
        0
    }

    fn sequence(&self) -> u64 {
        self.sequence
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

        // detect whether range is supported
        let response = self.client.get(self.url.as_str()).send().await?;
        let content_length = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let accept_ranges = response
            .headers()
            .get(ACCEPT_RANGES)
            .map(|r| r.as_bytes() == b"bytes")
            .unwrap_or(false)
            && content_length > 0;
        drop(response);

        let mut segments = Vec::new();
        if !accept_ranges {
            segments.push(HttpSegment {
                url: self.url.clone(),
                filename: format!("01"),
                ext: "raw".to_string(),
                sequence: 0,
                range: None,
            });
        } else {
            let mut seq = 0;
            let mut now = 0;

            while now < content_length {
                let end = (now + 2 * 1024 * 1024).min(content_length);
                let range = HttpRange {
                    start: now,
                    end: Some(end - 1), // 5MiB per chunk
                };
                now = end;
                segments.push(HttpSegment {
                    url: self.url.clone(),
                    filename: format!(
                        "{}_{}",
                        range.start,
                        range.end.unwrap_or(content_length - 1)
                    ),
                    ext: "raw".to_string(),
                    sequence: seq,
                    range: Some(range),
                });
                seq += 1;
            }
        }

        tx.send(Ok(segments)).unwrap();

        Ok(rx)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: AsyncWrite + Unpin + Send + Sync + 'static,
    {
        use futures::stream::TryStreamExt;

        let mut request = self.client.get(segment.url.as_str()).header(ACCEPT, "*/*");
        if let Some(range) = &segment.range {
            request = request.header(RANGE, range.to_string());
        }

        let response = request.send().await?;
        println!("{:?}", response);

        let stream = response.bytes_stream().map_err(std::io::Error::other);
        let mut reader = tokio_util::io::StreamReader::new(stream);
        tokio::io::copy(&mut reader, writer).await?;

        Ok(())
    }
}
