use std::collections::HashMap;

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
pub struct LiveStreamlingList {
    #[serde(default)]
    pub streaming_url_list: Vec<LiveStream>,
}

impl LiveStreamlingList {
    pub fn best(&self, prefer_lhls: bool) -> Option<&LiveStream> {
        let mut streams = self.streaming_url_list.iter().collect::<Vec<_>>();
        streams.sort_by_key(|k| {
            k.quality.unwrap_or(0)
                + if (prefer_lhls && k.r#type == "lhls") || (!prefer_lhls && k.r#type == "hls") {
                    1000000
                } else {
                    0
                }
        });

        streams.last().map(|r| *r)
    }
}

#[derive(Debug, Deserialize)]
pub struct LiveStream {
    pub label: String,
    pub url: String,
    pub quality: Option<u32>, // usually 1000 for normal, 100 for low

    pub id: u8,
    pub r#type: String, // hls, lhls
    #[serde(default)]
    pub is_default: bool,
}

// {"timeshift":{"entrance_url":"https://www.showroom-live.com/premium_live/stu48_8th_Empathy_/j36328","is_private":false,"can_watch_to":1746025140,"status":2,"start_position":0,"can_watch_from":1743908400,"view_url_key":"K86763","live_id":21142701,"room_name":"STU48 8周年コンサート 〜Empathy〜","live_ended_at":1743853916,"timeshift_id":2967,"view_url":"https://www.showroom-live.com/timeshift/stu48_8th_Empathy_/K86763","description":"4月5日(土)<br>\n広島国際会議場 フェニックスホール行われる『STU48 8th Anniversary<br>\nConcert THE STU SHOW〜Empathy〜』コンサート本編＆後日配信される“メンバーと8周年コンサートを振り返ろう”「同時視聴コメンタリー生配信（〜Empathy〜）」の計2配信が視聴できるチケットです。<br>\n<br>\n1️⃣見逃し配信アリ⭕️<br>\n2️⃣メンバーと振り返るコメンタリー生配信🎥<br>\n※出演メンバーは後日お知らせいたします<br>\n<br>\n会場にお越しいただけない方は勿論、来場した方も楽しめる内容盛り沢山です⛴💙<br>\n<br>\n■注意事項<br>\n・チケットのキャンセル及び払戻しについては、理由の如何を問わずお受けできません。<br>\n・当日の状況により、開演・終演時間は変動する場合がございます。<br>\n・機材トラブルにより配信時間が変動する場合がございます。<br>\n・配信の録画・撮影・録音は禁止といたします。","live_type":3,"default_status":2,"live_started_at":1743841813,"title":"STU48 8周年コンサート 〜Empathy〜","room_id":546080}}
#[derive(Debug, Deserialize)]
pub struct TimeshiftInfo {
    pub timeshift: Timeshift,
}

#[derive(Debug, Deserialize)]
pub struct Timeshift {
    pub title: String,
    pub description: String,
    pub room_id: u64,
    pub live_id: u64,
}

#[derive(Debug, Deserialize)]
pub struct TimeshiftStreamingList {
    pub streaming_url_list: HashMap<String, TimeshiftStream>,
}

impl TimeshiftStreamingList {
    pub fn best(&self) -> &TimeshiftStream {
        self.streaming_url_list.get("hls_all").unwrap_or_else(|| {
            self.streaming_url_list
                .get("hls_source")
                .unwrap_or_else(|| {
                    self.streaming_url_list
                        .values()
                        .next()
                        .expect("no timeshift stream")
                })
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TimeshiftStream {
    Hls {
        hls: String,
        /// source, medium, low
        quality: String,
    },
    HlsAll {
        hls_all: String,
        /// all
        quality: String,
    },
}

impl TimeshiftStream {
    pub fn url(&self) -> &str {
        match self {
            TimeshiftStream::Hls { hls, .. } => hls,
            TimeshiftStream::HlsAll { hls_all, .. } => hls_all,
        }
    }
}
