mod dash_mpd_rs;
mod r#static;

use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

async fn setup_mock_server(body: &str) -> (String, MockServer) {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/manifest.mpd"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&mock_server)
        .await;

    (format!("{}/manifest.mpd", mock_server.uri()), mock_server)
}
