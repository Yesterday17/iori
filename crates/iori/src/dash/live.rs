mod clock;

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration as StdDuration,
};

use super::url::merge_baseurls;
use crate::{
    dash::{segment::DashSegment, template::Template},
    decrypt::IoriKey,
    error::{IoriError, IoriResult},
    fetch::fetch_segment as fetch_segment_global,
    util::http::HttpClient,
    InitialSegment, SegmentType, StreamingSource,
};
use chrono::{DateTime, TimeDelta, Utc};
use clock::Clock;
use dash_mpd::{AdaptationSet, Period, Representation, MPD};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use url::Url;

// Representation selector function type
pub type RepresentationSelector =
    Arc<dyn Fn(&[Representation]) -> IoriResult<Representation> + Send + Sync>;

// LiveDashSource itself is not Clone anymore in the traditional sense for deep copies.
// It will be wrapped in Arc for shared ownership.
// Individual fields needing mutation from multiple tasks get their own Arc<Mutex<>>.
pub struct LiveDashSource {
    client: HttpClient,
    mpd_url: Url,
    key: Option<Arc<IoriKey>>,

    // Mutable state shared between fetch_info's initial setup and the background task
    clock: Arc<Mutex<Clock>>,
    mpd_data: Arc<Mutex<Option<MPD>>>, // Option<MPD> because it's fetched
    active_period_id: Arc<Mutex<Option<String>>>,
    last_mpd_update: Arc<Mutex<Option<DateTime<Utc>>>>,
    minimum_update_period: Arc<Mutex<Option<StdDuration>>>,

    // State primarily managed by the "owner" of the segment generation logic
    // (either initial fetch_info or the background task's instance of LiveDashSourceInternalState)
    // For simplicity here, current_segments will be part of an internal struct or managed by the task locally.
    // The main `LiveDashSource` won't directly hold `current_segments` to avoid complex locking for it.
    // Instead, `update_segments` will return the full list, and the caller (task) decides what's new.
    sequence_counter: Arc<AtomicU64>,
    shaka_packager_command: Option<PathBuf>,
    representation_selector: RepresentationSelector,
}

impl LiveDashSource {
    pub fn new(
        client: HttpClient,
        mpd_url: Url,
        key: Option<Arc<IoriKey>>,
        shaka_packager_command: Option<PathBuf>,
        representation_selector: Option<RepresentationSelector>,
    ) -> Self {
        let clock = Arc::new(Mutex::new(Clock::new()));

        let selector = representation_selector.unwrap_or_else(|| {
            Arc::new(|representations: &[Representation]| {
                representations
                    .iter()
                    .filter(|r| r.id.is_some())
                    .max_by_key(|r| r.bandwidth.unwrap_or(0))
                    .cloned()
                    .ok_or_else(|| IoriError::NoRepresentationFound)
            })
        });

        Self {
            client,
            mpd_url,
            key,
            clock,
            mpd_data: Arc::new(Mutex::new(None)),
            active_period_id: Arc::new(Mutex::new(None)),
            last_mpd_update: Arc::new(Mutex::new(None)),
            minimum_update_period: Arc::new(Mutex::new(None)),
            sequence_counter: Arc::new(AtomicU64::new(0)),
            shaka_packager_command,
            representation_selector: selector,
        }
    }

    // Helper to get a snapshot of the MPD for URL resolution.
    // It takes a locked MPD guard to ensure consistency.
    fn get_base_url_for_representation(
        &self,
        mpd: &MPD, // Pass the locked MPD data
        period: &Period,
        adaption_set: &AdaptationSet,
        representation: &Representation,
    ) -> IoriResult<Url> {
        let mut current_url = self.mpd_url.clone(); // Start with original MPD URL as ultimate base

        // MPD level BaseURLs
        for base_obj in &mpd.base_url {
            current_url = merge_baseurls(&current_url, &base_obj.base)?;
        }
        // Period level BaseURLs
        for base_obj in &period.BaseURL {
            current_url = merge_baseurls(&current_url, &base_obj.base)?;
        }
        // AdaptationSet level BaseURLs
        for base_obj in &adaption_set.BaseURL {
            current_url = merge_baseurls(&current_url, &base_obj.base)?;
        }
        // Representation level BaseURLs
        for base_obj in &representation.BaseURL {
            current_url = merge_baseurls(&current_url, &base_obj.base)?;
        }
        Ok(current_url)
    }

