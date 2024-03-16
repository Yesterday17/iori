use std::str::FromStr;

use fake_user_agent::get_chrome_rua;
use tokio_tungstenite::tungstenite::{
    error::UrlError,
    handshake::client::{generate_key, Request},
    http::Uri,
    Error as TungsteniteError,
};

pub(crate) fn prepare_websocket_request<S>(
    ws_url: S,
    protocols: Vec<String>,
) -> anyhow::Result<Request>
where
    S: AsRef<str>,
{
    log::debug!("ws_url: {}", ws_url.as_ref());
    let uri = Uri::from_str(ws_url.as_ref())?;
    let host = uri
        .authority()
        .ok_or(TungsteniteError::Url(UrlError::NoHostName))?
        .as_str();

    let mut request = Request::builder()
        .method("GET")
        .header("Host", host)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", generate_key())
        .header("User-Agent", get_chrome_rua());

    if protocols.len() > 0 {
        request = request.header("Sec-Websocket-Protocol", protocols.join(", "));
    }

    let request = request.uri(uri).body(())?;

    Ok(request)
}
