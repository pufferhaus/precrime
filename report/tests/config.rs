use report::config::ReportConfig;

#[test]
fn parses_minimal_config() {
    let raw = r#"
        program_connector_id = 32
        preview_connector_id = 34
        keyboard_device = "/dev/input/event0"
    "#;
    let cfg = ReportConfig::from_toml(raw).expect("parse");
    assert_eq!(cfg.program_connector_id, 32);
    assert_eq!(cfg.preview_connector_id, 34);
    assert_eq!(cfg.keyboard_device, "/dev/input/event0");
    assert!(cfg.source_slot_overrides.is_empty());
}

#[test]
fn parses_source_slot_overrides() {
    let raw = r#"
        program_connector_id = 32
        preview_connector_id = 34
        keyboard_device = "/dev/input/event0"

        [source_slot_overrides]
        "PRECOG-01-IPHONE-STAGE" = 1
        "PRECOG-02-CCTV-DOOR" = 2
    "#;
    let cfg = ReportConfig::from_toml(raw).expect("parse");
    assert_eq!(cfg.source_slot_overrides.get("PRECOG-01-IPHONE-STAGE"), Some(&1));
    assert_eq!(cfg.source_slot_overrides.get("PRECOG-02-CCTV-DOOR"), Some(&2));
}

#[test]
fn missing_required_field_errors() {
    let raw = r#"
        preview_connector_id = 34
        keyboard_device = "/dev/input/event0"
    "#;
    assert!(ReportConfig::from_toml(raw).is_err());
}
