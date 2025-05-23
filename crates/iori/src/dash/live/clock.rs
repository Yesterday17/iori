use chrono::{DateTime, TimeDelta, Utc};
use dash_mpd::{UTCTiming, MPD};

use crate::{HttpClient, IoriError, IoriResult};

#[derive(Debug)]
pub struct Clock {
    /// How much time the local clock is behind the remote clock
    offset: TimeDelta,
}

impl Clock {
    pub fn new() -> Self {
        Self {
            offset: TimeDelta::zero(),
        }
    }

    pub fn now(&self) -> DateTime<Utc> {
        Utc::now() + self.offset
    }

    fn set_time(
        &mut self,
        remote_now: DateTime<Utc>,
        before_request: DateTime<Utc>,
        after_request: DateTime<Utc>,
    ) {
        // <before_request> (inaccurate now time)
        // <remote_now> (accurate remote time)
        // <after_request>
        //
        // accurate now time = accurate remote time - rtt
        // offset = inaccurate now time - accurate now time
        let rtt = (after_request - before_request) / 2;
        let server_now = remote_now + rtt / 2;
        self.offset = server_now - after_request;
        tracing::info!(offset_milliseconds = %self.offset.num_milliseconds(), "Clock time set to {}, offset calculated", remote_now);
    }

    pub async fn sync(&mut self, mpd: &MPD, client: HttpClient) -> IoriResult<()> {
        sync_time(&mpd.UTCTiming, self, client).await
    }
}

fn parse_iso8601_response(response_text: &str) -> IoriResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(response_text)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            // Allow Z suffix for UTC, which is not strictly RFC3339 but used by xsdate
            DateTime::parse_from_str(response_text, "%Y-%m-%dT%H:%M:%SZ")
                .map(|dt| dt.with_timezone(&Utc))
        })?)
}

async fn sync_time(timing: &[UTCTiming], clock: &mut Clock, client: HttpClient) -> IoriResult<()> {
    if timing.is_empty() {
        tracing::warn!("No UTCTiming elements found in MPD, using local time.");
        clock.set_time(Utc::now(), Utc::now(), Utc::now()); // Default to local time if no timing info
        return Ok(());
    }

    let mut last_error: Option<IoriError> = None;

    let before_request = Utc::now();
    for timing in timing {
        tracing::debug!(scheme = %timing.schemeIdUri, value = %timing.value.as_deref().unwrap_or(""), "Attempting to sync time with scheme");
        match timing.schemeIdUri.as_str() {
            "urn:mpeg:dash:utc:http-xsdate:2014" | "urn:mpeg:dash:utc:http-iso:2014" => {
                if let Some(url) = &timing.value {
                    match client.get(url).send().await {
                        Ok(response) => {
                            let after_request = Utc::now();
                            if response.status().is_success() {
                                match response.text().await {
                                    Ok(text) => match parse_iso8601_response(text.trim()) {
                                        Ok(datetime) => {
                                            clock.set_time(datetime, before_request, after_request);
                                            return Ok(());
                                        }
                                        Err(e) => {
                                            tracing::warn!(url, error = %e, "Failed to parse xsdate/iso8601 response");
                                            last_error = Some(e);
                                        }
                                    },
                                    Err(e) => {
                                        tracing::warn!(url, error = %e, "Failed to read xsdate/iso8601 response text");
                                        last_error = Some(IoriError::RequestError(e));
                                    }
                                }
                            } else {
                                tracing::warn!(url, status = %response.status(), "HTTP request for xsdate/iso8601 failed");
                                last_error = Some(IoriError::HttpError(response.status()));
                            }
                        }
                        Err(e) => {
                            tracing::warn!(url, error = %e, "HTTP GET request for xsdate/iso8601 failed");
                            last_error = Some(IoriError::RequestError(e));
                        }
                    }
                } else {
                    tracing::warn!(scheme = %timing.schemeIdUri, "Missing value for timing scheme");
                    last_error = Some(IoriError::InvalidTimingSchema("Missing value".to_string()));
                }
            }
            "urn:mpeg:dash:utc:direct:2014" => {
                if let Some(value) = &timing.value {
                    match DateTime::parse_from_rfc3339(value) {
                        Ok(datetime) => {
                            clock.set_time(
                                datetime.with_timezone(&Utc),
                                before_request,
                                before_request,
                            );
                            return Ok(());
                        }
                        Err(e) => {
                            tracing::warn!(value, error = %e, "Failed to parse direct timing value");
                            last_error = Some(e.into());
                        }
                    }
                } else {
                    tracing::warn!(scheme = %timing.schemeIdUri, "Missing value for direct timing scheme");
                    last_error = Some(IoriError::InvalidTimingSchema(
                        "Missing value for direct scheme".to_string(),
                    ));
                }
            }
            "urn:mpeg:dash:utc:http-head:2014" => {
                if let Some(url) = &timing.value {
                    match client.head(url).send().await {
                        Ok(response) => {
                            let after_request = Utc::now();
                            if response.status().is_success() {
                                if let Some(date_header) =
                                    response.headers().get(reqwest::header::DATE)
                                {
                                    match date_header.to_str() {
                                        Ok(date_str) => {
                                            match DateTime::parse_from_rfc2822(date_str) {
                                                Ok(datetime) => {
                                                    clock.set_time(
                                                        datetime.with_timezone(&Utc),
                                                        before_request,
                                                        after_request,
                                                    );
                                                    return Ok(());
                                                }
                                                Err(e) => {
                                                    tracing::warn!(header = %date_str, error = %e, "Failed to parse Date header");
                                                    last_error = Some(e.into());
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(url, error = %e, "Invalid Date header string");
                                            last_error = Some(IoriError::DateTimeParsing(
                                                "Invalid Date header string".to_string(),
                                            ));
                                        }
                                    }
                                } else {
                                    tracing::warn!(
                                        url,
                                        "Missing Date header in HTTP HEAD response"
                                    );
                                    last_error = Some(IoriError::DateTimeParsing(
                                        "Missing Date header".to_string(),
                                    ));
                                }
                            } else {
                                tracing::warn!(url, status = %response.status(), "HTTP HEAD request failed");
                                last_error = Some(IoriError::HttpError(response.status()));
                            }
                        }
                        Err(e) => {
                            tracing::warn!(url, error = %e, "HTTP HEAD request failed");
                            last_error = Some(IoriError::RequestError(e));
                        }
                    }
                } else {
                    tracing::warn!(scheme = %timing.schemeIdUri, "Missing value for http-head timing scheme");
                    last_error = Some(IoriError::InvalidTimingSchema(
                        "Missing value for http-head scheme".to_string(),
                    ));
                }
            }
            "urn:mpeg:dash:utc:http-ntp:2014" | "urn:mpeg:dash:utc:ntp:2014" => {
                tracing::warn!(scheme = %timing.schemeIdUri, "NTP schemes are not supported");
                last_error = Some(IoriError::InvalidTimingSchema(format!(
                    "Unsupported scheme: {}",
                    timing.schemeIdUri
                )));
            }
            others => {
                tracing::warn!(scheme = %others, "Unknown timing scheme");
                last_error = Some(IoriError::InvalidTimingSchema(others.into()));
            }
        }
    }

    if let Some(err) = last_error {
        return Err(err);
    }

    // If all schemes failed but there was no UTCTiming element that could have been processed
    // (e.g. only unknown or NTP schemes), this is an error.
    // If UTCTiming was empty, we already defaulted to local time.
    if !timing.is_empty() {
        return Err(IoriError::InvalidTimingSchema(
            "All supported time sync methods failed".to_string(),
        ));
    }

    Ok(())
}
