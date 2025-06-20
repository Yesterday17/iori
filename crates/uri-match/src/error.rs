use matchit::InsertError;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, UriHandlerError>;

#[derive(Error, Debug)]
pub enum UriHandlerError {
    #[error("Invalid scheme: {0}")]
    InvalidScheme(String),

    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),

    #[error("Route insert error: {0}")]
    RouteInsertError(#[from] InsertError),

    #[error("Uri parse error: {0}")]
    UriParseError(#[from] url::ParseError),

    #[error("No matching route found for url: {0}")]
    NoMatchingRoute(url::Url),

    #[error("No matching route found for path: {0}")]
    NoMatchingPath(String),
}
