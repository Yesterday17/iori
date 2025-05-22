/// Implementation of DASH timeline
///
/// References:
/// - [DASH-IF implementation guidelines: restricted timing model](https://dashif.org/Guidelines-TimingModel)
/// - [MPEG-DASH](https://www.mpeg.org/standards/MPEG-DASH/)
/// - https://github.com/nilaoda/N_m3u8DL-RE/blob/ad7136ae64379cb5aae09a6ada2b788c7030c917/src/N_m3u8DL-RE.Parser/Extractor/DASHExtractor2.cs
/// - https://github.com/emarsden/dash-mpd-rs/blob/main/src/fetch.rs
use chrono::{DateTime, Duration, TimeDelta, Utc};
use dash_mpd::{
    AdaptationSet, Period, Representation, SegmentBase, SegmentList, SegmentTemplate, UTCTiming,
    MPD,
};
use url::Url;

use crate::{
    dash::{
        segment::DashSegment,
        template::TemplateUrl,
        url::{is_absolute_url, merge_baseurls},
    },
    HttpClient, IoriError, IoriResult,
};

use super::clock::Clock;

/// https://dashif.org/Guidelines-TimingModel/#mpd-general-timeline
///
/// > The MPD defines the MPD timeline of a DASH presentation, which serves as the baseline
/// > for all scheduling decisions made during playback and establishes the relative timing
/// > of periods and media segments. The MPD timeline informs DASH clients on when it can
/// > download and present which media segments. The contents of an MPD are a promise by a
/// > DASH service to make specific media segments available during specific time spans
/// > described by the MPD timeline.
///
/// > Values on the MPD timeline are all ultimately relative to the zero point of the MPD
/// > timeline, though possibly through several layers of indirection (e.g. period A is
/// > relative to period B, which is relative to the zero point).
///
/// > The following MPD elements are most relevant to locating and scheduling the media samples:
///
/// > 1. The MPD describes consecutive periods which map data onto the MPD timeline.
///
/// > 2. Each period describes of one or more representations, each of which provides media samples
/// > inside a sequence of media segments located via segment references. Representations contain
/// > independent sample timelines that are mapped to the time span on the MPD timeline that belongs
/// > to the period.
///
/// > 3. Representations within a period are grouped into adaptation sets, which associate related
/// > representations and decorate them with metadata.
pub struct MPDTimeline {
    presentation: DashPresentation,
    /// An MPD defines an ordered list of one or more consecutive non-overlapping periods ([DASH] 5.3.2).
    /// A period is both a time span on the MPD timeline and a definition of the data to be presented
    /// during this time span. Period timing is relative to the zero point of the MPD timeline, though
    /// often indirectly (being relative to the previous period).
    periods: Vec<DashPeriod>,
}

impl MPDTimeline {
    pub fn from_mpd(mpd: MPD, mpd_url: Option<&Url>) -> IoriResult<Self> {
        let presentation = DashPresentation::from_mpd(&mpd);

        let mpd_base_url = mpd.base_url.get(0).map(|u| u.base.as_str());
        let base_url = match (mpd_base_url, mpd_url) {
            (Some(mpd_base_url), Some(mpd_url)) => merge_baseurls(&mpd_url, mpd_base_url)?,
            (None, Some(mpd_url)) => mpd_url.clone(),
            (Some(mpd_base_url), None) if is_absolute_url(mpd_base_url) => {
                Url::parse(mpd_base_url)?
            }
            _ => return Err(IoriError::MpdParsing("Invalid base url".to_string())),
        };

        let mut periods: Vec<DashPeriod> = Vec::with_capacity(mpd.periods.len());
        for period in mpd.periods {
            let last_mut = periods.last_mut();
            let period = DashPeriod::from_mpd(&base_url, period, last_mut)?;
            periods.push(period);
        }

        Ok(Self {
            presentation,
            periods,
        })
    }

    pub fn is_static(&self) -> bool {
        matches!(self.presentation, DashPresentation::Static)
    }

    pub fn is_dynamic(&self) -> bool {
        !self.is_static()
    }

    /// Return all segments available in the dash timeline since the given time
    pub fn segments_since(&self, time: Option<DateTime<Utc>>) -> (Vec<DashSegment>, DateTime<Utc>) {
        let now = self.presentation.now();
        todo!()
    }

