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

#[derive(Clone)]
pub enum HostMatcher {
    Literal(String),
    Wildcard(Wildcard<'static>),
    AnyOf(Vec<HostMatcher>),
}

impl HostMatcher {
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

impl Eq for HostMatcher {}

impl From<&str> for HostMatcher {
    fn from(value: &str) -> Self {
        Self::Literal(value.to_string())
    }
}

impl TryFrom<&'static [u8]> for HostMatcher {
    type Error = UriHandlerError;

    fn try_from(value: &'static [u8]) -> std::result::Result<Self, Self::Error> {
        Ok(HostMatcher::Wildcard(Wildcard::new(value)?))
    }
}

pub enum PathMatcher {
    Route(String),
    Wildcard(Wildcard<'static>),
}

impl TryFrom<&str> for PathMatcher {
    type Error = UriHandlerError;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        let value = value.to_string();
        if !value.starts_with('/') {
            return Err(UriHandlerError::InvalidPathPattern(value));
        }

        Ok(PathMatcher::Route(value))
    }
}

impl TryFrom<&'static [u8]> for PathMatcher {
    type Error = UriHandlerError;

    fn try_from(value: &'static [u8]) -> std::result::Result<Self, Self::Error> {
        Ok(PathMatcher::Wildcard(Wildcard::new(value)?))
    }
}

pub struct HttpRouter<T> {
    http: Router<T>,
    https: Router<T>,
    both: Router<T>,

