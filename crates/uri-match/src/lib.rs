use matchit::{Match, Router};
use std::{collections::HashMap, hash::Hash};
pub use url::Url;
use wildcard::Wildcard;

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

pub struct UriSchemeMatcher<S = (), H = ()> {
    schemes: HashMap<String, S>,
    http: HashMap<HostMatcher, HttpRouter<H>>,
}

#[derive(Clone)]
pub enum HostMatcher {
    Literal(String),
    Wildcard(Wildcard<'static>),
    AnyOf(Vec<HostMatcher>),
}

impl HostMatcher {
    pub fn literal(literal: &'static str) -> Self {
        Self::Literal(literal.to_string())
    }

    pub fn wildcard(pattern: &'static [u8]) -> Result<Self> {
        Ok(Self::Wildcard(Wildcard::new(pattern)?))
    }

    pub fn matches(&self, host: &str) -> bool {
        match self {
            HostMatcher::Literal(literal) => literal == host,
            HostMatcher::Wildcard(wildcard) => wildcard.is_match(host.as_bytes()),
            HostMatcher::AnyOf(matchers) => matchers.iter().any(|m| m.matches(host)),
        }
    }
}

impl Hash for HostMatcher {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            HostMatcher::Literal(literal) => literal.hash(state),
            HostMatcher::Wildcard(wildcard) => wildcard.pattern().hash(state),
            HostMatcher::AnyOf(matchers) => matchers.iter().for_each(|m| m.hash(state)),
        }
    }
}

impl PartialEq for HostMatcher {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (HostMatcher::Literal(a), HostMatcher::Literal(b)) => a == b,
            (HostMatcher::Wildcard(a), HostMatcher::Wildcard(b)) => a.pattern().eq(b.pattern()),
            (HostMatcher::AnyOf(a), HostMatcher::AnyOf(b)) => a.eq(b),
            _ => false,
        }
    }
}

impl<I> From<I> for HostMatcher
where
    I: Into<String>,
{
    fn from(value: I) -> Self {
        Self::Literal(value.into())
    }
}

impl Eq for HostMatcher {}

impl<S, H> UriSchemeMatcher<S, H> {
    pub fn new() -> Self {
        Self {
            schemes: HashMap::new(),
            http: HashMap::new(),
        }
    }

    pub fn register_scheme(&mut self, scheme: &str, value: S) -> Result<()> {
        if scheme.is_empty() || scheme == "http" || scheme == "https" {
            return Err(UriHandlerError::InvalidScheme(scheme.to_string()));
        }

        self.schemes.insert(scheme.to_string(), value);
        Ok(())
    }

    pub fn register_http_route(
        &mut self,
        scheme: RouterScheme,
        host_matcher: impl Into<HostMatcher>,
        path_pattern: &str,
        value: H,
    ) -> Result<()> {
        if !path_pattern.starts_with('/') {
            return Err(UriHandlerError::InvalidPathPattern(
                path_pattern.to_string(),
            ));
        }

        let router = self.http.entry(host_matcher.into()).or_default();

        match scheme {
            RouterScheme::Http => router.http.insert(path_pattern, value)?,
            RouterScheme::Https => router.https.insert(path_pattern, value)?,
            RouterScheme::Both => router.both.insert(path_pattern, value)?,
        }

        Ok(())
    }

    pub fn try_match(&self, url: Url) -> MatchUriResult<&S, &H> {
        match url.scheme() {
            scheme @ ("http" | "https") => {
                let Some(matched) = url
                    .host_str()
                    .and_then(|h| self.http.keys().find(|m| m.matches(h)))
                    .and_then(|k| self.http.get(k))
                    .and_then(|m| m.at(scheme, url.path()).ok())
                else {
                    return MatchUriResult::NoMatch(url);
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

                MatchUriResult::Http(matched.value, params, url)
            }
            scheme => {
                let Some(inner) = self.schemes.get(scheme) else {
                    return MatchUriResult::NoMatch(url);
                };

                MatchUriResult::Scheme(inner, url)
            }
        }
    }
}

impl<S, H> Default for UriSchemeMatcher<S, H> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MatchUriResult<S, H> {
    Scheme(S, Url),
    Http(H, UriParams, Url),
    NoMatch(Url),
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
        let mut matcher = UriSchemeMatcher::<(), &str>::new();
        matcher.register_http_route(RouterScheme::Http, "localhost", "/", "1")?;
        matcher.register_http_route(RouterScheme::Http, "127.0.0.1", "/", "2")?;

        let result = matcher.try_match("http://localhost/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "1");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://127.0.0.1/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "2");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        Ok(())
    }

    #[test]
    fn test_http_path_match() -> Result<()> {
        let mut matcher = UriSchemeMatcher::<(), &str>::new();
        matcher.register_http_route(RouterScheme::Http, "localhost", "/", "/")?;
        matcher.register_http_route(RouterScheme::Http, "localhost", "/ping", "/ping")?;
        matcher.register_http_route(RouterScheme::Http, "localhost", "/ping/1", "/ping/1")?;

        let result = matcher.try_match("http://localhost/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "/");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://localhost/ping".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "/ping");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://localhost/ping/1".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "/ping/1");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        Ok(())
    }

    #[test]
    fn test_http_path_match_with_params() -> Result<()> {
        let mut matcher = UriSchemeMatcher::<(), &str>::new();
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

        let result = matcher.try_match("http://localhost/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "/");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://localhost/ping/1".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
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

        let result = matcher.try_match("http://localhost/ping/1/edit".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
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

        let result = matcher.try_match("http://localhost/foo/bar/baz/qux".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
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
        let mut matcher = UriSchemeMatcher::<(), &str>::new();
        matcher.register_http_route(RouterScheme::Http, "localhost", "/", "http")?;
        matcher.register_http_route(RouterScheme::Https, "localhost", "/", "https")?;

        let result = matcher.try_match("http://localhost/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "http");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("https://localhost/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "https");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Https result"),
        }

        Ok(())
    }

    #[test]
    fn test_http_host_wildcard() -> Result<()> {
        let mut matcher = UriSchemeMatcher::<(), &str>::new();
        matcher.register_http_route(
            RouterScheme::Http,
            HostMatcher::wildcard(b"*.mmf.moe")?,
            "/",
            "http",
        )?;
        matcher.register_http_route(
            RouterScheme::Https,
            HostMatcher::wildcard(b"*.mmf.moe")?,
            "/",
            "https",
        )?;

        let result = matcher.try_match("http://test1.mmf.moe/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "http");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("https://test2.mmf.moe/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "https");
                assert_eq!(params, UriParams::default());
            }
            _ => panic!("Expected Https result"),
        }

        let result = matcher.try_match("https://mmf.moe/".parse()?);
        assert!(matches!(result, MatchUriResult::NoMatch(_)));

        Ok(())
    }
}