    // update_segments now takes &self and operates on locked data.
    // It returns the full list of currently available segments.
    // The caller (background task) will compare with its previously sent list if diffing is needed.
    async fn update_segments(&self) -> IoriResult<Option<Vec<DashSegment>>> {
        // Lock necessary shared data
        let mpd_guard = self.mpd_data.lock().await;
        let clock_guard = self.clock.lock().await;
        let mut active_period_id_guard = self.active_period_id.lock().await;

        let mpd = match mpd_guard.as_ref() {
            Some(m) => m,
            None => {
                // This case should ideally be handled by the caller ensuring MPD is loaded first.
                // If called without MPD, it implies an issue or initial state not yet set.
                tracing::warn!(
                    "update_segments called but MPD data is None. Returning no segments."
                );
                return Ok(None);
            }
        };

        if mpd.mpdtype.as_deref() != Some("dynamic") {
            tracing::info!("MPD type is not 'dynamic'. Treating as static for segment generation.");
        }

        let now_synced = clock_guard.now(); // Use locked clock
        tracing::debug!(current_time = %now_synced, "Updating segments based on current time");

        let current_active_period_id_opt = active_period_id_guard.clone();
        let period = mpd
            .periods
            .iter()
            .find(|p| {
                current_active_period_id_opt.is_none()
                    || p.id.as_ref() == current_active_period_id_opt.as_ref()
            })
            // TODO: get best quality instead of first value
            .or_else(|| mpd.periods.get(0))
            .ok_or(IoriError::NoPeriodFound)?;

        // Update active_period_id if it changed (e.g. first time or period transition)
        if active_period_id_guard.as_ref() != period.id.as_ref() {
            *active_period_id_guard = period.id.clone();
        }
        // Drop guard soon as it's not needed for the rest of the computation if period is cloned or data extracted
        // However, period itself is borrowed from mpd_guard, so mpd_guard must live long enough.

        let adaptation_set = period
            .adaptations
            .iter()
            .find(|adapt| {
                let content_type = adapt.contentType.as_deref().unwrap_or_default();
                let mime_type = adapt.mimeType.as_deref().unwrap_or_default();
                content_type.starts_with("video")
                    || mime_type.starts_with("video")
                    || content_type.starts_with("audio")
                    || mime_type.starts_with("audio")
            })
            .ok_or(IoriError::NoAdaptationSetFound)?;

        let representation = (*self.representation_selector)(&adaptation_set.representations)?;
        tracing::debug!(representation_id = ?representation.id, bandwidth=representation.bandwidth, "Selected representation");

        // Pass the locked mpd_guard to get_base_url_for_representation
        let representation_base_url =
            self.get_base_url_for_representation(&mpd, period, adaptation_set, &representation)?;
        tracing::debug!(%representation_base_url, "Base URL for representation");

        // This local variable is used to compare against previously generated segments if we were doing diffing here.
        // However, the task is simplified to return the full list, and the caller (background task) handles what's "new".
        let mut new_segments = Vec::new();

        // Dropping mpd_guard and active_period_id_guard as their data is captured in local variables (mpd, period)
        // or updated and no longer needed for this specific segment generation pass.
        // This depends on whether `period` and `representation` are clones or still borrowing from `mpd_guard`.
        // Dash-mpd types are typically `Clone`. Let's assume `period` and `representation` are effectively owned here or their relevant data is copied.
        // To be safe, mpd_guard should live as long as period, adaptation_set, representation are used if they borrow.
        // Let's assume they are cloned or data extracted.
        // For this pass, the critical part is that `mpd` (the reference to the MPD inside the guard) is valid.

        // The actual segment generation logic (SegmentTemplate, SegmentList) remains largely the same as the previous version,
        // but it now operates with `mpd` (deref of `mpd_guard`) and `now_synced` (from `clock_guard.now()`).
        // The key change is that this function is now called by a task that manages the LiveDashSource state.

        // --- Start of segment generation logic (adapted from previous version) ---
        if let Some(segment_template) = representation
            .SegmentTemplate
            .as_ref()
            .or(adaptation_set.SegmentTemplate.as_ref())
        {
            let initialization_from_template = if let Some(init_template_str) =
                &segment_template.initialization
            {
                let mut init_vars = Template::new();
                init_vars.insert(
                    Template::REPRESENTATION_ID,
                    representation.id.clone().unwrap_or_default(),
                );
                init_vars.insert(
                    Template::BANDWIDTH,
                    representation.bandwidth.unwrap_or(0).to_string(),
                );
                let init_relative_url = init_vars.resolve(init_template_str);
                let init_url = representation_base_url.join(&init_relative_url)?;
                tracing::debug!(init_url = %init_url, "Resolved initialization URL from SegmentTemplate");
                Some(init_url)
            } else {
                None
            };

            if let Some(timeline) = &segment_template.SegmentTimeline {
                tracing::debug!(
                    "Processing SegmentTimeline for Rep ID: {:?}",
                    representation.id
                );
                let timescale = segment_template.timescale.unwrap_or(1);
                let mut current_presentation_time_pts =
                    timeline.segments.get(0).and_then(|s| s.t).unwrap_or(0);
                let mut current_segment_number = segment_template.startNumber.unwrap_or(1);

                let availability_start_time_mpd = mpd
                    .availabilityStartTime
                    .unwrap_or_else(|| Utc::now() - TimeDelta::days(7));
                let period_start_std_duration = period.start.unwrap_or(StdDuration::ZERO);
                let period_start_offset_from_availability =
                    TimeDelta::from_std(period_start_std_duration)
                        .inspect_err(|e| log::error!("Invalid period start duration: {e}"))?;
                let absolute_period_start_time =
                    availability_start_time_mpd + period_start_offset_from_availability;

                let suggested_delay_std = mpd
                    .suggestedPresentationDelay
                    .unwrap_or(StdDuration::from_secs(0));
                let time_shift_buffer_depth_std = mpd
                    .timeShiftBufferDepth
                    .unwrap_or_else(|| StdDuration::from_secs(3600 * 24 * 7));

                let live_edge_presentation_time =
                    now_synced - TimeDelta::from_std(suggested_delay_std)?;
                let earliest_available_segment_start_time =
                    live_edge_presentation_time - TimeDelta::from_std(time_shift_buffer_depth_std)?;

                for s_element in &timeline.segments {
                    if let Some(t) = s_element.t {
                        current_presentation_time_pts = t;
                    }
                    let duration_pts = s_element.d;
                    let repeat_count = s_element.r.unwrap_or(0);

                    for _ in 0..=repeat_count {
                        let segment_abs_start_time = absolute_period_start_time
                            + TimeDelta::from_std(StdDuration::from_secs_f64(
                                current_presentation_time_pts as f64 / timescale as f64,
                            ))?;
                        let segment_duration_sec = duration_pts as f64 / timescale as f64;
                        let segment_abs_end_time = segment_abs_start_time
                            + TimeDelta::from_std(StdDuration::from_secs_f64(
                                segment_duration_sec,
                            ))?;

                        if segment_abs_end_time <= earliest_available_segment_start_time
                            && mpd.timeShiftBufferDepth.is_some()
                        {
                            current_presentation_time_pts += duration_pts;
                            current_segment_number += 1;
                            continue;
                        }

                        if segment_abs_start_time
                            >= live_edge_presentation_time + TimeDelta::seconds(10)
                        {
                            current_presentation_time_pts += duration_pts;
                            current_segment_number += 1;
                            continue;
                        }

                        let mut template_vars = Template::new();
                        template_vars.insert(
                            Template::REPRESENTATION_ID,
                            representation.id.clone().unwrap_or_default(),
                        );
                        template_vars.insert(Template::NUMBER, current_segment_number.to_string());
                        template_vars.insert(
                            Template::BANDWIDTH,
                            representation.bandwidth.unwrap_or(0).to_string(),
                        );
                        template_vars
                            .insert(Template::TIME, current_presentation_time_pts.to_string());

                        let media_url_template =
                            segment_template.media.as_ref().ok_or_else(|| {
                                IoriError::MpdParsing(
                                    "SegmentTemplate media attribute missing for SegmentTimeline"
                                        .into(),
                                )
                            })?;
                        let relative_url = template_vars.resolve(media_url_template);
                        let segment_url = representation_base_url.join(&relative_url)?;
                        let segment_filename = relative_url
                            .split('/')
                            .last()
                            .unwrap_or(&format!("seg_{}.m4s", current_segment_number))
                            .to_string();

                        new_segments.push(DashSegment {
                            url: segment_url,
                            filename: segment_filename,
                            r#type: SegmentType::from_mime_type(
                                representation
                                    .mimeType
                                    .as_deref()
                                    .or(adaptation_set.mimeType.as_deref()),
                            ),
                            initial_segment: initialization_from_template
                                .as_ref()
                                .map_or(InitialSegment::None, |_| InitialSegment::None),
                            key: self.key.clone(),
                            byte_range: None,
                            sequence: self.sequence_counter.fetch_add(1, Ordering::Relaxed),
                            number: Some(current_segment_number),
                            time: Some(current_presentation_time_pts),
                        });

                        current_presentation_time_pts += duration_pts;
                        current_segment_number += 1;
                        if new_segments.len() >= 1000 {
                            break;
                        }
                    }
                    if new_segments.len() >= 1000 {
                        break;
                    }
                }
            } else if segment_template.media.is_some() {
                // SegmentTemplate @duration case
                let timescale = segment_template.timescale.unwrap_or(1);
                let duration_pts = segment_template.duration.ok_or(IoriError::MpdParsing(
                    "SegmentTemplate missing duration for @duration case".into(),
                ))?;
                let duration_sec = duration_pts as f64 / timescale as f64;
                if duration_sec <= 0.0 {
                    return Err(IoriError::MpdParsing(
                        "SegmentTemplate duration must be positive".into(),
                    ));
                }
                let start_number = segment_template.startNumber.unwrap_or(1);

                let availability_start_time_mpd = mpd
                    .availabilityStartTime
                    .unwrap_or_else(|| Utc::now() - TimeDelta::days(7));
                let period_start_std_duration = period.start.unwrap_or(StdDuration::ZERO);
                let period_start_offset_from_availability =
                    TimeDelta::from_std(period_start_std_duration)
                        .inspect_err(|e| tracing::error!("Invalid period start duration: {e}"))?;
                let absolute_period_start_time =
                    availability_start_time_mpd + period_start_offset_from_availability;

                let suggested_delay_std = mpd
                    .suggestedPresentationDelay
                    .unwrap_or(StdDuration::from_secs(0));
                let time_shift_buffer_depth_std = mpd
                    .timeShiftBufferDepth
                    .unwrap_or_else(|| StdDuration::from_secs(3600 * 24 * 7));

                let live_edge_presentation_time =
                    now_synced - TimeDelta::from_std(suggested_delay_std)?;
                let earliest_available_segment_start_time =
                    live_edge_presentation_time - TimeDelta::from_std(time_shift_buffer_depth_std)?;

                let mut current_segment_number =
                    if earliest_available_segment_start_time > absolute_period_start_time {
                        ((earliest_available_segment_start_time - absolute_period_start_time)
                            .num_milliseconds() as f64
                            / 1000.0
                            / duration_sec)
                            .floor() as u64
                            + start_number
                    } else {
                        start_number
                    }
                    .max(start_number);

                loop {
                    let segment_rel_start_pts =
                        ((current_segment_number - start_number) as f64 * duration_pts) as u64;
                    let segment_presentation_time = absolute_period_start_time
                        + TimeDelta::from_std(StdDuration::from_secs_f64(
                            segment_rel_start_pts as f64 / timescale as f64,
                        ))?;

                    if segment_presentation_time
                        > live_edge_presentation_time + TimeDelta::seconds(10)
                    {
                        break;
                    }
                    if segment_presentation_time < earliest_available_segment_start_time
                        && mpd.timeShiftBufferDepth.is_some()
                    {
                        current_segment_number += 1;
                        continue;
                    }

                    let mut template_vars = Template::new();
                    template_vars.insert(
                        Template::REPRESENTATION_ID,
                        representation.id.clone().unwrap_or_default(),
                    );
                    template_vars.insert(Template::NUMBER, current_segment_number.to_string());
                    template_vars.insert(
                        Template::BANDWIDTH,
                        representation.bandwidth.unwrap_or(0).to_string(),
                    );
                    template_vars.insert(Template::TIME, segment_rel_start_pts.to_string());

                    let media_url_template = segment_template.media.as_ref().unwrap();
                    let relative_url = template_vars.resolve(media_url_template);
                    let segment_url = representation_base_url.join(&relative_url)?;
                    let segment_filename = relative_url
                        .split('/')
                        .last()
                        .unwrap_or(&format!(
                            "{}_{}.m4s",
                            representation.id.as_deref().unwrap_or("s"),
                            current_segment_number
                        ))
                        .to_string();

                    new_segments.push(DashSegment {
                        url: segment_url,
                        filename: segment_filename,
                        r#type: SegmentType::from_mime_type(
                            representation
                                .mimeType
                                .as_deref()
                                .or(adaptation_set.mimeType.as_deref()),
                        ),
                        initial_segment: initialization_from_template
                            .as_ref()
                            .map_or(InitialSegment::None, |_| InitialSegment::None),
                        key: self.key.clone(),
                        byte_range: None,
                        sequence: self.sequence_counter.fetch_add(1, Ordering::Relaxed),
                        number: Some(current_segment_number),
                        time: Some(segment_rel_start_pts),
                    });
                    current_segment_number += 1;
                    if new_segments.len() >= 1000 {
                        break;
                    }
                    if let Some(pds) = period.duration {
                        if TimeDelta::from_std(pds)?
                            < (segment_presentation_time - absolute_period_start_time)
                        {
                            break;
                        }
                    }
                }
            } else {
                return Err(IoriError::MpdParsing(
                    "SegmentTemplate present but lacks media URL or SegmentTimeline.".into(),
                ));
            }
        } else if let Some(segment_list) = representation
            .SegmentList
            .as_ref()
            .or(adaptation_set.SegmentList.as_ref())
        {
            tracing::debug!("Processing SegmentList for Rep ID: {:?}", representation.id);
            let list_timescale = segment_list.timescale.unwrap_or(1);
            let list_initialization_resolved_url =
                if let Some(init_el) = &segment_list.Initialization {
                    if let Some(s_url) = &init_el.sourceURL {
                        Some(representation_base_url.join(s_url)?)
                    } else {
                        None
                    }
                } else {
                    None
                };

            let mut current_presentation_time_pts = 0_u64;
            let availability_start_time_mpd = mpd
                .availabilityStartTime
                .unwrap_or_else(|| Utc::now() - TimeDelta::days(7));
            let period_start_std_duration = period.start.unwrap_or(StdDuration::ZERO);
            let period_start_offset_from_availability =
                TimeDelta::from_std(period_start_std_duration)
                    .inspect_err(|e| tracing::error!("Invalid period start duration: {e}"))?;
            let absolute_period_start_time =
                availability_start_time_mpd + period_start_offset_from_availability;
            let suggested_delay_std = mpd
                .suggestedPresentationDelay
                .unwrap_or(StdDuration::from_secs(0));
            let time_shift_buffer_depth_std = mpd
                .timeShiftBufferDepth
                .unwrap_or_else(|| StdDuration::from_secs(3600 * 24 * 7));
            let live_edge_presentation_time =
                now_synced - TimeDelta::from_std(suggested_delay_std)?;
            let earliest_available_segment_start_time =
                live_edge_presentation_time - TimeDelta::from_std(time_shift_buffer_depth_std)?;

            for (idx, segment_url_el) in segment_list.segment_urls.iter().enumerate() {
                let media_uri = segment_url_el.media.as_ref().ok_or_else(|| {
                    IoriError::MpdParsing(format!("SegmentURL @index {} missing @media", idx))
                })?;
                let segment_absolute_url = representation_base_url.join(media_uri)?;
                let segment_filename = media_uri
                    .split(|c| c == '/' || c == '?')
                    .last()
                    .unwrap_or(&format!(
                        "sl_{}_{}.m4s",
                        representation.id.as_deref().unwrap_or("r"),
                        idx
                    ))
                    .to_string();

                if let Some(list_segment_duration_pts) = segment_list.duration {
                    let seg_dur_pts_u64 = list_segment_duration_pts;
                    let seg_start_abs = absolute_period_start_time
                        + TimeDelta::from_std(StdDuration::from_secs_f64(
                            current_presentation_time_pts as f64 / list_timescale as f64,
                        ))?;
                    let seg_dur_sec = seg_dur_pts_u64 as f64 / list_timescale as f64;
                    let seg_end_abs = seg_start_abs
                        + TimeDelta::from_std(StdDuration::from_secs_f64(seg_dur_sec))?;

                    if seg_end_abs <= earliest_available_segment_start_time
                        && mpd.timeShiftBufferDepth.is_some()
                    {
                        current_presentation_time_pts += seg_dur_pts_u64;
                        continue;
                    }
                    if seg_start_abs >= live_edge_presentation_time + TimeDelta::seconds(10)
                        && mpd.mpdtype.as_deref() == Some("dynamic")
                    {
                        current_presentation_time_pts += seg_dur_pts_u64;
                        continue;
                    }
                }

                new_segments.push(DashSegment {
                    url: segment_absolute_url,
                    filename: segment_filename,
                    r#type: SegmentType::from_mime_type(
                        representation
                            .mimeType
                            .as_deref()
                            .or(adaptation_set.mimeType.as_deref()),
                    ),
                    initial_segment: list_initialization_resolved_url
                        .as_ref()
                        .map_or(InitialSegment::None, |_| InitialSegment::None),
                    key: self.key.clone(),
                    byte_range: segment_url_el.mediaRange.clone(),
                    sequence: self.sequence_counter.fetch_add(1, Ordering::Relaxed),
                    number: None,
                    time: None,
                });

                if let Some(list_dur_pts) = segment_list.duration {
                    current_presentation_time_pts += list_dur_pts;
                }
                if new_segments.len() >= 2000 {
                    break;
                }
            }
        } else {
            return Err(IoriError::MpdParsing(
                "No SegmentTemplate or SegmentList found for selected representation.".into(),
            ));
        }
        // --- End of segment generation logic ---

        // Instead of storing current_segments in self and diffing, update_segments now returns the full list.
        // The caller (background task) will be responsible for diffing or deciding to send.
        // This simplifies locking within update_segments.
        if new_segments.is_empty() {
            Ok(None)
        } else {
            Ok(Some(new_segments))
        }
    }
}

