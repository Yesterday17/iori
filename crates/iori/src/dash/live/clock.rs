use chrono::{DateTime, TimeDelta, Utc};
use dash_mpd::MPD;

use crate::{HttpClient, IoriError, IoriResult};

#[derive(Debug)]
pub struct Clock {
    offset: TimeDelta,
}

impl Clock {
    pub fn new() -> Self {
        Self {
            offset: TimeDelta::zero(),
        }
    }

    // now() can take &self as it only reads offset
    pub fn now(&self) -> DateTime<Utc> {
        Utc::now() + self.offset
    }

    // set_time and sync need &mut self, will be called on a locked MutexGuard<Clock>
    fn set_time(&mut self, now: DateTime<Utc>) {
        self.offset = now - Utc::now();
        tracing::debug!(offset_seconds = %self.offset.num_seconds(), "Clock time set to {}, offset calculated", now);
    }

    pub async fn sync(&mut self, mpd: &MPD, client: HttpClient) -> IoriResult<()> {
        sync_time(mpd, self, client).await
    }
}

async fn parse_iso8601_response(response_text: &str) -> IoriResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(response_text)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            // Allow Z suffix for UTC, which is not strictly RFC3339 but used by xsdate
            DateTime::parse_from_str(response_text, "%Y-%m-%dT%H:%M:%SZ")
                .map(|dt| dt.with_timezone(&Utc))
        })
        .map_err(|e| IoriError::DateTimeParsing(e.to_string()))
}

async fn sync_time(mpd: &MPD, clock: &mut Clock, client: HttpClient) -> IoriResult<()> {
    if mpd.UTCTiming.is_empty() {
        tracing::warn!("No UTCTiming elements found in MPD, using local time.");
        clock.set_time(Utc::now()); // Default to local time if no timing info
        return Ok(());
    }

    let mut last_error: Option<IoriError> = None;

    for timing in &mpd.UTCTiming {
        tracing::debug!(scheme = %timing.schemeIdUri, value = %timing.value.as_deref().unwrap_or(""), "Attempting to sync time with scheme");
        match timing.schemeIdUri.as_str() {
            "urn:mpeg:dash:utc:http-xsdate:2014" | "urn:mpeg:dash:utc:http-iso:2014" => {
                if let Some(url) = &timing.value {
                    match client.get(url).send().await {
                        Ok(response) => {
                            if response.status().is_success() {
                                match response.text().await {
                                    Ok(text) => match parse_iso8601_response(text.trim()).await {
                                        Ok(datetime) => {
                                            clock.set_time(datetime);
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
                                last_error = Some(IoriError::General(format!(
                                    "Network Error: HTTP status {}",
                                    response.status()
                                )));
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
                            clock.set_time(datetime.with_timezone(&Utc));
                            return Ok(());
                        }
                        Err(e) => {
                            tracing::warn!(value, error = %e, "Failed to parse direct timing value");
                            last_error = Some(IoriError::DateTimeParsing(e.to_string()));
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
                            if response.status().is_success() {
                                if let Some(date_header) =
                                    response.headers().get(reqwest::header::DATE)
                                {
                                    match date_header.to_str() {
                                        Ok(date_str) => {
                                            match DateTime::parse_from_rfc2822(date_str) {
                                                Ok(datetime) => {
                                                    clock.set_time(datetime.with_timezone(&Utc));
                                                    return Ok(());
                                                }
                                                Err(e) => {
                                                    tracing::warn!(header = %date_str, error = %e, "Failed to parse Date header");
                                                    last_error = Some(IoriError::DateTimeParsing(
                                                        e.to_string(),
                                                    ));
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
                                last_error = Some(IoriError::General(format!(
                                    "Network Error: HTTP status {}",
                                    response.status()
                                )));
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
        Err(err)
    } else {
        // If all schemes failed but there was no UTCTiming element that could have been processed
        // (e.g. only unknown or NTP schemes), this is an error.
        // If UTCTiming was empty, we already defaulted to local time.
        if !mpd.UTCTiming.is_empty() {
            Err(IoriError::InvalidTimingSchema(
                "All supported time sync methods failed".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}
