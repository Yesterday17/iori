pub mod file;

use crate::{error::IoriResult, StreamingSegment};
use tokio::io::{AsyncRead, AsyncWrite};

pub trait CacheSource {
    fn open_writer(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl std::future::Future<
        Output = IoriResult<Option<impl AsyncWrite + Unpin + Send + Sync + 'static>>,
    > + Send;

    fn open_reader(
        &self,
        segment: &impl StreamingSegment,
    ) -> impl std::future::Future<Output = IoriResult<impl AsyncRead + Unpin + Send + Sync + 'static>>
           + Send;
}
