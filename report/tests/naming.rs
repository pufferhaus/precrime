use report::naming::{display_name, is_precog_source};

#[test]
fn accepts_precog_named_sources() {
    assert!(is_precog_source("PRECOG-01-IPHONE-STAGE (Channel 1)"));
    assert!(is_precog_source("PRECOG-02-CCTV-DOOR"));
}

#[test]
fn rejects_non_precog_sources() {
    assert!(!is_precog_source("REPORT (Internal)"));
    assert!(!is_precog_source("Random Studio Source"));
    assert!(!is_precog_source(""));
}

#[test]
fn strips_channel_suffix() {
    assert_eq!(
        display_name("PRECOG-01-IPHONE-STAGE (Channel 1)"),
        "PRECOG-01-IPHONE-STAGE"
    );
    assert_eq!(display_name("PRECOG-02-CCTV-DOOR"), "PRECOG-02-CCTV-DOOR");
}