// The background task for MPD updates
async fn live_updater_task(
    client: HttpClient,
    source: LiveDashSource,
    sender: mpsc::UnboundedSender<IoriResult<Vec<DashSegment>>>,
    initial_mpd_type: Option<String>,
    initial_min_update_period: Option<StdDuration>,
) {
    let mut mpd_type = initial_mpd_type;
    let mut min_update_period_opt = initial_min_update_period;
    let mut last_sent_segments_snapshot: Option<DashSegment> = None; // To compare and send only new lists

    loop {
        let sleep_duration = match min_update_period_opt {
            Some(p) if p > StdDuration::ZERO => p,
            _ => {
                // Default if not present or zero, or if MPD type is static
                if mpd_type.as_deref() != Some("dynamic") {
                    tracing::info!(
                        "MPD is static or no minimumUpdatePeriod, updater task finishing."
                    );
                    break;
                }
                tracing::warn!("minimumUpdatePeriod not specified or invalid for dynamic MPD, using 60s default.");
                StdDuration::from_secs(2)
            }
        };

        tracing::debug!("Updater task sleeping for {:?}", sleep_duration);
        tokio::time::sleep(sleep_duration).await;

        // 1. Fetch new MPD
        tracing::debug!("Updater task: Fetching new MPD from {}", source.mpd_url);
        let mpd_text = match source.client.get(source.mpd_url.clone()).send().await {
            Ok(resp) => match resp.text().await {
                Ok(text) => text,
                Err(e) => {
                    tracing::error!("Failed to get text from MPD response: {}", e);
                    // Retry after sleep_duration or a shorter error backoff? For now, continue loop.
                    continue;
                }
            },
            Err(e) => {
                tracing::error!("Failed to fetch MPD: {}", e);
                continue;
            }
        };

        // 2. Parse MPD
        let new_mpd = match dash_mpd::parse(&mpd_text) {
            Ok(mpd) => mpd,
            Err(e) => {
                tracing::error!("Failed to parse new MPD: {}", e);
                continue;
            }
        };
        mpd_type = new_mpd.mpdtype.clone();

        // 3. Lock and update shared state
        {
            // Scoped lock for shared data
            let mut mpd_data_guard = source.mpd_data.lock().await;
            let mut clock_guard = source.clock.lock().await;
            let mut last_mpd_update_guard = source.last_mpd_update.lock().await;
            let mut min_update_period_guard = source.minimum_update_period.lock().await;

            if let Err(e) = clock_guard.sync(&new_mpd, client.clone()).await {
                // Sync clock with new MPD
                tracing::warn!("Failed to re-sync clock with new MPD: {}", e);
                // Continue with old clock sync, or use new MPD time if available? For now, just log.
            }

            *mpd_data_guard = Some(new_mpd.clone()); // new_mpd is cloned here for the guard
            *last_mpd_update_guard = Some(clock_guard.now());
            min_update_period_opt = new_mpd.minimumUpdatePeriod;
            *min_update_period_guard = min_update_period_opt;
        } // Locks are released here

        // 4. Update segments
        match source.update_segments().await {
            Ok(Some(mut segments)) => {
                if !segments.is_empty() {
                    let send_update = match &last_sent_segments_snapshot {
                        Some(last_sent) => {
                            segments = segments
                                .into_iter()
                                .filter(|seg| {
                                    if let (Some(last_number), Some(number)) =
                                        (last_sent.number, seg.number)
                                    {
                                        number > last_number
                                    } else {
                                        seg.sequence > last_sent.sequence
                                    }
                                })
                                .collect();
                            !segments.is_empty()
                        }
                        None => true, // First time sending segments after initial
                    };

                    if send_update {
                        last_sent_segments_snapshot = segments.last().cloned();
                        if sender.send(Ok(segments)).is_err() {
                            tracing::warn!("Receiver dropped, updater task finishing.");
                            break; // Exit loop if channel closed
                        }
                    } else {
                        tracing::debug!("Segment list unchanged after MPD update.");
                    }
                } else {
                    tracing::debug!("MPD updated, but no new segments found or list became empty.");
                    // If list becomes empty, we might want to send an empty vec to signal that.
                    // However, current check `!segments.is_empty()` prevents this.
                    // Consider if sending empty lists is meaningful.
                }
            }
            Ok(None) => {
                tracing::debug!(
                    "MPD updated, but update_segments returned None (no change or no segments)."
                );
            }
            Err(e) => {
                tracing::error!("Error updating segments after MPD refresh: {}", e);
                if sender.send(Err(e)).is_err() {
                    // Propagate error
                    tracing::warn!("Receiver dropped while sending error, updater task finishing.");
                    break;
                }
            }
        }

        // 5. Check if stream ended
        if new_mpd.mpdtype.as_deref() == Some("static") {
            tracing::info!(
                "MPD type changed to 'static', assuming live stream ended. Updater task finishing."
            );
            break; // Close sender implicitly by dropping it when task ends.
        }
    }
    tracing::info!("Live MPD updater task has finished.");
}

