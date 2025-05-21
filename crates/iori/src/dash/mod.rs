//! # MPEG-DASH Streaming Support
//!
//! This module provides support for downloading MPEG-DASH streams, covering both
//! Video-on-Demand (VoD) / static content and Live streaming scenarios.
//!
//! ## VoD (Static MPD)
//!
//! For static MPDs (where `MPD@type` is typically "static"), the content is fully described
//! and all segments are available. Use [`archive::CommonDashArchiveSource`] for these streams.
//! It parses the MPD once and provides a complete list of segments to download.
//!
//! ## Live Streaming (Dynamic MPD)
//!
//! For live DASH streams (where `MPD@type` is "dynamic"), the MPD is updated periodically
//! by the server to reflect new available segments and remove old ones. [`live::LiveDashSource`]
//! is designed to handle these dynamic MPDs.
//!
//! ### `LiveDashSource` Capabilities:
//!
//! *   **Dynamic MPD Handling:** Periodically fetches and re-parses the MPD according to the
//!     `MPD@minimumUpdatePeriod` attribute.
//! *   **Clock Synchronization:** Synchronizes its internal clock with time sources specified
//!     in the MPD (e.g., using `UTCTiming` elements with schemes like `urn:mpeg:dash:utc:http-xsdate:2014`).
//!     This is crucial for accurately determining segment availability.
//! *   **Segment Generation:** Calculates the list of currently available segments based on:
//!     *   The live window defined by `MPD@timeShiftBufferDepth` and `MPD@suggestedPresentationDelay`.
//!     *   Segment addressing schemes:
//!         *   `SegmentTemplate` with `@duration` for numbered segments.
//!         *   `SegmentTemplate` with `SegmentTimeline` for explicitly timed segments with potential repeats.
//!         *   `SegmentList` for explicitly listed segment URLs.
//! *   **Initialization Segment Management:** Fetches and caches initialization segments (init segments)
//!     when the active `Representation` changes or when a more specific init segment is defined
//!     (e.g., in `SegmentTemplate` or `SegmentList`). This data is propagated to `DashSegment`
//!     instances, making it available for decryption if needed.
//!
//! ### Example Usage
//!
//! An example demonstrating the use of `LiveDashSource` can be found in
//! `crates/iori/examples/live_dash.rs`. This example shows how to set up the source,
//! downloader, and process a live DASH stream.
//!
//! ```no_run
//! # // This is a conceptual example, refer to live_dash.rs for a runnable one.
//! # async fn run() -> anyhow::Result<()> {
//! # use iori::HttpClient;
//! # use iori::dash::live::LiveDashSource;
//! # use url::Url;
//! # use std::sync::Arc;
//! # use iori::error::IoriError;
//! #
//! # let client = HttpClient::default();
//! # let mpd_url = Url::parse("https://example.com/live/manifest.mpd")?;
//! # let key = None; // Option<Arc<iori::decrypt::IoriKey>>
//! # let shaka_packager_command = None; // Option<std::path::PathBuf>
//! // Default representation selector: selects the representation with the maximum bandwidth.
//! # let representation_selector = Arc::new(|representations: &[dash_mpd::Representation]| {
//! #    representations
//! #        .iter()
//! #        .max_by_key(|r| r.bandwidth.unwrap_or(0))
//! #        .cloned()
//! #        .ok_or_else(|| IoriError::NoRepresentationFound)
//! # });
//! #
//! let live_source = LiveDashSource::new(
//!     client,
//!     mpd_url,
//!     key,
//!     shaka_packager_command,
//!     Some(representation_selector),
//! );
//! #
//! # // ... setup downloader and process segments ...
//! # Ok(())
//! # }
//! ```
//!
//! The `dash` module also includes common structures like [`segment::DashSegment`] used by both
//! archive and live sources.

pub mod archive;
pub mod live;
pub mod segment;
pub mod template;
