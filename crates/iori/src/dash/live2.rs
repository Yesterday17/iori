use std::{sync::Arc, time::Duration};

use tokio::sync::{mpsc, Mutex};
use url::Url;

use crate::{decrypt::IoriKey, fetch::fetch_segment, HttpClient, IoriResult, StreamingSource};

use super::{live::timeline::MPDTimeline, segment::DashSegment};

pub struct CommonDashLiveSource {
    client: HttpClient,
    mpd_url: Url,
    key: Option<Arc<IoriKey>>,
    timeline: Arc<Mutex<Option<MPDTimeline>>>,
}

impl CommonDashLiveSource {
    pub fn new(client: HttpClient, mpd_url: Url, key: Option<&str>) -> IoriResult<Self> {
        let key = key
            .map(|k| IoriKey::clear_key(k))
            .transpose()?
            .map(Arc::new);

        Ok(Self {
            client,
            mpd_url,
            key,
            timeline: Arc::new(Mutex::new(None)),
        })
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
        let timeline = MPDTimeline::from_mpd(mpd, Some(&self.mpd_url), self.client.clone()).await?;

        let (segments, mut last_update) = timeline.segments_since(None, self.key.clone()).await?;
        sender.send(Ok(segments)).unwrap();

        if timeline.is_dynamic() {
            self.timeline.lock().await.replace(timeline);

            let mpd_url = self.mpd_url.clone();
            let client = self.client.clone();
            let timeline = self.timeline.clone();
            let key = self.key.clone();
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
                    timeline.update_mpd(mpd, &mpd_url).await.unwrap();

                    let (segments, _last_update) = timeline
                        .segments_since(last_update, key.clone())
                        .await
                        .unwrap();
                    sender.send(Ok(segments)).unwrap();

                    if let Some(_last_update) = _last_update {
                        last_update = Some(_last_update);
                    }

                    if timeline.is_static() {
                        break;
                    }
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