    /// Sync clock for internal clock
    pub async fn sync_time(
        &mut self,
        client: HttpClient,
        timing: Option<&[UTCTiming]>,
    ) -> IoriResult<()> {
        self.presentation.sync_time(client, timing).await
    }

    pub fn update_mpd(&mut self, mpd: MPD, mpd_url: &Url) -> IoriResult<()> {
        // TODO: update clock and clock timing if necessary

        let mpd_base_url = mpd.base_url.get(0).map(|u| u.base.as_str());
        let base_url = match mpd_base_url {
            Some(mpd_base_url) => merge_baseurls(&mpd_url, mpd_base_url)?,
            None => mpd_url.clone(),
        };

        let mut periods: Vec<DashPeriod> = Vec::with_capacity(mpd.periods.len());
        for period in mpd.periods {
            let last_mut = periods.last_mut();
            let period = DashPeriod::from_mpd(&base_url, period, last_mut)?;
            periods.push(period);
        }
        self.periods = periods;

        Ok(())
    }
}

/// There exist two types of DASH presentations, indicated by MPD@type [DASH]:
pub enum DashPresentation {
    /// In a a static presentation (`MPD@type="static"`) any media segment may be
    /// presented at any time. The DASH client is in complete control over what
    /// content is presented when and the entire presentation is available at any time.
    Static,
    /// In a dynamic presentation (`MPD@type="dynamic"`) the MPD timeline is mapped to wall
    /// clock time, with each media segment on the MPD timeline intended to be presented at
    /// a specific moment in time (with some client-chosen time shift allowed).
    ///
    /// - Furthermore, media segments may become available and cease to be available with the passage of time.
    /// - The MPD may change over time, enabling the structure of the presentation to change over time (e.g.
    /// when a new title in the presentation is offered with a different set of languages).
    Dynamic {
        clock: Clock,
        timing: Vec<UTCTiming>,
        /// In a dynamic presentation, the zero point of the MPD timeline is the mapped to the point in
        /// wall clock time indicated by the effective availability start time, which is formed by taking
        /// `MPD@availabilityStartTime` and applying any LeapSecondInformation offset ([DASH] 5.3.9.5 and 5.13).
        zero_point: DateTime<Utc>,
    },
}

impl DashPresentation {
    pub fn from_mpd(mpd: &MPD) -> Self {
        match mpd.mpdtype.as_deref() {
            Some("dynamic") => Self::Dynamic {
                clock: Clock::new(),
                timing: mpd.UTCTiming.clone(),
                zero_point: mpd.availabilityStartTime.unwrap_or(DateTime::UNIX_EPOCH),
            },
            Some("static") | _ => Self::Static,
        }
    }

    pub async fn sync_time(
        &mut self,
        client: HttpClient,
        timing: Option<&[UTCTiming]>,
    ) -> IoriResult<()> {
        if let DashPresentation::Dynamic {
            clock, timing: t, ..
        } = self
        {
            clock.sync(timing.unwrap_or(t), client).await?;
        }

        Ok(())
    }

    pub fn now(&self) -> DateTime<Utc> {
        if let DashPresentation::Dynamic { clock, .. } = self {
            clock.now()
        } else {
            Utc::now()
        }
    }
}

pub struct DashPeriod {
    id: Option<String>,

    /// The start of a period is specified either explicitly as an offset from the MPD timeline zero point
    /// (Period@start) or implicitly by the end of the previous period ([DASH] 5.3.2). The duration of a
    /// period is specified either explicitly with Period@duration or implicitly by the start point of the
    /// next period ([DASH] 5.3.2).
    /// See also § 8.1 First and last period timing in static presentations and § 8.2 First and last period
    /// timing in dynamic presentations.
    start_time: DateTime<Utc>,
    /// In a dynamic presentation, the last period MAY have a Period@duration, in which case it has a fixed
    /// duration. If without Period@duration, the last period in a dynamic presentation has an unlimited
    /// duration (that may later be shortened by an MPD update).
    duration: Option<Duration>,

    adaptation_sets: Vec<DashAdaptationSet>,
}

