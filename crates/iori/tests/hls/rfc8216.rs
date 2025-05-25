use crate::hls::{setup_mock_server, HlsMock};
use iori::{hls::HlsPlaylistSource, HttpClient};

#[tokio::test]
async fn rfc8216_8_1_simple_media_playlist() -> anyhow::Result<()> {
    let data = include_str!("../fixtures/hls/rfc8216/8-1-simple-media-playlist.m3u8");
    let (uri, _server) = setup_mock_server(data).await;

    let client = HttpClient::default();
    let mut playlist = HlsPlaylistSource::new(client, uri.parse()?, None);

    let latest_media_sequences = playlist.load_streams(1).await?;
    let (streams, is_end) = playlist.load_segments(&latest_media_sequences, 1).await?;

    assert!(is_end);
    assert_eq!(streams.len(), 1);

    let segments = &streams[0];
    assert_eq!(segments.len(), 3);
    assert_eq!(
        segments[0].url,
        "http://media.example.com/first.ts".parse()?
    );
    assert_eq!(segments[0].sequence, 0);

    assert_eq!(
        segments[1].url,
        "http://media.example.com/second.ts".parse()?
    );
    assert_eq!(segments[1].sequence, 1);

    assert_eq!(
        segments[2].url,
        "http://media.example.com/third.ts".parse()?
    );
    assert_eq!(segments[2].sequence, 2);

    Ok(())
}

#[tokio::test]
async fn rfc8216_8_2_live_media_playlist_using_https() -> anyhow::Result<()> {
    let data = include_str!("../fixtures/hls/rfc8216/8-2-live-media-playlist-using-https.m3u8");
    let (uri, _server) = setup_mock_server(data).await;

    let client = HttpClient::default();
    let mut playlist = HlsPlaylistSource::new(client, uri.parse()?, None);

    let latest_media_sequences = playlist.load_streams(1).await?;
    let (streams, is_end) = playlist.load_segments(&latest_media_sequences, 1).await?;

    assert!(!is_end);
    assert_eq!(streams.len(), 1);

    let segments = &streams[0];
    assert_eq!(segments.len(), 3);

    assert_eq!(
        segments[0].url,
        "https://priv.example.com/fileSequence2680.ts".parse()?
    );
    assert_eq!(segments[0].sequence, 0);
    assert_eq!(segments[0].media_sequence, 2680);

    assert_eq!(
        segments[1].url,
        "https://priv.example.com/fileSequence2681.ts".parse()?
    );
    assert_eq!(segments[1].sequence, 1);
    assert_eq!(segments[1].media_sequence, 2681);

    assert_eq!(
        segments[2].url,
        "https://priv.example.com/fileSequence2682.ts".parse()?
    );
    assert_eq!(segments[2].sequence, 2);
    assert_eq!(segments[2].media_sequence, 2682);

    Ok(())
}

#[tokio::test]
async fn rfc8216_8_3_playlist_with_encrypted_media_segments() -> anyhow::Result<()> {
    let data =
        include_str!("../fixtures/hls/rfc8216/8-3-playlist-with-encrypted-media-segments.m3u8");
    let (uri, _server) = setup_mock_server(data).await;

    let client = HttpClient::default();
    let mut playlist = HlsPlaylistSource::new(
        client,
        uri.parse()?,
        Some("1234567890abcdef1234567890abcdef"), // mocked key
    );

    let latest_media_sequences = playlist.load_streams(1).await?;
    let (streams, is_end) = playlist.load_segments(&latest_media_sequences, 1).await?;

    assert!(!is_end);
    assert_eq!(streams.len(), 1);

    let segments = &streams[0];
    assert_eq!(segments.len(), 4);

    assert_eq!(
        segments[0].url,
        "http://media.example.com/fileSequence52-A.ts".parse()?
    );
    assert_eq!(segments[0].sequence, 0);
    assert_eq!(segments[0].media_sequence, 7794);

    assert_eq!(
        segments[1].url,
        "http://media.example.com/fileSequence52-B.ts".parse()?
    );
    assert_eq!(segments[1].sequence, 1);
    assert_eq!(segments[1].media_sequence, 7795);

    assert_eq!(
        segments[2].url,
        "http://media.example.com/fileSequence52-C.ts".parse()?
    );
    assert_eq!(segments[2].sequence, 2);
    assert_eq!(segments[2].media_sequence, 7796);

    assert_eq!(
        segments[3].url,
        "http://media.example.com/fileSequence53-A.ts".parse()?
    );
    assert_eq!(segments[3].sequence, 3);
    assert_eq!(segments[3].media_sequence, 7797);

    assert!(segments[0].key.is_some());
    assert!(segments[1].key.is_some());
    assert!(segments[2].key.is_some());
    assert!(segments[3].key.is_some());

    Ok(())
}

