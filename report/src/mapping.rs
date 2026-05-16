//! Map discovered NDI source names to keyboard slots 1..=9.

use std::collections::{BTreeSet, HashMap};
use tracing::warn;

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
    // When two overrides target the same slot, the alphabetically-first source
    // name wins; the other falls back to the alphabetical pass.
    let mut sorted_overrides: Vec<(&String, &u8)> = overrides.iter().collect();
    sorted_overrides.sort_by(|(a, _), (b, _)| a.as_str().cmp(b.as_str()));
    for (name, slot) in sorted_overrides {
        if !sources.iter().any(|s| s == name) {
            warn!(source = %name, "override dropped: source not present in input list");
            continue;
        }
        if *slot < 1 || *slot > MAX_SLOTS {
            warn!(source = %name, slot, max = MAX_SLOTS, "override dropped: slot out of range");
            continue;
        }
        if taken.insert(*slot) {
            mapping.insert(name.clone(), *slot);
        } else {
            warn!(source = %name, slot, "override dropped: slot already taken by alphabetically-prior name");
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
