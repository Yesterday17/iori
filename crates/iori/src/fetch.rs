use std::path::PathBuf;

use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{
    error::{IoriError, IoriResult},
    util::http::HttpClient,
    InitialSegment, RemoteStreamingSegment, StreamingSegment, ToSegmentData,
};

pub async fn fetch_segment<S, W>(
    client: HttpClient,
    segment: &S,
    tmp_file: &mut W,
    shaka_packager_command: Option<PathBuf>,
) -> IoriResult<()>
where
    S: StreamingSegment + ToSegmentData,
    W: AsyncWrite + Unpin + Send + Sync + 'static,
{
    let bytes = segment.to_segment_data(client).await?;

    // TODO: use bytes_stream to improve performance
    // .bytes_stream();
    let decryptor = segment
        .key()
        .map(|key| key.to_decryptor(shaka_packager_command));
    if let Some(decryptor) = decryptor {
        let bytes = match segment.initial_segment() {
            crate::InitialSegment::Encrypted(data) => {
                let mut result = data.to_vec();
                result.extend_from_slice(&bytes);
                decryptor.decrypt(&result).await?
            }
            crate::InitialSegment::Clear(data) => {
                tmp_file.write_all(&data).await?;
                decryptor.decrypt(&bytes).await?
            }
            crate::InitialSegment::None => decryptor.decrypt(&bytes).await?,
        };
        tmp_file.write_all(&bytes).await?;
    } else {
        if let InitialSegment::Clear(initial_segment) = segment.initial_segment() {
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
        client: HttpClient,
    ) -> impl std::future::Future<Output = IoriResult<bytes::Bytes>> + Send {
        let url = self.url();
        let byte_range = self.byte_range();
        let headers = self.headers();
        async move {
            let mut request = client.get(url);
            if let Some(headers) = headers {
                request = request.headers(headers);
            }
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
