use crate::StreamingSource;

pub struct SequencialDownloader<S>
where
    S: StreamingSource,
{
    source: S,
}

impl<S> SequencialDownloader<S>
where
    S: StreamingSource,
{
    pub fn new(source: S) -> Self {
        Self { source }
    }

    pub async fn download(&mut self) {
        let mut info = self.source.fetch_info().await;
        while let Some(segment) = info.recv().await {
            self.source.fetch_segment(segment).await;
        }
    }
}
