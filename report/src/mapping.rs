//! Map discovered NDI source names to keyboard slots 1..=9.

use std::collections::{BTreeSet, HashMap};

pub const MAX_SLOTS: u8 = 9;

/// Return `{source_name -> slot}`. Overrides pin names to specific slots;
/// the remainder fill the lowest free slots alphabetically.
///
/// Sources beyond the available slots (`MAX_SLOTS`) are dropped.
pub fn assign_slots(
    sources: &[String],
    overrides: &HashMap<String, u8>,
) -> HashMap<String, u8> {
    let mut mapping = HashMap::new();
    let mut taken = BTreeSet::new();

    // Apply pinned overrides first.
    for (name, slot) in overrides {
        if !sources.iter().any(|s| s == name) {
            continue;
        }
        if *slot < 1 || *slot > MAX_SLOTS {
            continue;
        }
        if taken.insert(*slot) {
            mapping.insert(name.clone(), *slot);
        }
    }

    // Fill remaining sources alphabetically into the lowest free slots.
    let mut remaining: Vec<&String> = sources
        .iter()
        .filter(|s| !mapping.contains_key(*s))
        .collect();
    remaining.sort();

    let mut free_slot: u8 = 1;
    for name in remaining {
        while free_slot <= MAX_SLOTS && taken.contains(&free_slot) {
            free_slot += 1;
        }
        if free_slot > MAX_SLOTS {
            break;
        }
        mapping.insert(name.clone(), free_slot);
        taken.insert(free_slot);
        free_slot += 1;
    }

    mapping
}
