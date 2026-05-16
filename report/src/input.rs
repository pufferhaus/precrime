//! USB keyboard input via evdev. Yields slot numbers 1..=9 on key press.

use anyhow::{Context, Result};
use evdev::{Device, EventType, Key};
use std::path::Path;
use std::sync::mpsc::Sender;

pub fn run_keyboard_loop(device_path: impl AsRef<Path>, tx: Sender<u8>) -> Result<()> {
    let path = device_path.as_ref();
    let mut device =
        Device::open(path).with_context(|| format!("opening evdev device {}", path.display()))?;

    loop {
        let events = device
            .fetch_events()
            .with_context(|| format!("fetch_events on {}", path.display()))?;
        for ev in events {
            if ev.event_type() != EventType::KEY {
                continue;
            }
            if ev.value() != 1 {
                continue;
            }
            if let Some(slot) = key_to_slot(Key::new(ev.code())) {
                let _ = tx.send(slot);
            }
        }
    }
}

fn key_to_slot(code: Key) -> Option<u8> {
    match code {
        Key::KEY_1 => Some(1),
        Key::KEY_2 => Some(2),
        Key::KEY_3 => Some(3),
        Key::KEY_4 => Some(4),
        Key::KEY_5 => Some(5),
        Key::KEY_6 => Some(6),
        Key::KEY_7 => Some(7),
        Key::KEY_8 => Some(8),
        Key::KEY_9 => Some(9),
        _ => None,
    }
}
