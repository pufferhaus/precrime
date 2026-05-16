use report::mapping::{assign_slots, MAX_SLOTS};
use std::collections::HashMap;

#[test]
fn alphabetical_order_assigns_slots() {
    let sources = vec![
        "PRECOG-02-CCTV-DOOR".to_string(),
        "PRECOG-01-IPHONE-STAGE".to_string(),
    ];
    let mapping = assign_slots(&sources, &HashMap::new());
    assert_eq!(mapping.get("PRECOG-01-IPHONE-STAGE"), Some(&1));
    assert_eq!(mapping.get("PRECOG-02-CCTV-DOOR"), Some(&2));
}

#[test]
fn overrides_pin_specific_sources() {
    let sources = vec![
        "PRECOG-A".to_string(),
        "PRECOG-B".to_string(),
        "PRECOG-C".to_string(),
    ];
    let mut overrides = HashMap::new();
    overrides.insert("PRECOG-C".to_string(), 1u8);
    let mapping = assign_slots(&sources, &overrides);
    assert_eq!(mapping.get("PRECOG-C"), Some(&1));
    assert_eq!(mapping.get("PRECOG-A"), Some(&2));
    assert_eq!(mapping.get("PRECOG-B"), Some(&3));
}

#[test]
fn extras_beyond_max_slots_dropped() {
    let sources: Vec<String> = (1..=11).map(|i| format!("PRECOG-{i:02}")).collect();
    let mapping = assign_slots(&sources, &HashMap::new());
    assert_eq!(mapping.len(), usize::from(MAX_SLOTS));
    assert_eq!(mapping.get("PRECOG-01"), Some(&1));
    assert_eq!(mapping.get("PRECOG-09"), Some(&9));
    assert!(mapping.get("PRECOG-10").is_none());
}

#[test]
fn empty_input_returns_empty_mapping() {
    assert!(assign_slots(&[], &HashMap::new()).is_empty());
}