#[tokio::test]
async fn rfc8216_8_4_master_playlist() -> anyhow::Result<()> {
    let data = include_str!("../fixtures/hls/rfc8216/8-4-master-playlist.m3u8");
    let (uri, server) = setup_mock_server(data).await;
    server
        .mock_playlist("/low.m3u8", "http://media.example.com/low.ts")
        .await
        .mock_playlist("/mid.m3u8", "http://media.example.com/mid.ts")
        .await
        .mock_playlist("/hi.m3u8", "http://media.example.com/hi.ts")
        .await
        .mock_playlist("/audio-only.m3u8", "http://media.example.com/audio-only.ts")
        .await;

    let client = HttpClient::default();
    let mut playlist = HlsPlaylistSource::new(client, uri.parse()?, None);

    let latest_media_sequences = playlist.load_streams(1).await?;
    let (streams, is_end) = playlist.load_segments(&latest_media_sequences, 1).await?;

    assert!(is_end);
    assert_eq!(streams.len(), 1);

    let segments = &streams[0];
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].url, "http://media.example.com/hi.ts".parse()?);
    assert_eq!(segments[0].sequence, 0);
    assert_eq!(segments[0].media_sequence, 0);

    Ok(())
}

#[tokio::test]
async fn rfc8216_8_6_master_playlist_with_alternative_audio() -> anyhow::Result<()> {
    let data =
        include_str!("../fixtures/hls/rfc8216/8-6-master-playlist-with-alternative-audio.m3u8");
    let (uri, server) = setup_mock_server(data).await;
    server
        .mock_playlist(
            "/main/english-audio.m3u8",
            "http://media.example.com/english-audio.ts",
        )
        .await
        .mock_playlist(
            "/main/german-audio.m3u8",
            "http://media.example.com/german-audio.ts",
        )
        .await
        .mock_playlist(
            "/commentary/audio-only.m3u8",
            "http://media.example.com/commentary.ts",
        )
        .await
        .mock_playlist(
            "/low/video-only.m3u8",
            "http://media.example.com/video-low.ts",
        )
        .await
        .mock_playlist(
            "/mid/video-only.m3u8",
            "http://media.example.com/video-mid.ts",
        )
        .await
        .mock_playlist(
            "/hi/video-only.m3u8",
            "http://media.example.com/video-hi.ts",
        )
        .await;

    let client = HttpClient::default();
    let mut playlist = HlsPlaylistSource::new(client, uri.parse()?, None);

    let latest_media_sequences = playlist.load_streams(1).await?;
    let (streams, is_end) = playlist.load_segments(&latest_media_sequences, 1).await?;

    assert!(is_end);
    assert_eq!(streams.len(), 2);

    let segments = &streams[0];
    assert_eq!(segments.len(), 1);
    assert_eq!(
        segments[0].url,
        "http://media.example.com/video-hi.ts".parse()?
    );
    assert_eq!(segments[0].sequence, 0);
    assert_eq!(segments[0].media_sequence, 0);

    let segments = &streams[1];
    assert_eq!(segments.len(), 1);
    assert_eq!(
        segments[0].url,
        "http://media.example.com/english-audio.ts".parse()?
    );
    Ok(())
}

#[tokio::test]
async fn rfc8216_8_7_master_playlist_with_alternative_video() -> anyhow::Result<()> {
    let data =
        include_str!("../fixtures/hls/rfc8216/8-7-master-playlist-with-alternative-video.m3u8");
    let (uri, server) = setup_mock_server(data).await;
    server
        .mock_playlist(
            "/hi/main/audio-video.m3u8",
            r#"http://media.example.com/video-hi.ts"#,
        )
        .await;

    let client = HttpClient::default();
    let mut playlist = HlsPlaylistSource::new(client, uri.parse()?, None);

    let latest_media_sequences = playlist.load_streams(1).await?;
    let (streams, is_end) = playlist.load_segments(&latest_media_sequences, 1).await?;

    assert!(is_end);
    assert_eq!(streams.len(), 1);

    let segments = &streams[0];
    assert_eq!(segments.len(), 1);
    assert_eq!(
        segments[0].url,
        "http://media.example.com/video-hi.ts".parse()?
    );
    assert_eq!(segments[0].sequence, 0);
    assert_eq!(segments[0].media_sequence, 0);

    Ok(())
}
