use std::collections::BTreeSet;

use fake_user_agent::get_chrome_rua;
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use protocol::{
    data::nicolive_message::Data,
    service::edge::{
        chunked_entry::Entry, chunked_message::Payload, BackwardSegment, PackedSegment,
    },
};
use reqwest::Client;
use serde_json::json;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{model::*, utils::prepare_websocket_request, xml2ass::xml2ass};

pub mod protocol {
    pub mod data {
        pub mod atoms {
            include!(concat!(
                env!("OUT_DIR"),
                "/dwango.nicolive.chat.data.atoms.rs"
            ));
        }
        include!(concat!(env!("OUT_DIR"), "/dwango.nicolive.chat.data.rs"));
    }
    pub mod service {
        pub mod edge {
            include!(concat!(
                env!("OUT_DIR"),
                "/dwango.nicolive.chat.service.edge.rs"
            ));
        }
    }
}

pub struct DanmakuClient {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,

    thread_id: String,
    when: u64,
    last_no: Option<i64>,

    r: u64,
    p: u64,
}

impl DanmakuClient {
    pub async fn new(
        danmaku_server_url: String,
        thread_id: String,
        end_time: u64,
    ) -> anyhow::Result<Self> {
        let (socket, _) =
            connect_async(prepare_websocket_request(danmaku_server_url, Vec::new())?).await?;
        Ok(Self {
            socket,
            thread_id,
            when: end_time,

            last_no: Some(i64::MAX),

            r: 0,
            p: 0,
        })
    }

    pub async fn recv(&mut self) -> anyhow::Result<DanmakuThread> {
        let mut result = DanmakuThread::new();
        if let Some(1) = self.last_no {
            // got the first danmaku, finishing
            return Ok(result);
        }

        let final_indicator = format!("rf:{}", self.r);
        self.request_thread().await?;
        loop {
            if let Some(msg) = self.socket.next().await {
                let msg = msg?;
                if let Message::Text(text) = msg {
                    log::trace!("message = {text}");
                    let data: DanmakuResponse = serde_json::from_str(&text)?;
                    match data {
                        DanmakuResponse::Ping(ping) => {
                            if ping.content == final_indicator {
                                break;
                            }
                        }
                        DanmakuResponse::Thread(msg) => result.thread = Some(msg),
                        DanmakuResponse::Chat(msg) => {
                            if result.is_empty() && msg.date == self.when {
                                break;
                            }

                            result.chats.push(msg);
                        }
                    }
                }
            }
        }

        if !result.is_empty() {
            self.last_no = result.chats[0].no.clone();
            self.when = result.chats[0].date
        }

        Ok(result)
    }

    pub async fn recv_all(&mut self) -> anyhow::Result<DanmakuList> {
        let mut result = BTreeSet::new();

        loop {
            let danmaku = self.recv().await?;

            if danmaku.is_empty() {
                break;
            }

            result.extend(danmaku.chats);
        }

        Ok(DanmakuList(result.into_iter().collect()))
    }

    async fn request_thread(&mut self) -> anyhow::Result<()> {
        let message = json!([
            {"ping": {"content": format!("rs:{}", self.r)}},
            {"ping": {"content": format!("ps:{}", self.p)}},
            {
                "thread": {
                    "thread": self.thread_id,
                    "version": "20061206",
                    "when": self.when + 10,
                    "user_id": "guest",
                    "res_from": -1000,
                    "with_global": 1,
                    "scores": 1,
                    "nicoru": 0,
                    "waybackkey": "",
                }
            },
            {"ping": {"content": format!("pf:{}", self.p)}},
            {"ping": {"content": format!("rf:{}", self.r)}},
        ]);
        self.socket.send(Message::Text(message.to_string())).await?;

        self.r += 1;
        self.p += 5;

        Ok(())
    }
}

pub struct NewDanmakuClient {
    client: Client,

    view_uri: String,
}

