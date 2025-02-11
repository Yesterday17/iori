use reqwest::header::{HeaderMap, HeaderValue};

pub trait IntoLicenseHeaders {
    fn into_license_headers(self) -> HeaderMap<HeaderValue>;
}

impl IntoLicenseHeaders for HeaderMap<HeaderValue> {
    fn into_license_headers(self) -> HeaderMap<HeaderValue> {
        self
    }
}

impl IntoLicenseHeaders for String {
    fn into_license_headers(self) -> HeaderMap<HeaderValue> {
        let mut map = HeaderMap::new();
        map.insert(
            "AcquireLicenseAssertion",
            HeaderValue::from_str(&self).unwrap(),
        );
        map.into_license_headers()
    }
}

impl IntoLicenseHeaders for () {
    fn into_license_headers(self) -> HeaderMap<HeaderValue> {
        HeaderMap::new().into_license_headers()
    }
}
