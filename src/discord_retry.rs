#![allow(dead_code)]
//! Discord HTTP retry helpers currently live in `app.rs`.
//!
//! They are intentionally kept with the Clanlist HTTP code until the shared
//! Discord IO crate takes ownership of all module HTTP calls.

pub const DISCORD_RETRY_REFACTOR_NOTE: &str = "Discord retry extraction pending";
