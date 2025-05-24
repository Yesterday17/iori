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
    async fn mock_get(&self, mock_path: &str, body: &str) -> &Self;
}

impl HlsMock for MockServer {
    async fn mock_get(&self, mock_path: &str, body: &str) -> &Self {
        Mock::given(method("GET"))
            .and(path(mock_path))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(self)
            .await;
        self
    }
}
