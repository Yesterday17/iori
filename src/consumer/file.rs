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

    pub async fn open_writer(&self, filename: String) -> IoriResult<Option<File>> {
        let path = self.output_dir.join(filename);
        if path.exists() {
            log::warn!("File {} already exists, ignoring.", path.display());
            return Ok(None);
        }
        let tmp_file = File::create(path).await?;
        Ok(Some(tmp_file))
    }
}