impl DashPeriod {
    pub fn from_mpd(
        base_url: &Url,
        period: Period,
        previous: Option<&mut Self>,
    ) -> IoriResult<Self> {
        // If start time is specified, then read it directly
        let (start_time, duration) = if let Some(start) = period.start {
            let start = DateTime::UNIX_EPOCH + TimeDelta::from_std(start)?;

            // if duration of last period is not specified, calculate by current period start and last period start
            if let Some(previous) = previous {
                if previous.duration.is_none() {
                    previous.duration = Some(start - previous.start_time);
                }
            }

            (start, period.duration.map(TimeDelta::from_std).transpose()?)
        } else {
            // Otherwises, current.start = previous.start + previous.duration
            let start = previous
                .ok_or_else(|| {
                    IoriError::MpdParsing("Missing start time for initial period".to_string())
                })
                .and_then(|previous| {
                    previous
                        .duration
                        .map(|duration| previous.start_time + duration)
                        .ok_or_else(|| IoriError::MpdParsing("Missing period duration".to_string()))
                })?;

            (start, period.duration.map(TimeDelta::from_std).transpose()?)
        };

        let inherited = InheritedAddressingValues {
            segment_base: period.SegmentBase.as_ref(),
            segment_list: period.SegmentList.as_ref(),
            segment_template: period.SegmentTemplate.as_ref(),
        };

        let mut adaptation_sets = Vec::with_capacity(period.adaptations.len());
        for adaptation_set in period.adaptations {
            let period_base_url = period.BaseURL.get(0).map(|u| u.base.as_str());
            let base_url = match period_base_url {
                Some(period_base_url) => merge_baseurls(base_url, period_base_url)?,
                None => base_url.clone(),
            };
            let adaptation_set = DashAdaptationSet::from_mpd(base_url, &inherited, adaptation_set)?;
            adaptation_sets.push(adaptation_set);
        }

        Ok(Self {
            id: period.id,
            start_time,
            duration,
            adaptation_sets,
        })
    }
}

pub struct DashAdaptationSet {
    content_type: Option<DashAdaptationSetType>,

    representations: Vec<DashRepresentation>,
}

impl DashAdaptationSet {
    pub fn from_mpd(
        base_url: Url,
        inherited: &InheritedAddressingValues,
        adaptation_set: AdaptationSet,
    ) -> IoriResult<Self> {
        let mut representations = Vec::with_capacity(adaptation_set.representations.len());
        for representation in adaptation_set.representations {
            let adaptation_set_base_url = adaptation_set.BaseURL.get(0).map(|u| u.base.as_str());
            let base_url = match adaptation_set_base_url {
                Some(adaptation_set_base_url) => {
                    merge_baseurls(&base_url, adaptation_set_base_url)?
                }
                None => base_url.clone(),
            };
            let representation = DashRepresentation::from_mpd(
                &base_url,
                InheritedAddressingValues {
                    segment_base: adaptation_set.SegmentBase.as_ref(),
                    segment_list: adaptation_set.SegmentList.as_ref(),
                    segment_template: adaptation_set.SegmentTemplate.as_ref(),
                }
                .merge(inherited),
                representation,
            )?;
            representations.push(representation);
        }

        Ok(Self {
            content_type: adaptation_set
                .contentType
                .map(DashAdaptationSetType::from_string),
            representations,
        })
    }
}

/// Top-level type defined in [RFC6838](https://datatracker.ietf.org/doc/html/rfc6838#section-4.2)
pub enum DashAdaptationSetType {
    Text,
    Image,
    Audio,
    Video,
    Application,
    Custom(String),
}

impl DashAdaptationSetType {
    pub fn from_string(input: String) -> Self {
        match input.as_str() {
            "text" => Self::Text,
            "image" => Self::Image,
            "audio" => Self::Audio,
            "video" => Self::Video,
            "application" => Self::Application,
            _ => Self::Custom(input),
        }
    }
}