    http_wildcard: Vec<(Wildcard<'static>, T)>,
    https_wildcard: Vec<(Wildcard<'static>, T)>,
    both_wildcard: Vec<(Wildcard<'static>, T)>,
}

pub enum HttpRouterMatchResult<'k, 'v, 'f, T> {
    Match(Match<'k, 'v, &'f T>),
    Wildcard(&'f T),
}

impl<T> HttpRouter<T> {
    pub fn insert(&mut self, scheme: RouterScheme, matcher: PathMatcher, value: T) -> Result<()> {
        match (scheme, matcher) {
            (RouterScheme::Http, PathMatcher::Route(route)) => self.http.insert(route, value)?,
            (RouterScheme::Https, PathMatcher::Route(route)) => self.https.insert(route, value)?,
            (RouterScheme::Both, PathMatcher::Route(route)) => self.both.insert(route, value)?,
            (RouterScheme::Http, PathMatcher::Wildcard(wildcard)) => {
                self.http_wildcard.push((wildcard, value));
            }
            (RouterScheme::Https, PathMatcher::Wildcard(wildcard)) => {
                self.https_wildcard.push((wildcard, value));
            }
            (RouterScheme::Both, PathMatcher::Wildcard(wildcard)) => {
                self.both_wildcard.push((wildcard, value));
            }
        }

        Ok(())
    }

    pub fn at<'path>(
        &self,
        scheme: &str,
        path: &'path str,
    ) -> Result<HttpRouterMatchResult<'_, 'path, '_, T>> {
        match scheme {
            "http" => self
                .both
                .at(path)
                .or_else(|_| self.http.at(path))
                .map(HttpRouterMatchResult::Match)
                .ok()
                .or_else(|| {
                    self.both_wildcard.iter().find_map(|(matcher, f)| {
                        matcher
                            .is_match(path.as_bytes())
                            .then_some(HttpRouterMatchResult::Wildcard(f))
                    })
                })
                .or_else(|| {
                    self.http_wildcard.iter().find_map(|(matcher, f)| {
                        matcher
                            .is_match(path.as_bytes())
                            .then_some(HttpRouterMatchResult::Wildcard(f))
                    })
                }),
            "https" => self
                .both
                .at(path)
                .or_else(|_| self.https.at(path))
                .map(HttpRouterMatchResult::Match)
                .ok()
                .or_else(|| {
                    self.both_wildcard.iter().find_map(|(matcher, f)| {
                        matcher
                            .is_match(path.as_bytes())
                            .then_some(HttpRouterMatchResult::Wildcard(f))
                    })
                })
                .or_else(|| {
                    self.https_wildcard.iter().find_map(|(matcher, f)| {
                        matcher
                            .is_match(path.as_bytes())
                            .then_some(HttpRouterMatchResult::Wildcard(f))
                    })
                }),
            _ => unreachable!(),
        }
        .ok_or_else(|| UriHandlerError::NoMatchingPath(path.to_string()))
    }
}

impl<T> Default for HttpRouter<T> {
    fn default() -> Self {
        Self {
            http: Router::new(),
            https: Router::new(),
            both: Router::new(),

            http_wildcard: Vec::new(),
            https_wildcard: Vec::new(),
            both_wildcard: Vec::new(),
        }
    }
}

pub struct UriSchemeMatcher<S = (), H = ()> {
    schemes: HashMap<String, S>,
    http: Vec<(HostMatcher, HttpRouter<H>)>,
}

impl<S, H> UriSchemeMatcher<S, H> {
    pub fn new() -> Self {
        Self {
            schemes: HashMap::new(),
            // TODO: use IndexMap
            http: Vec::new(),
        }
    }

    pub fn register_scheme(&mut self, scheme: &str, value: S) -> Result<()> {
        // TODO: Check scheme at compile time using procedural macro
        if scheme.is_empty() || scheme == "http" || scheme == "https" {
            return Err(UriHandlerError::InvalidScheme(scheme.to_string()));
        }

        self.schemes.insert(scheme.to_string(), value);
        Ok(())
    }

    pub fn register_http_route<HO, PA>(
        &mut self,
        scheme: RouterScheme,
        host_matcher: HO,
        path_matcher: PA,
        value: H,
    ) -> Result<()>
    where
        HO: TryInto<HostMatcher>,
        HO::Error: Into<UriHandlerError>,
        PA: TryInto<PathMatcher>,
        PA::Error: Into<UriHandlerError>,
    {
        let host_matcher = host_matcher.try_into().map_err(|e| e.into())?;
        let path_matcher = path_matcher.try_into().map_err(|e| e.into())?;

        let router = self.http.iter_mut().find_map(|h| {
            if h.0 == host_matcher {
                Some(&mut h.1)
            } else {
                None
            }
        });
        if let Some(router) = router {
            router.insert(scheme, path_matcher, value)?;
        } else {
            let mut router = HttpRouter::default();
            router.insert(scheme, path_matcher, value)?;
            self.http.push((host_matcher, router));
        }

        Ok(())
    }

    pub fn try_match(&self, url: Url) -> MatchUriResult<&S, &H> {
        match url.scheme() {
            scheme @ ("http" | "https") => {
                let Some(matched) = url
                    .host_str()
                    .and_then(|h| self.http.iter().find(|m| m.0.matches(h)))
                    .map(|k| &k.1)
                    .and_then(|m| m.at(scheme, url.path()).ok())
                else {
                    return MatchUriResult::NoMatch(url);
                };

                let mut params = UriParams {
                    host: url.host_str().map(|s| s.to_string()),
                    path_params: HashMap::new(),
                    query_params: HashMap::new(),
                };

                let f = match matched {
                    HttpRouterMatchResult::Match(matched) => {
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

                        matched.value
                    }
                    HttpRouterMatchResult::Wildcard(f) => f,
                };

                MatchUriResult::Http(f, params, url)
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
    pub host: Option<String>,
    pub path_params: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
}

impl UriParams {
    pub fn with_host(host: impl Into<String>) -> Self {
        UriParams {
            host: Some(host.into()),
            path_params: HashMap::new(),
            query_params: HashMap::new(),
        }
    }
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
                assert_eq!(params, UriParams::with_host("localhost"));
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://127.0.0.1/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "2");
                assert_eq!(params, UriParams::with_host("127.0.0.1"));
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
                assert_eq!(params, UriParams::with_host("localhost"));
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://localhost/ping".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "/ping");
                assert_eq!(params, UriParams::with_host("localhost"));
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("http://localhost/ping/1".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "/ping/1");
                assert_eq!(params, UriParams::with_host("localhost"));
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
                assert_eq!(params, UriParams::with_host("localhost"));
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
                        host: Some("localhost".to_string()),
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
                        host: Some("localhost".to_string()),
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
                        host: Some("localhost".to_string()),
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
                assert_eq!(params, UriParams::with_host("localhost"));
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("https://localhost/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "https");
                assert_eq!(params, UriParams::with_host("localhost"));
            }
            _ => panic!("Expected Https result"),
        }

        Ok(())
    }

    #[test]
    fn test_http_host_wildcard() -> Result<()> {
        let mut matcher = UriSchemeMatcher::<(), &str>::new();
        matcher.register_http_route(RouterScheme::Http, "*.mmf.moe".as_bytes(), "/", "http")?;
        matcher.register_http_route(RouterScheme::Https, "*.mmf.moe".as_bytes(), "/", "https")?;

        let result = matcher.try_match("http://test1.mmf.moe/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "http");
                assert_eq!(params, UriParams::with_host("test1.mmf.moe"));
            }
            _ => panic!("Expected Http result"),
        }

        let result = matcher.try_match("https://test2.mmf.moe/".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "https");
                assert_eq!(params, UriParams::with_host("test2.mmf.moe"));
            }
            _ => panic!("Expected Https result"),
        }

        let result = matcher.try_match("https://mmf.moe/".parse()?);
        assert!(matches!(result, MatchUriResult::NoMatch(_)));

        Ok(())
    }

    #[test]
    fn tset_http_rest_with_suffix() -> Result<()> {
        let mut matcher = UriSchemeMatcher::<(), &str>::new();
        matcher.register_http_route(
            RouterScheme::Http,
            "localhost",
            "/*.mpd".as_bytes(),
            "http",
        )?;

        let result = matcher.try_match("http://localhost/test.mpd".parse()?);
        match result {
            MatchUriResult::Http(value, params, _) => {
                assert_eq!(*value, "http");
                assert_eq!(params, UriParams::with_host("localhost"));
            }
            _ => panic!("Expected Http result"),
        }

        Ok(())
    }
}
