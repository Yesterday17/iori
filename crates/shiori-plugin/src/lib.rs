use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[async_trait]
pub trait Inspect: Send + Sync {
    fn name(&self) -> &'static str;

    /// Check if this handler can handle the URL
    async fn matches(&self, url: &str) -> bool;

    /// Inspect the URL and return the result
    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult>;

    /// Inspect a previously returned candidate and return the result
    async fn inspect_candidate(
        &self,
        _candidate: InspectCandidate,
    ) -> anyhow::Result<InspectResult> {
        Ok(InspectResult::None)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum InspectResult {
    /// This site handler can not handle this URL
    NotMatch,
    /// Found multiple available sources to choose
    Candidates(Vec<InspectCandidate>),
    /// Inspect data is found
    Playlist(InspectPlaylist),
    /// Redirect happens
    Redirect(String),
    /// Inspect data is not found
    None,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InspectCandidate {
    pub title: String,

    pub playlist_type: Option<PlaylistType>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub enum PlaylistType {
    #[default]
    HLS,
    DASH,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct InspectPlaylist {
    /// Metadata of the resource
    pub title: Option<String>,

    /// URL of the playlist
    pub playlist_url: String,

    /// Type of the playlist
    pub playlist_type: PlaylistType,

    /// Key used to decrypt the media
    pub key: Option<String>,

    /// Headers to use when requesting
    pub headers: Vec<String>,

    /// Initial data of the playlist
    ///
    /// Inspector may have already sent a request to the server, in which case we can reuse the data
    pub initial_playlist_data: Option<String>,
}

pub trait InspectorApp {
    fn choose_candidates(&self, candidates: Vec<InspectCandidate>) -> Vec<InspectCandidate>;
}

pub trait InspectExt: Inspect {
    fn to_box(self) -> Box<Self>
    where
        Self: Sized,
    {
        Box::new(self)
    }
}

impl<T: Inspect> InspectExt for T {}
