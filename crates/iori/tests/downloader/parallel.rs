use std::sync::{atomic::AtomicU8, Arc};

use iori::{cache::memory::MemoryCacheSource, download::ParallelDownloader, merge::SkipMerger};

use crate::source::{TestSegment, TestSource};

#[tokio::test]
async fn test_parallel_downloader_with_failed_retry() -> anyhow::Result<()> {
    let source = TestSource::new(vec![TestSegment {
        stream_id: 1,
        sequence: 1,
        file_name: "test.ts".to_string(),
        fail_count: Arc::new(AtomicU8::new(2)),
    }]);

    let cache = Arc::new(MemoryCacheSource::new());

    ParallelDownloader::builder()
        .merger(SkipMerger)
        .cache(cache.clone())
        .retries(1)
        .download(source)
        .await?;

    let result = cache.into_inner();
    let result = result.lock().unwrap();
    assert_eq!(result.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_parallel_downloader_with_success_retry() -> anyhow::Result<()> {
    let source = TestSource::new(vec![TestSegment {
        stream_id: 1,
        sequence: 1,
        file_name: "test.ts".to_string(),
        fail_count: Arc::new(AtomicU8::new(2)),
    }]);

    let cache = Arc::new(MemoryCacheSource::new());

    ParallelDownloader::builder()
        .merger(SkipMerger)
        .cache(cache.clone())
        .retries(3)
        .download(source)
        .await?;

    let result = cache.into_inner();
    let result = result.lock().unwrap();
    assert_eq!(result.len(), 1);

    Ok(())
}
