#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceActivityRuntimeConfig {
    pub ignored_channel_ids: Vec<u64>,
    pub vacation_marker_role_id: u64,
    pub heartbeat_interval_seconds: u64,
    pub public_stats_update_interval_seconds: u64,
}
