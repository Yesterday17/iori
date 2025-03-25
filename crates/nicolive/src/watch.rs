use std::time::SystemTime;

use fake_user_agent::get_chrome_rua;
use futures_util::{sink::SinkExt, StreamExt};
use reqwest::Client;
use reqwest_websocket::{Message, RequestBuilderExt, WebSocket};
use serde_json::json;

use crate::model::*;

pub struct WatchClient {
    websocket: WebSocket,

    keep_seat_interval: u64,
    last_keep_seat_time: SystemTime,
}

impl WatchClient {
    pub async fn new<S>(ws_url: S) -> anyhow::Result<Self>
    where
        S: AsRef<str>,
    {
        let client = Client::builder()
            .user_agent(get_chrome_rua())
            .build()
            .unwrap();
        let response = client.get(ws_url.as_ref()).upgrade().send().await?;
        let websocket = response.into_websocket().await?;

        Ok(Self {
            websocket,

            keep_seat_interval: 30,
            last_keep_seat_time: SystemTime::now(),
        })
    }

    pub async fn init(&mut self) -> anyhow::Result<()> {
        self.start_watching().await?;
        self.get_akashic().await?;
        // self.get_resume().await?;

        Ok(())
    }

    pub async fn recv(&mut self) -> anyhow::Result<Option<WatchResponse>> {
        while let Some(msg) = self.websocket.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                let data: WatchResponse = serde_json::from_str(&text)?;
                match data {
                    WatchResponse::Ping => {
                        self.pong().await?;

                        // check whether to keep seat
                        let elapsed = self.last_keep_seat_time.elapsed()?;
                        if elapsed.as_secs() >= self.keep_seat_interval {
                            self.keep_seat().await?;
                        }
                    }
                    WatchResponse::ServerTime(_) => (), // dismiss server time
                    WatchResponse::Seat(seat) => {
                        self.last_keep_seat_time = SystemTime::now();
                        self.keep_seat_interval = seat.keep_interval_sec;
                    }
                    WatchResponse::Stream(msg) => return Ok(Some(WatchResponse::Stream(msg))),
                    WatchResponse::MessageServer(msg) => {
                        return Ok(Some(WatchResponse::MessageServer(msg)))
                    }
                    WatchResponse::Statistics(msg) => {
                        return Ok(Some(WatchResponse::Statistics(msg)))
                    }
                    WatchResponse::EventState(_) => (), // dismiss event state
                    WatchResponse::Akashic(msg) => return Ok(Some(WatchResponse::Akashic(msg))),
                    WatchResponse::Schedule(_) => (), // dismiss schedule
                }
            }
        }

        Ok(None)
    }

    async fn pong(&mut self) -> anyhow::Result<()> {
        self.websocket
            .send(Message::Text(json!({"type":"pong"}).to_string()))
            .await?;
        Ok(())
    }

    async fn keep_seat(&mut self) -> anyhow::Result<()> {
        log::debug!("keep seat");
        self.websocket
            .send(Message::Text(json!({"type": "keepSeat"}).to_string()))
            .await?;
        self.last_keep_seat_time = SystemTime::now();
        Ok(())
    }

    // Initialize messages
    async fn start_watching(&mut self) -> anyhow::Result<()> {
        self.websocket
            .send(Message::Text(
                json!({
                    "type": "startWatching",
                    "data": {
                        "stream": {
                            "quality": "super_high",
                            "protocol": "hls",
                            "latency": "low",
                            "chasePlay": false,
                            "accessRightMethod": "single_cookie"
                        },
                        "room": {
                            "protocol": "webSocket",
                            "commentable": true
                        },
                        "reconnect": false
                    }
                })
                .to_string(),
            ))
            .await?;

        Ok(())
    }

    async fn get_akashic(&mut self) -> anyhow::Result<()> {
        self.websocket
            .send(Message::Text(
                json!({
                    "type": "getAkashic",
                    "data": {
                        "chasePlay":false
                    }
                })
                .to_string(),
            ))
            .await?;

        Ok(())
    }

    async fn get_resume(&mut self) -> anyhow::Result<()> {
        self.websocket
            .send(Message::Text(json!({"type":"getResume"}).to_string()))
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{model::WatchResponse, program::NicoEmbeddedData, watch::WatchClient};

    #[tokio::test]
    async fn test_get_room() -> anyhow::Result<()> {
        let data =
            NicoEmbeddedData::new("https://live.nicovideo.jp/watch/lv342260645", None).await?;
        let wss_url = data.websocket_url().expect("No websocket url found");

        let mut watcher = WatchClient::new(wss_url).await.unwrap();
        watcher.init().await.unwrap();

        loop {
            let msg = watcher.recv().await.unwrap();
            if let Some(WatchResponse::MessageServer(message_server)) = msg {
                println!("{message_server:?}");
                std::process::exit(0);
            }
        }
    }
}
