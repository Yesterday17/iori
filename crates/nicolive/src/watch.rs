use std::time::SystemTime;

use futures_util::{sink::SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{model::*, utils::prepare_websocket_request};

pub struct WatchClient {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,

    keep_seat_interval: u64,
    last_keep_seat_time: SystemTime,
}

impl WatchClient {
    pub async fn new<S>(ws_url: S) -> anyhow::Result<Self>
    where
        S: AsRef<str>,
    {
        let (socket, _) = connect_async(prepare_websocket_request(ws_url, Vec::new())?).await?;

        Ok(Self {
            socket,

            keep_seat_interval: 30,
            last_keep_seat_time: SystemTime::now(),
        })
    }

    pub async fn init(&mut self) -> anyhow::Result<()> {
        self.start_watching().await?;
        self.get_akashic().await?;
        self.get_resume().await?;

        Ok(())
    }

    pub async fn recv(&mut self) -> anyhow::Result<Option<WatchResponse>> {
        while let Some(msg) = self.socket.next().await {
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
                    WatchResponse::Room(msg) => return Ok(Some(WatchResponse::Room(msg))),
                    WatchResponse::Statistics(msg) => {
                        return Ok(Some(WatchResponse::Statistics(msg)))
                    }
                    WatchResponse::EventState(_) => (), // dismiss event state
                    WatchResponse::Akashic(msg) => return Ok(Some(WatchResponse::Akashic(msg))),
                }
            }
        }

        Ok(None)
    }

    async fn pong(&mut self) -> anyhow::Result<()> {
        self.socket
            .send(Message::Text(json!({"type":"pong"}).to_string()))
            .await?;
        Ok(())
    }

    async fn keep_seat(&mut self) -> anyhow::Result<()> {
        self.socket
            .send(Message::Text(json!({"type": "keepSeat"}).to_string()))
            .await?;
        self.last_keep_seat_time = SystemTime::now();
        Ok(())
    }

    // Initialize messages
    async fn start_watching(&mut self) -> anyhow::Result<()> {
        self.socket
            .send(Message::Text(
                json!({
                    "type": "startWatching",
                    "data": {
                        "stream": {
                            "quality": "super_high",
                            "protocol": "hls",
                            "latency": "low",
                            "chasePlay": false
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
        self.socket
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
        self.socket
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
            if let Some(WatchResponse::Room(room)) = msg {
                println!("{room:?}");
                std::process::exit(0);
            }
        }
    }
}
