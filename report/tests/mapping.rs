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

#[test]
fn two_overrides_same_slot_deterministic_winner() {
    let sources = vec!["PRECOG-B".to_string(), "PRECOG-A".to_string()];
    let mut overrides = HashMap::new();
    overrides.insert("PRECOG-A".to_string(), 1u8);
    overrides.insert("PRECOG-B".to_string(), 1u8);
    let mapping = assign_slots(&sources, &overrides);
    // PRECOG-A (alphabetically first) wins slot 1.
    assert_eq!(mapping.get("PRECOG-A"), Some(&1));
    // PRECOG-B falls into the alphabetical pass, gets slot 2.
    assert_eq!(mapping.get("PRECOG-B"), Some(&2));
}

#[test]
fn out_of_range_override_ignored() {
    let sources = vec!["PRECOG-A".to_string()];
    let mut overrides = HashMap::new();
    overrides.insert("PRECOG-A".to_string(), 0u8); // 0 is invalid
    let mapping_zero = assign_slots(&sources, &overrides);
    assert_eq!(mapping_zero.get("PRECOG-A"), Some(&1)); // falls through to alphabetical

    let mut overrides = HashMap::new();
    overrides.insert("PRECOG-A".to_string(), 10u8); // 10 > MAX_SLOTS
    let mapping_ten = assign_slots(&sources, &overrides);
    assert_eq!(mapping_ten.get("PRECOG-A"), Some(&1));
}
