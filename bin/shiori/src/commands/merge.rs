use std::{collections::HashMap, path::PathBuf, sync::Arc};

use clap::Parser;
use clap_handler::handler;
use iori::{
    cache::CacheSource,
    merge::{IoriMerger, Merger},
    SegmentInfo,
};
use tokio::{
    fs::{read_dir, File},
    sync::Mutex,
};

struct ExistingLocalCache {
    files: Mutex<HashMap<u128, PathBuf>>,
}

impl ExistingLocalCache {
    fn new() -> Self {
        Self {
            files: Mutex::new(HashMap::new()),
        }
    }

    async fn add_file(&self, segment: &SegmentInfo, file: PathBuf) {
        self.files.lock().await.insert(segment.index(), file);
    }
}

impl CacheSource for ExistingLocalCache {
    async fn open_writer(
        &self,
        _segment: &iori::SegmentInfo,
    ) -> iori::IoriResult<Option<iori::cache::CacheSourceWriter>> {
        unreachable!()
    }

    async fn open_reader(
        &self,
        segment: &iori::SegmentInfo,
    ) -> iori::IoriResult<iori::cache::CacheSourceReader> {
        let lock = self.files.lock().await;
        let file = lock.get(&segment.index()).unwrap();
        let file = File::open(file).await?;
        Ok(Box::new(file))
    }

    async fn segment_path(&self, segment: &SegmentInfo) -> Option<PathBuf> {
        self.files.lock().await.get(&segment.index()).cloned()
    }

    async fn invalidate(&self, _segment: &iori::SegmentInfo) -> iori::IoriResult<()> {
        todo!()
    }

    async fn clear(&self) -> iori::IoriResult<()> {
        todo!()
    }
}

#[derive(Parser, Clone, Default, Debug)]
#[clap(name = "merge", short_flag = 'm')]
pub struct MergeCommand {
    #[clap(short, long)]
    pub output: PathBuf,

    pub inputs: Vec<PathBuf>,
}

#[handler(MergeCommand)]
pub async fn merge_command(me: MergeCommand) -> anyhow::Result<()> {
    eprintln!("{:#?}", me);

    let cache = Arc::new(ExistingLocalCache::new());
    let mut merger = IoriMerger::mkvmerge(me.output, true);

    let files = if me.inputs.len() == 1 && me.inputs[0].is_dir() {
        // read all files in directory and merge
        let mut dir = read_dir(&me.inputs[0]).await?;
        let mut files = Vec::new();
        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.ends_with(".DS_Store") {
                continue;
            }

            if path.is_file() {
                files.push(path);
            }
        }
        files.sort();
        files
    } else {
        me.inputs
    };

    eprintln!("{:#?}", files);
    for (sequence, input) in files.into_iter().enumerate() {
        let segment = iori::SegmentInfo {
            sequence: sequence as u64,
            ..Default::default()
        };
        cache.add_file(&segment, input).await;
        merger.update(segment, cache.clone()).await?;
    }

    merger.finish(cache).await?;

    Ok(())
}
