mod m3u8_rs;
mod rfc8216;

use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

async fn setup_mock_server(body: &str) -> (String, MockServer) {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/playlist.m3u8"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&mock_server)
        .await;

    (format!("{}/playlist.m3u8", mock_server.uri()), mock_server)
}

trait HlsMock {
    async fn mock<S>(&self, mock_path: &str, body: S) -> &Self
    where
        S: AsRef<str>;

    async fn mock_playlist(&self, mock_path: &str, url: &str) -> &Self;
}

impl HlsMock for MockServer {
    async fn mock<S>(&self, mock_path: &str, body: S) -> &Self
    where
        S: AsRef<str>,
    {
        Mock::given(method("GET"))
            .and(path(mock_path))
            .respond_with(ResponseTemplate::new(200).set_body_string(body.as_ref()))
            .mount(self)
            .await;
        self
    }

    async fn mock_playlist(&self, mock_path: &str, url: &str) -> &Self {
        self.mock(
            mock_path,
            format!(
                "#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-VERSION:3
#EXTINF:9.009,
{url}
#EXT-X-ENDLIST"
            ),
        )
        .await
    }
}
