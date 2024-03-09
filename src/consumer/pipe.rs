use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use tokio::fs::File;

use super::ConsumerOutput;
use crate::{error::IoriResult, StreamingSegment};

pub struct PipeConsumer {
    output_dir: PathBuf,

    next: Arc<AtomicU64>,
    segments: Arc<Mutex<HashMap<u64, PathBuf>>>,
}

impl PipeConsumer {
    pub fn new<P>(output_dir: P) -> IoriResult<Self>
    where
        P: Into<PathBuf>,
    {
        let output_dir = output_dir.into();
        std::fs::create_dir_all(&output_dir)?;

        Ok(Self {
            output_dir,

            next: Arc::new(AtomicU64::new(0)),
            segments: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn open_writer(
        &self,
        segment: &(impl StreamingSegment + Send + Sync + 'static),
    ) -> IoriResult<Option<ConsumerOutput>> {
        let filename = segment.file_name();
        let sequence = segment.sequence();
        let filename = format!("{sequence:06}_{filename}");
        let path = self.output_dir.join(filename);

        let file = File::create(&path).await?;

        let next = self.next.clone();
        let segments = self.segments.clone();
        Ok(Some(ConsumerOutput::new(Box::pin(file)).on_finish(
            move || {
                // Hold the lock so that no one would be able to write new segments and modify `next`
                let mut segments = segments.lock().unwrap();

                // write file path to HashMap
                segments.insert(sequence, path);

                if sequence == next.load(Ordering::Relaxed) {
                    while let Some(path) = segments.remove(&next.load(Ordering::Relaxed)) {
                        // open file and write binary content to stdout
                        let mut file = std::fs::File::open(&path).unwrap();
                        std::io::copy(&mut file, &mut std::io::stdout()).unwrap();

                        next.fetch_add(1, Ordering::Relaxed);
                    }
                }
            },
        )))
    }
}
