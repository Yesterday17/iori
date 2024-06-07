pub mod archive;

use std::sync::Arc;

// pub use decrypt::M3u8Key;
pub use m3u8_rs;

use crate::StreamingSegment;

pub struct DashSegment {
    pub url: reqwest::Url,
    pub filename: String,

    // pub key: Option<Arc<decrypt::M3u8Key>>,
    pub initial_segment: Option<Arc<Vec<u8>>>,

    pub byte_range: Option<m3u8_rs::ByteRange>,

    /// Sequence id allocated by the downloader, starts from 0
    pub sequence: u64,
}

// pub trait M3u8StreamingSegment: StreamingSegment {
//     fn url(&self) -> reqwest::Url;
//     // fn key(&self) -> Option<Arc<decrypt::M3u8Key>>;
//     fn initial_segment(&self) -> Option<Arc<Vec<u8>>>;
//     fn byte_range(&self) -> Option<m3u8_rs::ByteRange>;
// }

impl StreamingSegment for DashSegment {
    fn sequence(&self) -> u64 {
        self.sequence
    }

    fn file_name(&self) -> &str {
        self.filename.as_str()
    }

    fn initial_segment(&self) -> Option<Arc<Vec<u8>>> {
        self.initial_segment.clone()
    }
}

// impl M3u8StreamingSegment for DashSegment {
//     fn url(&self) -> reqwest::Url {
//         self.url.clone()
//     }

//     // fn key(&self) -> Option<Arc<decrypt::M3u8Key>> {
//     //     self.key.clone()
//     // }
// }
