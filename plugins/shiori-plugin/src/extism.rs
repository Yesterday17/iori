use extism_pdk::*;
use serde::de::DeserializeOwned;

#[derive(Default)]
pub struct HttpClient {
    user_agent: Option<&'static str>,
}

impl HttpClient {
    pub fn new() -> Self {
        Self { user_agent: None }
    }

    pub fn ua(user_agent: &'static str) -> Self {
        Self {
            user_agent: Some(user_agent),
        }
    }

    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> anyhow::Result<T> {
        let mut request = HttpRequest::new(url);
        if let Some(user_agent) = self.user_agent {
            request = request.with_header("User-Agent", user_agent);
        }

        let response = http::request::<()>(&request, None)?;
        let data: T = response.json()?;
        Ok(data)
    }
}
