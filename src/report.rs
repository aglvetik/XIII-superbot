#![allow(dead_code)]
//! Shared report rendering will be extracted here from `app.rs`.
//!
//! The current pass keeps the existing report functions in place to avoid
//! breaking the Clanlist writer, while new orchestration code continues to
//! use `xiii_core::Report`.

pub const REPORT_REFACTOR_NOTE: &str = "report rendering extraction pending";
