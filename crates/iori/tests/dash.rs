use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration as StdDuration; // Renamed to avoid conflict with chrono::Duration

use bytes::Bytes;
use chrono::{DateTime, TimeDelta, Utc}; // Added chrono types
// For parsing MPD in tests if needed, though mostly serving strings. 
// Not strictly required if MPD XML is manually crafted and valid.
// use dash_mpd::MPD; 
use iori::cache::{Cache}; 
use iori::dash::live::{LiveDashSource, RepresentationSelector};
use iori::dash::segment::DashSegment;
// use iori::decrypt::IoriKey; // Not used in the first basic test
use iori::download::{Downloader, SequencialDownloader};
use iori::error::{IoriError, IoriResult};
use iori::merge::{Merger, SkipMerger};
use iori::{HttpClient, InitialSegment, SegmentType, StreamingSegment}; 
use tokio::sync::Mutex;
use url::Url;
use wiremock::matchers::{method, path}; 
use wiremock::{Mock, MockServer, ResponseTemplate};
// use futures::TryStreamExt; // For processing the segment stream from fetch_info, used in later tests


// Helper: In-memory cache to collect segment data for assertions
#[derive(Debug, Clone, Default)]
struct VecCacheSource {
    segments: Arc<Mutex<HashMap<String, Bytes>>>,
    init_segments: Arc<Mutex<HashMap<String, Bytes>>>, // To distinguish init segments
}

impl VecCacheSource {
    fn new() -> Self {
        Self::default()
    }

    async fn get_segment_data(&self, filename: &str) -> Option<Bytes> {
        self.segments.lock().await.get(filename).cloned()
    }
    
    #[allow(dead_code)] // May not be used in all tests initially
    async fn get_init_segment_data(&self, filename: &str) -> Option<Bytes> {
        self.init_segments.lock().await.get(filename).cloned()
    }


    async fn total_media_segments_count(&self) -> usize {
        self.segments.lock().await.len()
    }
    
    #[allow(dead_code)]
    async fn get_all_media_segment_names(&self) -> Vec<String> {
        self.segments.lock().await.keys().cloned().collect()
    }
}

#[async_trait::async_trait]
impl Cache for VecCacheSource {
    async fn has(&self, file_name: &str) -> bool {
        self.segments.lock().await.contains_key(file_name) 
            || self.init_segments.lock().await.contains_key(file_name)
    }

    async fn save<P: AsRef<Path> + Send>(
        &self,
        _temp_path: P, // Not used as we save bytes directly
        _file_name: &str,
    ) -> IoriResult<()> {
        Ok(())
    }
    
    async fn save_from_bytes(&self, data: Bytes, file_name: &str, _segment_type: Option<SegmentType>) -> IoriResult<PathBuf> {
        // Simplified: assume "init" in name means init segment for this test cache.
        if file_name.to_lowercase().contains("init") {
             self.init_segments.lock().await.insert(file_name.to_string(), data);
        } else {
            self.segments.lock().await.insert(file_name.to_string(), data);
        }
        Ok(PathBuf::from(file_name)) 
    }

    async fn get_path(&self, file_name: &str) -> Option<PathBuf> {
        if self.segments.lock().await.contains_key(file_name) ||  self.init_segments.lock().await.contains_key(file_name) {
            Some(PathBuf::from(file_name)) 
        } else {
            None
        }
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        None 
    }
}


// Helper to initialize tracing for tests
fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("iori=trace,wiremock=trace") // More verbose for tests
        .try_init();
}

// Default RepresentationSelector for tests (max bandwidth)
fn max_bandwidth_selector() -> RepresentationSelector {
    Arc::new(|representations| {
        representations
            .iter()
            .filter(|r| r.id.is_some()) // Ensure representation has an ID for caching logic
            .max_by_key(|r| r.bandwidth.unwrap_or(0))
            .cloned()
            .ok_or_else(|| IoriError::NoRepresentationFound)
    })
}

fn generate_mpd_body(
    base_url: &str, 
    time_server_uri: &str, 
    availability_start_time_str: &str,
    minimum_update_period_sec: u64,
    segment_duration_sec: u64,
    start_number: u64,
    mpd_type: &str, // "dynamic" or "static"
    init_segment_name: &str,
    media_segment_pattern: &str, // e.g., "segment_$Number$.m4s"
    timescale: u64
) -> String {
    format!(
        r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="{}" availabilityStartTime="{}" minimumUpdatePeriod="PT{}S" timeShiftBufferDepth="PT30S" suggestedPresentationDelay="PT10S">
            <UTCTiming schemeIdUri="urn:mpeg:dash:utc:http-xsdate:2014" value="{}/time.xsdat" />
            <Period start="PT0S">
                <AdaptationSet contentType="video">
                    <Representation id="1" bandwidth="1000000">
                        <BaseURL>{}/</BaseURL>
                        <SegmentTemplate media="{}" initialization="{}" timescale="{}" duration="{}" startNumber="{}" />
                    </Representation>
                </AdaptationSet>
            </Period>
        </MPD>"#,
        mpd_type,
        availability_start_time_str,
        minimum_update_period_sec,
        time_server_uri,
        base_url,
        media_segment_pattern,
        init_segment_name,
        timescale,
        segment_duration_sec * timescale, // duration in timescale units
        start_number
    )
}


