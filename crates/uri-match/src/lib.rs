use matchit::{Match, Router};
use std::collections::HashMap;
use url::Url;

mod error;
pub use error::*;

pub enum RouterScheme {
    Http,
    Https,
    Both,
}

pub struct HttpRouter<T> {
    http: Router<T>,
    https: Router<T>,
    both: Router<T>,
}

impl<T> HttpRouter<T> {
    pub fn at<'path>(&self, scheme: &str, path: &'path str) -> Result<Match<'_, 'path, &T>> {
        match scheme {
            "http" => self.both.at(path).or_else(|_| self.http.at(path)),
            "https" => self.both.at(path).or_else(|_| self.https.at(path)),
            _ => unreachable!(),
        }
        .map_err(|_| UriHandlerError::NoMatchingPath(path.to_string()))
    }
}

impl<T> Default for HttpRouter<T> {
    fn default() -> Self {
        Self {
            http: Router::new(),
            https: Router::new(),
            both: Router::new(),
        }
    }
}

pub struct UriSchemeMatcher<T> {
    schemes: HashMap<String, T>,
    http: HashMap<String, HttpRouter<T>>,
}

impl<T> UriSchemeMatcher<T> {
    pub fn new() -> Self {
        Self {
            schemes: HashMap::new(),
            http: HashMap::new(),
        }
    }

    pub fn register_scheme(&mut self, scheme: &str, value: T) -> Result<()> {
        if scheme.is_empty() || scheme == "http" || scheme == "https" {
            return Err(UriHandlerError::InvalidScheme(scheme.to_string()));
        }

        self.schemes.insert(scheme.to_string(), value);
        Ok(())
    }

    pub fn register_http_route(
        &mut self,
        scheme: RouterScheme,
        hostname: impl Into<String>,
        pattern: &str,
        value: T,
    ) -> Result<()> {
        if !pattern.starts_with('/') {
            return Err(UriHandlerError::InvalidPattern(pattern.to_string()));
        }

        let router = self.http.entry(hostname.into()).or_default();

        match scheme {
            RouterScheme::Http => router.http.insert(pattern, value)?,
            RouterScheme::Https => router.https.insert(pattern, value)?,
            RouterScheme::Both => router.both.insert(pattern, value)?,
        }

        Ok(())
    }

    pub fn try_match(&self, uri: &str) -> Result<MatchUriResult<&T>> {
        let url = Url::parse(uri)?;

        match url.scheme() {
            scheme @ ("http" | "https") => {
                let Some(matched) = url
                    .host_str()
                    .and_then(|h| self.http.get(h))
                    .and_then(|m| m.at(scheme, url.path()).ok())
                else {
                    return Err(UriHandlerError::NoMatchingRoute(url));
                };

                let mut params = UriParams {
                    path_params: HashMap::new(),
                    query_params: HashMap::new(),
                };

                for (key, value) in matched.params.iter() {
                    params
                        .path_params
                        .insert(key.to_string(), value.to_string());
                }

                for (key, values) in url.query_pairs() {
                    params
                        .query_params
                        .insert(key.to_string(), values.to_string());
                }

                Ok(MatchUriResult::Http(matched.value, params))
            }
            scheme => {
                let Some(inner) = self.schemes.get(scheme) else {
                    return Err(UriHandlerError::NoMatchingRoute(url));
                };

                Ok(MatchUriResult::Scheme(inner))
            }
        }
    }
}

impl<T> Default for UriSchemeMatcher<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MatchUriResult<T> {
    Http(T, UriParams),
    Scheme(T),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UriParams {
    pub path_params: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_hosts_match() -> Result<()> {
        let mut matcher = UriSchemeMatcher::new();
        matcher.register_http_route(RouterScheme::Http, "localhost", "/", "1")?;
        matcher.register_http_route(RouterScheme::Http, "127.0.0.1", "/", "2")?;

        let result = matcher.try_match("http://localhost/")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "1");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://127.0.0.1/")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "2");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        Ok(())
    }

    #[test]
    fn test_http_path_match() -> Result<()> {
        let mut matcher = UriSchemeMatcher::new();
        matcher.register_http_route(RouterScheme::Http, "localhost", "/", "/")?;
        matcher.register_http_route(RouterScheme::Http, "localhost", "/ping", "/ping")?;
        matcher.register_http_route(RouterScheme::Http, "localhost", "/ping/1", "/ping/1")?;

        let result = matcher.try_match("http://localhost/")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "/");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://localhost/ping")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "/ping");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://localhost/ping/1")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "/ping/1");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        Ok(())
    }

    #[test]
    fn test_http_path_match_with_params() -> Result<()> {
        let mut matcher = UriSchemeMatcher::new();
        matcher.register_http_route(RouterScheme::Http, "localhost", "/", "/")?;
        matcher.register_http_route(RouterScheme::Http, "localhost", "/ping/{id}", "/ping/{id}")?;
        matcher.register_http_route(
            RouterScheme::Http,
            "localhost",
            "/ping/{id}/edit",
            "/ping/{id}/edit",
        )?;
        matcher.register_http_route(
            RouterScheme::Http,
            "localhost",
            "/{foo}/{bar}/{*rest}",
            "/{foo}/{bar}/{*rest}",
        )?;

        let result = matcher.try_match("http://localhost/")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "/");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://localhost/ping/1")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "/ping/{id}");
                assert_eq!(
                    params,
                    UriParams {
                        path_params: HashMap::from([("id".to_string(), "1".to_string())]),
                        query_params: HashMap::new(),
                    }
                );
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://localhost/ping/1/edit")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "/ping/{id}/edit");
                assert_eq!(
                    params,
                    UriParams {
                        path_params: HashMap::from([("id".to_string(), "1".to_string())]),
                        query_params: HashMap::new(),
                    }
                );
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://localhost/foo/bar/baz/qux")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "/{foo}/{bar}/{*rest}");
                assert_eq!(
                    params,
                    UriParams {
                        path_params: HashMap::from([
                            ("foo".to_string(), "foo".to_string()),
                            ("bar".to_string(), "bar".to_string()),
                            ("rest".to_string(), "baz/qux".to_string()),
                        ]),
                        query_params: HashMap::new(),
                    }
                );
            }
            _ => panic!("Expected Http result"),
        }
        Ok(())
    }

    #[test]
    fn test_http_https_match() -> Result<()> {
        let mut matcher = UriSchemeMatcher::new();
        matcher.register_http_route(RouterScheme::Http, "localhost", "/", "http")?;
        matcher.register_http_route(RouterScheme::Https, "localhost", "/", "https")?;

        let result = matcher.try_match("http://localhost/")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "http");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("https://localhost/")?;
        match result {
            MatchUriResult::Http(value, params) => {
                assert_eq!(*value, "https");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Https result"),
        }

        Ok(())
    }
}
