// crates/iori/tests/fixtures/hls/m3u8-rs/media-playlist-with-byterange.m3u8

use iori::{hls::HlsPlaylistSource, ByteRange, HttpClient};

use crate::hls::setup_mock_server;

#[tokio::test]
async fn media_playlist_with_byterange() -> anyhow::Result<()> {
    let data = include_str!("../fixtures/hls/m3u8-rs/media-playlist-with-byterange.m3u8");
    let (playlist_uri, server) = setup_mock_server(data).await;

    let client = HttpClient::default();
    let mut playlist = HlsPlaylistSource::new(client, playlist_uri.parse()?, None);

    let latest_media_sequences = playlist.load_streams(1).await?;
    let (streams, is_end) = playlist.load_segments(&latest_media_sequences, 1).await?;

    assert!(!is_end);
    assert_eq!(streams.len(), 1);

    let segments = &streams[0];
    assert_eq!(segments.len(), 3);

    let segment = &segments[0];
    assert_eq!(segment.url, format!("{}/video.ts", server.uri()).parse()?);
    assert_eq!(segment.byte_range, Some(ByteRange::new(0, Some(75232))));

    let segment = &segments[1];
    assert_eq!(segment.url, format!("{}/video.ts", server.uri()).parse()?);
    assert_eq!(
        segment.byte_range,
        Some(ByteRange::new(752321, Some(82112)))
    );

    let segment = &segments[2];
    assert_eq!(segment.url, format!("{}/video.ts", server.uri()).parse()?);
    assert_eq!(
        segment.byte_range,
        Some(ByteRange::new(834433, Some(69864)))
    );

    Ok(())
}

#[tokio::test]
async fn mediaplaylist_byterange() -> anyhow::Result<()> {
    let data = include_str!("../fixtures/hls/m3u8-rs/mediaplaylist-byterange.m3u8");
    let (playlist_uri, server) = setup_mock_server(data).await;

    let client = HttpClient::default();
    let mut playlist = HlsPlaylistSource::new(client, playlist_uri.parse()?, None);

    let latest_media_sequences = playlist.load_streams(1).await?;
    let (streams, is_end) = playlist.load_segments(&latest_media_sequences, 1).await?;

    assert!(is_end);
    assert_eq!(streams.len(), 1);

    let segments = &streams[0];
    assert_eq!(segments.len(), 8);

    let segment = &segments[0];
    assert_eq!(segment.url, format!("{}/main.aac", server.uri()).parse()?);
    assert_eq!(segment.byte_range, Some(ByteRange::new(0, Some(86920))));

    let segment = &segments[1];
    assert_eq!(segment.url, format!("{}/main.aac", server.uri()).parse()?);
    assert_eq!(
        segment.byte_range,
        Some(ByteRange::new(86920, Some(136595)))
    );

    let segment = &segments[2];
    assert_eq!(segment.url, format!("{}/main.aac", server.uri()).parse()?);
    assert_eq!(
        segment.byte_range,
        Some(ByteRange::new(223515, Some(136567)))
    );

    let segment = &segments[3];
    assert_eq!(segment.url, format!("{}/main.aac", server.uri()).parse()?);
    assert_eq!(
        segment.byte_range,
        Some(ByteRange::new(360082, Some(136954)))
    );

    let segment = &segments[4];
    assert_eq!(segment.url, format!("{}/main.aac", server.uri()).parse()?);
    assert_eq!(
        segment.byte_range,
        Some(ByteRange::new(497036, Some(137116)))
    );

    let segment = &segments[5];
    assert_eq!(segment.url, format!("{}/main.aac", server.uri()).parse()?);
    assert_eq!(
        segment.byte_range,
        Some(ByteRange::new(634152, Some(136770)))
    );

    let segment = &segments[6];
    assert_eq!(segment.url, format!("{}/main.aac", server.uri()).parse()?);
    assert_eq!(
        segment.byte_range,
        Some(ByteRange::new(770922, Some(137219)))
    );

    let segment = &segments[7];
    assert_eq!(segment.url, format!("{}/main.aac", server.uri()).parse()?);
    assert_eq!(
        segment.byte_range,
        Some(ByteRange::new(908141, Some(137132)))
    );

    Ok(())
}