impl StreamingSource for LiveDashSource {
    type Segment = DashSegment;

    async fn fetch_info(
        &self,
    ) -> IoriResult<mpsc::UnboundedReceiver<IoriResult<Vec<Self::Segment>>>> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let initial_mpd_text = self
            .client
            .get(self.mpd_url.clone())
            .send()
            .await?
            .text()
            .await?;

        let initial_mpd = dash_mpd::parse(&initial_mpd_text)?;

        let initial_mpd_type = initial_mpd.mpdtype.clone();
        let initial_min_update_period = initial_mpd.minimumUpdatePeriod;

        // Lock and update shared state for the first time
        {
            let mut mpd_data_guard = self.mpd_data.lock().await;
            let mut clock_guard = self.clock.lock().await;
            let mut last_mpd_update_guard = self.last_mpd_update.lock().await;
            let mut min_update_period_guard = self.minimum_update_period.lock().await;

            clock_guard.sync(&initial_mpd, self.client.clone()).await?; // Initial clock sync
            *mpd_data_guard = Some(initial_mpd); // Store initial MPD
            *last_mpd_update_guard = Some(clock_guard.now());
            *min_update_period_guard = initial_min_update_period;
        }

        // --- Spawn Background Task for Dynamic MPDs ---
        if initial_mpd_type.as_deref() == Some("dynamic") && initial_min_update_period.is_some() {
            // Clone Arcs for the spawned task. The LiveDashSource itself is not Arc<Mutex<Self>>.
            // Instead, its fields that need sharing are Arc<Mutex<FieldType>>.
            // So we clone these Arcs.
            let task_source = LiveDashSource {
                client: self.client.clone(),
                mpd_url: self.mpd_url.clone(),
                key: self.key.clone(),
                clock: Arc::clone(&self.clock),
                mpd_data: Arc::clone(&self.mpd_data),
                active_period_id: Arc::clone(&self.active_period_id),
                last_mpd_update: Arc::clone(&self.last_mpd_update),
                minimum_update_period: Arc::clone(&self.minimum_update_period),
                sequence_counter: Arc::clone(&self.sequence_counter),
                shaka_packager_command: self.shaka_packager_command.clone(),
                representation_selector: self.representation_selector.clone(),
            };
            let task_sender = sender; // Give sender ownership to the task

            tokio::spawn(live_updater_task(
                self.client.clone(),
                task_source,
                task_sender,
                initial_mpd_type,
                initial_min_update_period,
            ));
        } else {
            tracing::debug!("MPD is not dynamic or lacks minimumUpdatePeriod; no background updater task will be spawned.");
            // Sender is dropped here if not moved to task, closing the channel after initial segments.
        }

        Ok(receiver)
    }

    async fn fetch_segment<W>(&self, segment: &Self::Segment, writer: &mut W) -> IoriResult<()>
    where
        W: tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        fetch_segment_global(
            self.client.clone(),
            segment,
            writer,
            self.shaka_packager_command.clone(),
        )
        .await
    }
}
