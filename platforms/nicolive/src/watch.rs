use std::time::Duration;

use fake_user_agent::get_chrome_rua;
use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream},
    StreamExt,
};
use reqwest::Client;
use reqwest_websocket::{Message, RequestBuilderExt, WebSocket};
use serde_json::json;
use tokio::{
    sync::Mutex,
    time::{Instant, Interval},
};

use crate::model::*;

pub struct WatchClient {
    sender: Mutex<SplitSink<WebSocket, Message>>,
    receiver: Mutex<SplitStream<WebSocket>>,

    keep_seat_interval: Mutex<Option<Interval>>,
}

impl WatchClient {
    pub async fn new<S>(ws_url: S) -> anyhow::Result<Self>
    where
        S: AsRef<str>,
    {
        let client = Client::builder()
            .user_agent(get_chrome_rua())
            // https://github.com/jgraef/reqwest-websocket/issues/2
            .http1_only()
            .build()
            .unwrap();
        let response = client.get(ws_url.as_ref()).upgrade().send().await?;
        let websocket = response.into_websocket().await?;
        let (sender, receiver) = websocket.split();

        Ok(Self {
            sender: Mutex::new(sender),
            receiver: Mutex::new(receiver),

            keep_seat_interval: Mutex::new(None),
        })
    }

    pub async fn reconnect(
        &self,
        ws_url: &str,
        quality: &str,
        chase_play: bool,
    ) -> anyhow::Result<()> {
        log::info!("Reconnecting...");

        // lock before making new connection
        let mut sender = self.sender.lock().await;
        let mut receiver = self.receiver.lock().await;

        let client = Client::builder()
            .user_agent(get_chrome_rua())
            // https://github.com/jgraef/reqwest-websocket/issues/2
            .http1_only()
            .build()
            .unwrap();
        let response = client.get(ws_url).upgrade().send().await?;
        let websocket = response.into_websocket().await?;
        let (_sender, _receiver) = websocket.split();

        *sender = _sender;
        *receiver = _receiver;

        // release lock after reconnected
        drop(sender);
        drop(receiver);

        self.send_start_watching(quality, chase_play, true).await?;
        Ok(())
    }

    pub async fn recv(&self) -> anyhow::Result<Option<WatchResponse>> {
        while let Some(msg) = self.receiver.lock().await.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                let data: WatchResponse = serde_json::from_str(&text)?;
                log::debug!("recv: {:?}", data);
                match data {
                    WatchResponse::Ping => {
                        self.pong().await?;
                        self.send_keep_seat().await?;
                    }
                    WatchResponse::ServerTime(_) => (), // dismiss server time
                    WatchResponse::Seat(seat) => {
                        // self.notify_new_visit().await?;
                        *self.keep_seat_interval.lock().await = Some(tokio::time::interval_at(
                            Instant::now() + Duration::from_secs(seat.keep_interval_sec),
                            Duration::from_secs(seat.keep_interval_sec),
                        ));
                    }
                    WatchResponse::Stream(msg) => return Ok(Some(WatchResponse::Stream(msg))),
                    WatchResponse::MessageServer(msg) => {
                        self.get_akashic().await?;
                        return Ok(Some(WatchResponse::MessageServer(msg)));
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

    pub(crate) async fn send(&self, msg: Message) -> Result<(), reqwest_websocket::Error> {
        log::debug!("send: {:?}", msg);
        self.sender.lock().await.send(msg).await
    }

    async fn pong(&self) -> anyhow::Result<()> {
        self.send(Message::Text(json!({"type":"pong"}).to_string()))
            .await?;
        Ok(())
    }

    pub(crate) async fn keep_seat(&self) -> anyhow::Result<()> {
        let mut interval = self.keep_seat_interval.lock().await;
        if interval.is_none() {
            return Ok(());
        }

        if let Some(ticker) = interval.as_mut() {
            ticker.tick().await;
        }

        self.send_keep_seat().await?;
        Ok(())
    }

    async fn send_keep_seat(&self) -> anyhow::Result<()> {
        log::debug!("keep seat");
        self.send(Message::Text(json!({"type": "keepSeat"}).to_string()))
            .await?;
        Ok(())
    }

    // Initialize messages
    pub async fn start_watching(&self, quality: &str, chase_play: bool) -> anyhow::Result<()> {
        self.send_start_watching(quality, chase_play, false).await
    }

    async fn send_start_watching(
        &self,
        quality: &str,
        chase_play: bool,
        reconnect: bool,
    ) -> anyhow::Result<()> {
        self.send(Message::Text(
            json!({
                "type": "startWatching",
                "data": {
                    "stream": {
                        "quality": quality,
                        "protocol": "hls",
                        "latency": "low",
                        "chasePlay": chase_play,
                        "accessRightMethod": "single_cookie"
                    },
                    "room": {
                        "protocol": "webSocket",
                        "commentable": true
                    },
                    "reconnect": reconnect
                }
            })
            .to_string(),
        ))
        .await?;

        Ok(())
    }

    async fn get_akashic(&self) -> anyhow::Result<()> {
        self.send(Message::Text(
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
}

#[cfg(test)]
mod tests {
    use crate::{model::WatchResponse, program::NicoEmbeddedData, watch::WatchClient};

    #[tokio::test]
    async fn test_get_room() -> anyhow::Result<()> {
        let data =
            NicoEmbeddedData::new("https://live.nicovideo.jp/watch/lv342260645", None).await?;
        let wss_url = data.websocket_url().expect("No websocket url found");

        let watcher = WatchClient::new(wss_url).await.unwrap();
        watcher.start_watching("super_high", false).await.unwrap();

        loop {
            let msg = watcher.recv().await.unwrap();
            if let Some(WatchResponse::MessageServer(message_server)) = msg {
                println!("{message_server:?}");
                std::process::exit(0);
            }
        }
    }
}
