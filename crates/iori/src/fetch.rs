use reqwest::Client;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{
    error::{IoriError, IoriResult},
    RemoteStreamingSegment, StreamingSegment, ToSegmentData,
};

pub async fn fetch_segment<S, W>(client: Client, segment: &S, tmp_file: &mut W) -> IoriResult<()>
where
    S: StreamingSegment + ToSegmentData,
    W: AsyncWrite + Unpin + Send + Sync + 'static,
{
    let bytes = segment.to_segment_data(client).await?;

    // TODO: use bytes_stream to improve performance
    // .bytes_stream();
    let decryptor = segment.key().map(|key| key.to_decryptor());
    if let Some(decryptor) = decryptor {
        let bytes = if let Some(initial_segment) = segment.initial_segment() {
            let mut result = initial_segment.to_vec();
            result.extend_from_slice(&bytes);
            decryptor.decrypt(&result)?
        } else {
            decryptor.decrypt(&bytes)?
        };
        tmp_file.write_all(&bytes).await?;
    } else {
        if let Some(initial_segment) = segment.initial_segment() {
            tmp_file.write_all(&initial_segment).await?;
        }
        tmp_file.write_all(&bytes).await?;
    }
    tmp_file.flush().await?;

    Ok(())
}

impl<T> ToSegmentData for T
where
    T: RemoteStreamingSegment,
{
    fn to_segment_data(
        &self,
        client: Client,
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
