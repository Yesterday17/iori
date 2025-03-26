// #[test]
// fn test_a2d_tv() {
//     let mut a2d_tv = M3u8Source::new(
//         Arc::new(Client::new()),
//         "https://a2d.tv/hls/1/index.m3u8".to_string(),
//         None,
//         None,
//     );

//     let (segments, playlist_url, playlist) = block_on(a2d_tv.load_segments(None)).unwrap();
//     assert_eq!(segments.len(), 0);
//     assert_eq!(
//         playlist_url,
//         Url::parse("https://a2d.tv/hls/1/index.m3u8").unwrap()
//     );
//     assert_eq!(playlist.media_sequence, 0);
// }
