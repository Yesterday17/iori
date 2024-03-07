use std::path::PathBuf;

use tokio::fs::File;

use crate::error::IoriResult;

pub struct FileConsumer {
    output_dir: PathBuf,
}

impl FileConsumer {
    pub fn new<P>(output_dir: P) -> IoriResult<Self>
    where
        P: Into<PathBuf>,
    {
        let output_dir = output_dir.into();
        if !output_dir.exists() {
            std::fs::create_dir_all(&output_dir)?;
        }
        Ok(Self { output_dir })
    }

    pub async fn open_file(&self, filename: String) -> IoriResult<File> {
        let tmp_file = File::create(self.output_dir.join(filename)).await?;
        Ok(tmp_file)
    }
}