#[tokio::test]
async fn test_basic_live_stream_segment_template_duration() -> anyhow::Result<()> {
    init_test_tracing();
    let mock_server = MockServer::start().await;
    let time_server = MockServer::start().await;

    let now_chrono = Utc::now();
    // MPD indicates availability starts 60s in the past.
    // Segments are 2s long.
    // timeShiftBufferDepth="PT30S" (segments available for 30s before live edge)
    // suggestedPresentationDelay="PT10S" (live edge is 10s behind current time)
    let availability_start_time = now_chrono - TimeDelta::seconds(60); 
    let availability_start_time_str = availability_start_time.to_rfc3339();
    
    Mock::given(method("GET"))
        .and(path("/time.xsdat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(now_chrono.to_rfc3339()))
        .mount(&time_server)
        .await;

    let mpd_body = generate_mpd_body(
        &mock_server.uri(), 
        &time_server.uri(), 
        &availability_start_time_str, 
        5,  // minimumUpdatePeriod (long enough to not interfere with initial fetch)
        2,  // segmentDuration
        1,  // startNumber
        "dynamic",
        "init.mp4",
        "segment_$Number$.m4s",
        1 // timescale
    );

    Mock::given(method("GET"))
        .and(path("/live.mpd"))
        .respond_with(ResponseTemplate::new(200).set_body_string(mpd_body.clone()))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/init.mp4"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"init_data".to_vec()))
        .mount(&mock_server)
        .await;

    // Calculation for expected segments:
    // PeriodStart_abs = availability_start_time = now_chrono - 60s
    // LiveEdge_pres = now_synced (approx now_chrono) - suggestedPresentationDelay (10s) = now_chrono - 10s
    // EarliestAvailable_pres = LiveEdge_pres - timeShiftBufferDepth (30s) = now_chrono - 40s
    // Segment N (startNumber=1) has presentation start time: PeriodStart_abs + (N-1)*duration_sec
    //                                                     = (now_chrono - 60s) + (N-1)*2s
    // We need segments where:
    //   segment_start_time <= LiveEdge_pres + safety_buffer (e.g. 10s in LiveDashSource)
    //   AND segment_end_time >= EarliestAvailable_pres
    //
    //   (now_chrono - 60s) + (N-1)*2s  <= (now_chrono - 10s) + 10s  (using the +10s buffer from LiveDashSource)
    //   (N-1)*2s <= 60s  => N-1 <= 30 => N <= 31
    //
    //   (now_chrono - 60s) + N*2s >= (now_chrono - 40s)
    //   N*2s >= 20s => N >= 10
    //
    // So, segments N=10 to N=31 are expected. (31 - 10 + 1 = 22 segments)
    
    for i in 1..=40 { // Mock enough segments
        Mock::given(method("GET"))
            .and(path(format!("/segment_{}.m4s", i)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(format!("segment_data_{}", i).into_bytes()))
            .mount(&mock_server)
            .await;
    }
    
    let client = HttpClient::default();
    let source = LiveDashSource::new(
        client,
        Url::parse(&format!("{}/live.mpd", mock_server.uri()))?,
        None, 
        None, 
        Some(max_bandwidth_selector()),
    );

    let cache = Arc::new(VecCacheSource::new());
    let merger: Arc<dyn Merger<DashSegment = DashSegment>> = Arc::new(SkipMerger::new());
    
    let downloader = SequencialDownloader::new(Arc::new(source), merger, Arc::clone(&cache));

    // We are not calling download() fully as it might loop.
    // Instead, we'll take from the stream provided by fetch_info.
    // The fetch_info method itself spawns a task that does the initial update_segments.
    let mut segment_receiver = downloader.source().fetch_info().await?;
    
    let mut actual_segments_info = Vec::new();
    if let Some(res) = segment_receiver.recv().await {
        actual_segments_info = res?;
    }
    
    // Close the receiver to allow the updater task (if any) to stop,
    // though for this test, we only care about the initial fetch.
    segment_receiver.close();


    tracing::info!("Initially fetched {} segments.", actual_segments_info.len());

    // Assert based on the calculation: N from 10 to N=31 (inclusive)
    assert_eq!(actual_segments_info.len(), 22, "Expected 22 segments based on live window calculation.");

    let mut segment_numbers_found: Vec<u64> = actual_segments_info.iter()
        .filter_map(|seg_info| {
            seg_info.file_name().replace("segment_", "").replace(".m4s", "").parse::<u64>().ok()
        })
        .collect();
    segment_numbers_found.sort_unstable();
    
    let expected_segment_numbers: Vec<u64> = (10..=31).collect();
    assert_eq!(segment_numbers_found, expected_segment_numbers, "Downloaded segment numbers do not match expected range.");

    // Check that the init segment was downloaded (via LiveDashSource's internal caching)
    // This test doesn't directly check VecCacheSource for init because LiveDashSource handles it.
    // We rely on the segments being processable, which implies init was available to them.
    // A more direct test for init caching would be in Test 6.

    Ok(())
}

// More tests will be added here...
