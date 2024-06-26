use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::{
    consumer::Consumer,
    error::{IoriError, IoriResult},
    RemoteStreamingSegment, StreamingSegment, ToSegmentData,
};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SegmentType {
    Video,
    Audio,
    Subtitle,
}

impl SegmentType {
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

pub struct CommonSegmentFetcher {
    client: Arc<Client>,
    consumer: Consumer,
}

impl CommonSegmentFetcher {
    pub fn new(client: Arc<Client>, consumer: Consumer) -> Self {
        Self { client, consumer }
    }

    pub async fn fetch<S>(&self, segment: &S, will_retry: bool) -> IoriResult<()>
    where
        S: StreamingSegment + ToSegmentData + Send + Sync + 'static,
    {
        let tmp_file = self.consumer.open_writer(segment).await?;
        let mut tmp_file = match tmp_file {
            Some(f) => f,
            None => return Ok(()),
        };

        let bytes = match segment.to_segment(self.client.clone()).await {
            Ok(b) => b,
            Err(e) => {
                if !will_retry {
                    tmp_file.fail().await?;
                }
                return Err(e);
            }
        };

        // TODO: use bytes_stream to improve performance
        // .bytes_stream();
        let decryptor = segment.key().map(|key| key.to_decryptor());
        if let Some(decryptor) = decryptor {
            let bytes = if let Some(initial_segment) = segment.initial_segment() {
                let mut result = initial_segment.to_vec();
                result.extend_from_slice(&bytes);
                result
            } else {
                bytes.to_vec()
            };
            let bytes = decryptor.decrypt(&bytes)?;
            tmp_file.write_all(&bytes).await?;
        } else {
            if let Some(initial_segment) = segment.initial_segment() {
                tmp_file.write_all(&initial_segment).await?;
            }
            tmp_file.write_all(&bytes).await?;
        }

        tmp_file.finish().await?;
        Ok(())
    }
}

impl<T> ToSegmentData for T
where
    T: RemoteStreamingSegment,
{
    fn to_segment(
        &self,
        client: Arc<Client>,
    ) -> impl std::future::Future<Output = IoriResult<bytes::Bytes>> + Send {
        let url = self.url();
        let byte_range = self.byte_range();
        async move {
            let mut request = client.get(url);
            if let Some(byte_range) = byte_range {
                // offset = 0, length = 1024
                // Range: bytes=0-1023
                //
                // start = offset
                let start = byte_range.offset.unwrap_or(0);
                // end = start + length - 1
                let end = start + byte_range.length - 1;
                request = request.header("Range", format!("bytes={}-{}", start, end));
            }
            let response = request.send().await?;
            if !response.status().is_success() {
                return Err(IoriError::HttpError(response.status()));
            }

            let bytes = response.bytes().await?;
            Ok(bytes)
        }
    }
}
