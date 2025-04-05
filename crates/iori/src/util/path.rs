use std::path::PathBuf;

pub(crate) struct DuplicateOutputFileNamer {
    output_path: PathBuf,
    /// The count of files that have been generated.
    file_count: u32,
    file_extension: String,
}

impl DuplicateOutputFileNamer {
    pub fn new(output_path: PathBuf) -> Self {
        let file_extension = output_path
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_string();

        Self {
            output_path,
            file_count: 0,
            file_extension,
        }
    }

    pub fn next(&mut self) -> PathBuf {
        self.file_count += 1;
        self.get_path(self.file_count)
    }

    fn get_path(&self, file_id: u32) -> PathBuf {
        self.output_path
            .with_extension(format!("{file_id}.{}", self.file_extension))
    }
}

impl Drop for DuplicateOutputFileNamer {
    fn drop(&mut self) {
        if self.file_count == 1 {
            if let Err(e) = std::fs::rename(self.get_path(1), &self.output_path) {
                log::error!("Failed to rename file: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::DuplicateOutputFileNamer;

    #[test]
    fn test_file_names() {
        let mut namer = DuplicateOutputFileNamer::new(PathBuf::from("output.ts"));
        for i in 1..=100 {
            assert_eq!(namer.next(), PathBuf::from(format!("output.{i}.ts")));
        }
    }
}
