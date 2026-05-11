#![allow(dead_code)]
//! Old-service status parsing currently lives in `app.rs`.
//!
//! Keeping it there for this pass avoids changing tested Clanlist update
//! behavior while still documenting the target module boundary.

pub const SERVICE_GUARD_REFACTOR_NOTE: &str = "service guard extraction pending";
