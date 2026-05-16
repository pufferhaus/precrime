//! Titler subsystem — character generator that burns title pages over the program out.
//!
//! T1 scope: load one TOML page, render to RGBA via Cairo, hand to the custom
//! `titleroverlay` GStreamer element which composites over program video.

pub mod page;
pub mod render;
// pub mod element;  // added in Task 3
