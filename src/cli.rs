#![allow(dead_code)]
//! Clap definitions are still hosted in `app.rs` while the CLI is being
//! split without disturbing the proven Clanlist commands.
//!
//! This file exists as the public home for the next extraction: moving the
//! command enum out of `app.rs` once the remaining module cutover commands
//! are stable.

pub const CLI_REFACTOR_NOTE: &str = "CLI extraction pending after cutover command stabilization";
