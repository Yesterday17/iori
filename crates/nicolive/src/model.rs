use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum WatchResponse {
    /// Ping message, should respond Pong
    Ping,
    ServerTime(WatchMessageServerTime),
    Seat(WatchMessageSeat),
    Stream(WatchMessageStream),
    Room(WatchMessageRoom),
    Statistics(WatchMessageStatistics),
    EventState(WatchMessageEventState),
    Akashic(WatchMessageAkashic),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchMessageServerTime {
    pub current_ms: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchMessageSeat {
    pub keep_interval_sec: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchMessageStream {
    /// hls
    pub protocol: String,

    /// super_high
    pub quality: String,
    /// ["abr", "super_high", "high", "normal", "low", "super_low", "audio_high"]
    pub available_qualities: Vec<String>,

    /// sync json uri
    pub sync_uri: String,
    /// HLS m3u8 uri
    pub uri: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WatchMessageRoom {
    pub name: String,
    pub is_first: bool,
    pub thread_id: String,
    pub waybackkey: String,
    pub your_post_key: Option<String>,

    pub vpos_base_time: String,
    pub message_server: DanmakuMessageServer,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DanmakuMessageServer {
    /// niwavided
    pub r#type: String,
    /// wss://
    pub uri: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WatchMessageStatistics {
    pub viewers: i32,
    pub comments: i32,
    pub ad_points: i32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WatchMessageEventState {
    pub comment_state: WatchMessageEventStateCommentState,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WatchMessageEventStateCommentState {
    pub layout: String, // normal
    pub locked: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WatchMessageAkashic {
    pub content_url: String,
    pub log_server_url: String,
    pub play_id: String,
    pub player_id: String,
    pub status: String,
    pub token: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum DanmakuResponse {
    Ping(DanmakuMessagePing),
    Thread(DanmakuMessageThread),
    Chat(DanmakuMessageChat),
}

#[derive(Deserialize, Debug)]
pub struct DanmakuMessagePing {
    pub content: String,
}

#[derive(Deserialize, Debug)]
pub struct DanmakuMessageThread {
    pub last_res: Option<u64>,
    pub revision: i32,
    pub resultcode: i32,
    pub server_time: u64,
    pub thread: String,
    pub ticket: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Ord)]
pub struct DanmakuMessageChat {
    // {
    //   "thread": "M.V67enstLPeSLVO0U1lcbDA",
    //   "no": 6,
    //   "vpos": 437,
    //   "date": 1693485004,
    //   "date_usec": 565330,
    //   "mail": "184",
    //   "user_id": "-Br5e0CRUyNpHCL3i4SrPOWyqVU",
    //   "premium": 1,
    //   "anonymity": 1,
    //   "content": "(*>△<)＜ ナーンナーンっっ"
    // }
    //
    // {
    //   "thread": "M.V67enstLPeSLVO0U1lcbDA",
    //   "no": 95,
    //   "vpos": 7955,
    //   "date": 1693485081,
    //   "date_usec": 566853,
    //   "name": "インパクト",
    //   "user_id": "12015520",
    //   "premium": 24,
    //   "content": "かーっ！"
    // }
    // {
    //     "chat": {
    //         "thread": "M.YDYUsM77eTeeX0zyRfJEIg",
    //         "vpos": 241,
    //         "date": 1693482605,
    //         "date_usec": 980236,
    //         "mail": "184",
    //         "user_id": "n20sthjgLwnk5_r25NbrtX8XnHU",
    //         "premium": 1,
    //         "anonymity": 1,
    //         "content": "さいまえ"
    //     }
    // },
    pub thread: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no: Option<u64>,
    // vpos might be negative:
    // {"chat":{"thread":"M.V67enstLPeSLVO0U1lcbDA","no":1,"vpos":-298283036,"date":1690502169,"date_usec":660934,"mail":"184","user_id":"zQVo171K_fu_CsvU99gO305dOU4","premium":3,"anonymity":1,"content":"/trialpanel on 1"}}
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpos: Option<i64>,

    pub date: u64,
    pub date_usec: u64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mail: Option<String>,
    pub user_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anonymity: Option<u8>,

    pub content: String,
}

impl PartialOrd for DanmakuMessageChat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // compare date first, then vpos
        Some(self.date.cmp(&other.date).then(self.vpos.cmp(&other.vpos)))
    }
}

pub struct DanmakuThread {
    pub thread: Option<DanmakuMessageThread>,
    pub chats: Vec<DanmakuMessageChat>,
}

impl DanmakuThread {
    pub(crate) fn new() -> Self {
        Self {
            thread: None,
            chats: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.chats.is_empty()
    }
}
