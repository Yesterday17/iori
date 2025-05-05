use iori::{SegmentFormat, SegmentType};

#[test]
fn test_segment_format_from_filename() {
    assert_eq!(
        SegmentFormat::from_filename("test.ts"),
        SegmentFormat::Mpeg2TS
    );
    assert_eq!(SegmentFormat::from_filename("test.mp4"), SegmentFormat::Mp4);
    assert_eq!(SegmentFormat::from_filename("test.m4a"), SegmentFormat::M4a);
    assert_eq!(
        SegmentFormat::from_filename("test.cmfv"),
        SegmentFormat::Cmfv
    );
    assert_eq!(
        SegmentFormat::from_filename("test.cmfa"),
        SegmentFormat::Cmfa
    );
    assert_eq!(
        SegmentFormat::from_filename("test.txt"),
        SegmentFormat::Raw("txt".to_string())
    );
    assert_eq!(
        SegmentFormat::from_filename("test.unknown"),
        SegmentFormat::Other("unknown".to_string())
    );
}

#[test]
fn test_segment_format_as_ext() {
    assert_eq!(SegmentFormat::Mpeg2TS.as_ext(), "ts");
    assert_eq!(SegmentFormat::Mp4.as_ext(), "mp4");
    assert_eq!(SegmentFormat::M4a.as_ext(), "m4a");
    assert_eq!(SegmentFormat::Cmfv.as_ext(), "cmfv");
    assert_eq!(SegmentFormat::Cmfa.as_ext(), "cmfa");
    assert_eq!(SegmentFormat::Raw("txt".to_string()).as_ext(), "txt");
    assert_eq!(
        SegmentFormat::Other("unknown".to_string()).as_ext(),
        "unknown"
    );
}

#[test]
fn test_segment_type_from_mime_type() {
    assert_eq!(
        SegmentType::from_mime_type(Some("video/mp4")),
        SegmentType::Video
    );
    assert_eq!(
        SegmentType::from_mime_type(Some("audio/mp4")),
        SegmentType::Audio
    );
    assert_eq!(
        SegmentType::from_mime_type(Some("text/vtt")),
        SegmentType::Subtitle
    );
    assert_eq!(SegmentType::from_mime_type(None), SegmentType::Video);
}
