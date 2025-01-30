use chrono::{DateTime, TimeDelta, Utc};
use dash_mpd::MPD;

use crate::error::{IoriError, IoriResult};

pub struct Clock {
    offset: TimeDelta,
}

impl Clock {
    fn new() -> Self {
        Self {
            offset: TimeDelta::zero(),
        }
    }

    fn set_time(&mut self, now: DateTime<Utc>) {
        self.offset = now - Utc::now();
    }

    fn now(&self) -> DateTime<Utc> {
        Utc::now() + self.offset
    }

    fn sync(&mut self) -> IoriResult<()> {
        todo!()
    }
}

pub async fn sync_time(mpd: &MPD) -> IoriResult<()> {
    for timing in &mpd.UTCTiming {
        match timing.schemeIdUri.as_str() {
            "urn:mpeg:dash:utc:http-xsdate:2014" => todo!(),
            "urn:mpeg:dash:utc:http-iso:2014" => todo!(),
            "urn:mpeg:dash:utc:http-ntp:2014" => todo!(),
            "urn:mpeg:dash:utc:ntp:2014" => todo!(),
            "urn:mpeg:dash:utc:http-head:2014" => todo!(),
            "urn:mpeg:dash:utc:direct:2014" => todo!(),
            others => {
                return Err(IoriError::InvalidTimingSchema(others.into()));
            }
        }
    }

    Ok(())
}
