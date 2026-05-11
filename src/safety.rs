#![allow(dead_code)]
//! Path-safety and redaction helpers currently live in `app.rs` and
//! `xiii-config`.
//!
//! This module is reserved for shared command safety utilities once the
//! remaining module cutover commands stop changing shape.

pub const SAFETY_REFACTOR_NOTE: &str = "safety helper extraction pending";
