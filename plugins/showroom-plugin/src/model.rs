use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LiveInfo {
    pub live_id: u64,
    pub room_id: u64,

    /// 1: Not Living
    /// 2: Living
    live_status: u64,

    pub room_name: String,
}

impl LiveInfo {
    pub fn is_living(&self) -> bool {
        self.live_status == 2
    }
}

#[derive(Debug, Deserialize)]
pub struct StreamlingList {
    pub streaming_url_list: Vec<Streaming>,
}

impl StreamlingList {
    pub fn best(&self, prefer_lhls: bool) -> &Streaming {
        let mut streams = self.streaming_url_list.iter().collect::<Vec<_>>();
        streams.sort_by_key(|k| {
            k.quality.unwrap_or(0)
                + if (prefer_lhls && k.r#type == "lhls") || (!prefer_lhls && k.r#type == "hls") {
                    1000000
                } else {
                    0
                }
        });

        streams.last().unwrap()
    }
}

#[derive(Debug, Deserialize)]
pub struct Streaming {
    pub label: String,
    pub url: String,
    pub quality: Option<u32>, // usually 1000 for normal, 100 for low

    pub id: u8,
    pub r#type: String, // hls, lhls
    #[serde(default)]
    pub is_default: bool,
}