pub enum DashRepresentation {
    /// A representation that uses indexed addressing consists of a CMAF track file containing an
    /// index segment, an initialization segment and a sequence of media segments.
    ///
    /// > Note: This addressing mode is sometimes called "SegmentBase" in other documents.
    ///
    /// Not supported yet.
    IndexedAddressing(SegmentBase),
    /// A representation that uses explicit addressing consists of a set of media segments accessed
    /// via URLs constructed using a template defined in the MPD, with the exact sample timeline time
    /// span covered by the samples in each media segment described in the MPD.
    ///
    /// > Note: This addressing mode is sometimes called "SegmentTemplate with SegmentTimeline" in other documents.
    ExplicitAddressing {
        initialization: Option<Url>,
        media: TemplateUrl,
        start_number: Option<u64>,
        timescale: Option<u64>,
        duration: Option<f64>,

        timeline_segments: Vec<TimelineSegment>,
    },
    /// A representation that uses simple addressing consists of a set of media segments accessed via
    /// URLs constructed using a template defined in the MPD, with the MPD describing the nominal time
    /// span of the sample timeline covered by each media segment.
    ///
    /// > Note: This addressing mode is sometimes called "SegmentTemplate without SegmentTimeline" in other documents.
    SimpleAddressing {
        initialization: Option<Url>,
        media: TemplateUrl,
        start_number: Option<u64>,
        timescale: Option<u64>,
        duration: Option<f64>,
    },
    SegmentList(SegmentList),
}

impl DashRepresentation {
    fn from_mpd(
        base_url: &Url,
        inherited: InheritedAddressingValues,
        representation: Representation,
    ) -> IoriResult<Self> {
        let representation_base_url = representation.BaseURL.get(0).map(|u| u.base.as_str());
        let base_url = match representation_base_url {
            Some(adaptation_set_base_url) => merge_baseurls(&base_url, adaptation_set_base_url)?,
            None => base_url.clone(),
        };

        Ok(
            if let Some(segment_base) = representation
                .SegmentBase
                .as_ref()
                .or_else(|| inherited.segment_base)
            {
                // TODO: extract the needed data from segment_base
                Self::IndexedAddressing(segment_base.clone())
            } else if let Some(segment_list) = representation
                .SegmentList
                .as_ref()
                .or_else(|| inherited.segment_list)
            {
                // TODO: extract the needed data from segment_list
                Self::SegmentList(segment_list.clone())
            } else if let Some(template) = representation
                .SegmentTemplate
                .as_ref()
                .or_else(|| inherited.segment_template)
            {
                let initialization = template
                    .initialization
                    .as_ref()
                    .map(|new| merge_baseurls(&base_url, &new))
                    .transpose()?;
                let media = template
                    .media
                    .as_ref()
                    .map(|new| merge_baseurls(&base_url, &new))
                    .transpose()?
                    .map(|u| TemplateUrl(u.to_string()))
                    .ok_or_else(|| {
                        IoriError::MpdParsing(
                            "Missing media url template in representation".to_string(),
                        )
                    })?;
                let start_number = template.startNumber;
                let timescale = template.timescale;
                let duration = template.duration;

                // ExplicitAddressing, aka SegmentTemplate with SegmentTimeline
                if let Some(ref timeline) = template.SegmentTimeline {
                    Self::ExplicitAddressing {
                        initialization,
                        media,
                        start_number,
                        timescale,
                        duration,

                        timeline_segments: timeline
                            .segments
                            .iter()
                            .map(|r| TimelineSegment {
                                time: r.t,
                                duration: r.d,
                                repeat_count: r.r,
                                n: r.n,
                                k: r.k,
                            })
                            .collect(),
                    }
                } else {
                    // SimpleAddressing, aka SegmentTemplate without SegmentTimeline
                    Self::SimpleAddressing {
                        initialization,
                        media,
                        start_number,
                        timescale,
                        duration,
                    }
                }
            } else {
                return Err(IoriError::MpdParsing(
                    "Invalid representation format".to_string(),
                ));
            },
        )
    }
}

pub struct TimelineSegment {
    pub time: Option<u64>,
    pub duration: u64,
    pub repeat_count: Option<i64>,

    pub n: Option<u64>,
    pub k: Option<u64>,
}

pub struct InheritedAddressingValues<'a> {
    segment_base: Option<&'a SegmentBase>,
    segment_list: Option<&'a SegmentList>,
    segment_template: Option<&'a SegmentTemplate>,
}

impl<'a> InheritedAddressingValues<'a> {
    pub fn merge(self, alternate: &Self) -> Self {
        InheritedAddressingValues {
            segment_base: self.segment_base.or_else(|| alternate.segment_base),
            segment_list: self.segment_list.or_else(|| alternate.segment_list),
            segment_template: self.segment_template.or_else(|| alternate.segment_template),
        }
    }
}
