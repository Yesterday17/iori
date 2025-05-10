#[cfg(feature = "extism")]
mod extism;

#[cfg(feature = "extism")]
pub mod extism_pdk {
    pub use crate::extism::*;
    pub use extism_pdk::*;
}

pub use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub trait InspectorCommand {
    fn add_argument(
        &mut self,
        long: &'static str,
        value_name: Option<&'static str>,
        help: &'static str,
    );

    fn add_boolean_argument(&mut self, long: &'static str, help: &'static str);
}

pub trait InspectorArguments: Send + Sync {
    fn get_string(&self, argument: &'static str) -> Option<String>;
    fn get_boolean(&self, argument: &'static str) -> bool;
}

pub trait InspectorBuilder {
    fn name(&self) -> String;

    fn help(&self) -> Vec<String> {
        vec!["No help available".to_string()]
    }

    fn arguments(&self, _command: &mut dyn InspectorCommand) {}

    fn build(&self, args: &dyn InspectorArguments) -> anyhow::Result<Box<dyn Inspect>>;
}

#[async_trait]
pub trait Inspect: Send + Sync {
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
    /// Multiple playlists are found and need to be downloaded
    Playlists(Vec<InspectPlaylist>),
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
    Raw(String),
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

    /// Cookies to use when requesting
    pub cookies: Vec<String>,

    /// Initial data of the playlist
    ///
    /// Inspector may have already sent a request to the server, in which case we can reuse the data
    pub initial_playlist_data: Option<String>,

    /// Hints how many streams does this playlist contains.
    pub streams_hint: Option<u32>,
}

pub trait InspectorApp {
    fn choose_candidates(&self, candidates: Vec<InspectCandidate>) -> Vec<InspectCandidate>;
}
