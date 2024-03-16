use std::collections::BTreeSet;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{model::*, utils::prepare_websocket_request};

pub struct DanmakuClient {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,

    thread_id: String,
    when: u64,
    last_no: Option<u64>,

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

            last_no: Some(u64::MAX),

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

    pub async fn recv_all(&mut self) -> anyhow::Result<Vec<DanmakuMessageChat>> {
        let mut result = BTreeSet::new();

        loop {
            let danmaku = self.recv().await?;

            if danmaku.is_empty() {
                break;
            }

            result.extend(danmaku.chats);
        }

        Ok(result.into_iter().collect())
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

#[cfg(test)]
mod tests {
    use super::DanmakuClient;
    use crate::{model::WatchResponse, program::NicoEmbeddedData, watch::WatchClient};

    #[tokio::test]
    async fn test_get_danmaku() -> anyhow::Result<()> {
        let data =
            NicoEmbeddedData::new("https://live.nicovideo.jp/watch/lv342260645", None).await?;
        let wss_url = data.websocket_url().expect("No websocket url found");

        let mut watcher = WatchClient::new(wss_url).await.unwrap();
        watcher.init().await.unwrap();

        let room = loop {
            let msg = watcher.recv().await.unwrap();
            if let Some(WatchResponse::Room(room)) = msg {
                break room;
            }
        };

        let mut client = DanmakuClient::new(
            room.message_server.uri,
            room.thread_id,
            data.program_end_time(),
        )
        .await?;

        let mut no = 0;
        let danmaku = client.recv_all().await?;

        if danmaku[0].no.is_some() {
            for danmaku in danmaku.iter() {
                no += 1;

                loop {
                    if danmaku.no.unwrap() > no {
                        eprintln!("Missing: {no}");
                        no += 1;
                    } else {
                        break;
                    }
                }
            }
        }
        println!("{}", serde_json::to_string(&danmaku)?);

        Ok(())
    }
}
