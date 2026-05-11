pub fn utc_now_rfc3339() -> String {
    // Keep core free of time dependencies for now; runtime crates use chrono directly.
    "runtime-provided".to_owned()
}