impl NewDanmakuClient {
    pub async fn new(view_uri: String) -> anyhow::Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ORIGIN,
            reqwest::header::HeaderValue::from_str("https://live.nicovideo.jp")?,
        );
        headers.insert(
            reqwest::header::REFERER,
            reqwest::header::HeaderValue::from_str("https://live.nicovideo.jp/")?,
        );
        let client = Client::builder()
            .default_headers(headers)
            .user_agent(get_chrome_rua())
            .build()?;
        Ok(Self { client, view_uri })
    }

    pub async fn get_backward_segment(&self, mut at: String) -> anyhow::Result<BackwardSegment> {
        // TODO: limit loop times
        loop {
            let response = self
                .client
                .get(&self.view_uri)
                .query(&[("at", &at)])
                .send()
                .await?;
            let data = response.error_for_status()?.bytes().await?;
            let s = protocol::service::edge::ChunkedEntry::decode_length_delimited(data)?;

            if let Some(entry) = s.entry {
                match entry {
                    Entry::Backward(backward) => return Ok(backward),
                    Entry::Previous(previous) => {
                        unimplemented!("{previous:#?}")
                    }
                    Entry::Segment(segment) => {
                        unimplemented!("{segment:#?}")
                    }
                    Entry::Next(next) => {
                        at = next.at.to_string();
                    }
                }
            };
        }
    }

    pub async fn recv(
        &self,
        uri: String,
        start_time: Option<i64>,
    ) -> anyhow::Result<(DanmakuList, Option<String>)> {
        let data = self.client.get(uri).send().await?;
        let b = data.error_for_status()?.bytes().await?;
        let segment: PackedSegment = prost::Message::decode(b)?;

        let mut danmakus = Vec::with_capacity(segment.messages.len());
        for message in segment.messages {
            if let (Some(meta), Some(payload)) = (message.meta, message.payload) {
                match payload {
                    Payload::Message(message) => {
                        if let Some(data) = message.data {
                            match data {
                                Data::Chat(chat) | Data::OverflowedChat(chat) => {
                                    danmakus.push(DanmakuMessageChat::from_chat(chat, &meta))
                                }
                                Data::SimpleNotification(notification) => {
                                    eprintln!("unhandled simple_notification: {notification:?}");
                                }
                                Data::Gift(_) => {}
                                Data::Nicoad(_) => {}
                                Data::GameUpdate(game_update) => {
                                    eprintln!("unhandled game_update: {game_update:?}");
                                }
                                Data::TagUpdated(tag_updated) => {
                                    eprintln!("unhandled tag_updated: {tag_updated:?}");
                                }
                                Data::ModeratorUpdated(updated) => {
                                    eprintln!("unhandled moderator_updated: {updated:?}");
                                }
                                Data::SsngUpdated(ssng_updated) => {
                                    eprintln!("unhandled ssng_updated: {ssng_updated:?}")
                                }
                            }
                        }
                    }
                    Payload::State(state) => {
                        // {
                        //   "thread":"M.K4fxhVMa5jZmjb464HyG4A",
                        //   "no":21324,
                        //   "vpos":534110,
                        //   "date":1704378541,
                        //   "date_usec":118206,
                        //   "mail":"184",
                        //   "user_id":"vht4QQNupbLDtvKRx-rJkstr2Hg",
                        //   "premium":3,
                        //   "anonymity":1,
                        //   "content":"以上で番組は終了です。皆さん、みりおっつ～"
                        // }
                        if let Some(marquee) = &state.marquee {
                            if let Some(display) = &marquee.display {
                                if let Some(comment) = &display.operator_comment {
                                    danmakus.push(DanmakuMessageChat::from_operator_comment(
                                        comment.clone(),
                                        &meta,
                                        start_time,
                                    ));
                                }
                            }
                            continue;
                        }

                        if let Some(enquete) = &state.enquete {
                            danmakus.push(DanmakuMessageChat::from_enquete(
                                enquete.clone(),
                                &meta,
                                start_time,
                            ));
                            continue;
                        }

                        eprintln!("unhandled state: {state:?}");
                    }
                    Payload::Signal(signal) => {
                        eprintln!("unhandled signal: {signal:?}");
                    }
                }
            }
        }

        Ok((DanmakuList(danmakus), segment.next.map(|n| n.uri)))
    }

    pub async fn recv_all(
        &self,
        mut url: String,
        start_time: Option<i64>,
    ) -> anyhow::Result<DanmakuList> {
        let mut danmakus = Vec::new();
        loop {
            let (messages, next) = self.recv(url, start_time).await?;
            danmakus.extend(messages.into_inner());
            if let Some(next) = next {
                url = next;
            } else {
                break;
            }
        }

        let mut danmakus = DanmakuList(danmakus);
        danmakus.sort();
        Ok(danmakus)
    }
}

pub struct DanmakuList(Vec<DanmakuMessageChat>);

impl DanmakuList {
    pub fn sort(&mut self) {
        self.0.sort_by_key(|d| d.vpos.unwrap_or(0));
    }

    pub fn into_inner(self) -> Vec<DanmakuMessageChat> {
        self.0
    }

    pub fn iter(&self) -> std::slice::Iter<'_, DanmakuMessageChat> {
        self.0.iter()
    }

    pub fn to_json(&self, pretty: bool) -> anyhow::Result<String> {
        if pretty {
            Ok(serde_json::to_string_pretty(&self.0)?)
        } else {
            Ok(serde_json::to_string(&self.0)?)
        }
    }

    pub fn to_ass(&self) -> anyhow::Result<String> {
        xml2ass(self)
    }
}

#[cfg(test)]
mod tests {
    use super::NewDanmakuClient;
    use crate::{model::WatchResponse, program::NicoEmbeddedData, watch::WatchClient};
    use chrono::{DateTime, Utc};
    use std::str::FromStr;

    #[tokio::test]
    async fn test_get_danmaku() -> anyhow::Result<()> {
        let data =
            NicoEmbeddedData::new("https://live.nicovideo.jp/watch/lv345610602", None).await?;
        let wss_url = data.websocket_url().expect("No websocket url found");

        let mut watcher = WatchClient::new(wss_url).await.unwrap();
        watcher.init().await.unwrap();

        let message_server = loop {
            let msg = watcher.recv().await.unwrap();
            if let Some(WatchResponse::MessageServer(message_server)) = msg {
                break message_server;
            }
        };

        let client = NewDanmakuClient::new(message_server.view_uri).await?;
        let start_time = DateTime::<Utc>::from_str(&message_server.vpos_base_time)
            .map(|r| r.timestamp())
            .ok();
        let at = "now".to_string();
        let backward = client.get_backward_segment(at).await?;
        if let Some(segment) = backward.segment {
            let danmakus = client.recv_all(segment.uri, start_time).await?;
            std::fs::write("/tmp/test.json", danmakus.to_json(true)?)?;
        }
        Ok(())
    }
}
