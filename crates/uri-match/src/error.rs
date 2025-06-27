use matchit::InsertError;
use thiserror::Error;
use wildcard::WildcardError;

pub type Result<T> = std::result::Result<T, UriHandlerError>;

#[derive(Error, Debug)]
pub enum UriHandlerError {
    #[error("Invalid scheme: {0}")]
    InvalidScheme(String),

    #[error("Invalid host pattern: {0}")]
    InvalidHostPattern(#[from] WildcardError),

    #[error("Invalid pattern: {0}")]
    InvalidPathPattern(String),

    #[error("Route insert error: {0}")]
    RouteInsertError(#[from] InsertError),

    #[error("Uri parse error: {0}")]
    UriParseError(#[from] url::ParseError),

    #[error("No matching route found for path: {0}")]
    NoMatchingPath(String),
}
