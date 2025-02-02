use clap_handler::async_trait;
use serde::{Deserialize, Serialize};

#[async_trait]
pub trait Inspect: Send {
    fn name(&self) -> &'static str;

    /// Check if this handler can handle the URL
    async fn matches(&self, url: &str) -> bool;

    /// Inspect the URL and return the result
    async fn inspect(&self, url: &str) -> anyhow::Result<InspectResult>;
}

#[derive(Serialize, Deserialize, Debug)]
pub enum InspectResult {
    /// This site handler can not handle this URL
    NotMatch,
    /// Inspect data is found
    Playlist(InspectData),
    /// Redirect happens
    Redirect(String),
    /// Inspect data is not found
    None,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum PlaylistType {
    HLS,
    DASH,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InspectData {
    /// URL of the playlist
    pub playlist_url: String,

    /// Type of the playlist
    pub playlist_type: PlaylistType,

    /// Key used to decrypt the media
    pub key: Option<String>,

    /// Headers to use when requesting
    pub headers: Vec<String>,

    /// Metadata of the resource
    pub metadata: Option<String>,

    /// Initial data of the playlist
    ///
    /// Inspector may have already sent a request to the server, in which case we can reuse the data
    // TODO: implement this in iori
    pub initial_playlist_data: Option<String>,
}
