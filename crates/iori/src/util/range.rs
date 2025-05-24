#[derive(Debug, Clone)]
pub struct ByteRange {
    pub offset: u64,
    pub length: Option<u64>,
}

impl ByteRange {
    pub fn new(offset: u64, length: Option<u64>) -> Self {
        Self { offset, length }
    }

    pub fn to_http_range(&self) -> String {
        if let Some(length) = self.length {
            format!("bytes={}-{}", self.offset, self.offset + length - 1)
        } else {
            format!("bytes={}-", self.offset)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_http_range() {
        let range = ByteRange::new(10, Some(10));
        assert_eq!(range.to_http_range(), "bytes=10-19");

        let range = ByteRange::new(10, None);
        assert_eq!(range.to_http_range(), "bytes=10-");
    }
}
