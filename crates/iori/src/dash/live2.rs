use std::{sync::Arc, time::Duration};

use tokio::sync::{mpsc, Mutex};
use url::Url;

use crate::{fetch::fetch_segment, HttpClient, IoriResult, StreamingSource};

use super::{live::timeline::MPDTimeline, segment::DashSegment};

pub struct CommonDashLiveSource {
    client: HttpClient,
    mpd_url: Url,
    timeline: Arc<Mutex<Option<MPDTimeline>>>,
}

impl CommonDashLiveSource {
    pub fn new(client: HttpClient, mpd_url: Url) -> Self {
        Self {
            client,
            mpd_url,
            timeline: Arc::new(Mutex::new(None)),
        }
    }
}

impl StreamingSource for CommonDashLiveSource {
    type Segment = DashSegment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let mpd = self
            .client
            .get(self.mpd_url.as_ref())
            .send()
            .await?
            .text()
            .await?;
        let mpd = dash_mpd::parse(&mpd)?;

        let minimum_update_period = mpd.minimumUpdatePeriod.unwrap_or(Duration::from_secs(2));
        let timeline = MPDTimeline::from_mpd(mpd, Some(&self.mpd_url))?;

        let (segments, mut last_update) = timeline.segments_since(None);
        sender.send(Ok(segments)).unwrap();

        if timeline.is_dynamic() {
            self.timeline.lock().await.replace(timeline);

            let mpd_url = self.mpd_url.clone();
            let client = self.client.clone();
            let timeline = self.timeline.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(minimum_update_period).await;

                    let mpd = client
                        .get(mpd_url.as_ref())
                        .send()
                        .await
                        .unwrap()
                        .text()
                        .await
                        .unwrap();
                    let mpd = dash_mpd::parse(&mpd).unwrap();

                    let mut timeline = timeline.lock().await;
                    let timeline = timeline.as_mut().unwrap();
                    timeline.update_mpd(mpd, &mpd_url).unwrap();

                    let (segments, _last_update) = timeline.segments_since(Some(last_update));
                    sender.send(Ok(segments)).unwrap();

                    last_update = _last_update;
                }
            });
        }

        Ok(receiver)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        fetch_segment(self.client.clone(), segment, writer, None).await
    }
}
