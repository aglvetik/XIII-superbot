use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveVoiceSession {
    pub guild_id: u64,
    pub user_id: u64,
    pub channel_id: u64,
    pub started_at: String,
    pub last_seen_at: String,
    pub recovered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedVoiceSession {
    pub id: Option<i64>,
    pub guild_id: u64,
    pub user_id: u64,
    pub channel_id: u64,
    pub started_at: String,
    pub ended_at: String,
    pub duration_seconds: i64,
    pub close_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceCutoverClosedSession {
    pub guild_id: u64,
    pub user_id: u64,
    pub channel_id: u64,
    pub started_at: String,
    pub ended_at: String,
    pub duration_seconds: i64,
    pub completed_row_inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceCutoverCloseResult {
    pub cutover_at_utc: String,
    pub active_sessions_before: usize,
    pub closed_sessions: Vec<VoiceCutoverClosedSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceActivityCutoverState {
    pub source: String,
    pub policy: String,
    pub guild_id: u64,
    pub cutover_at_utc: String,
    pub active_sessions_before: usize,
    pub closed_sessions: Vec<VoiceCutoverClosedSession>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredVoiceUser {
    pub user_id: u64,
    pub display_name: String,
    pub username: Option<String>,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceSession {
    pub user_id: u64,
    pub channel_id: u64,
    pub started_unix: i64,
    pub ended_unix: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoicePanelState {
    pub source: String,
    pub guild_id: u64,
    pub bot_user_id: u64,
    pub public_stats_panel: VoicePanelTarget,
    pub created_at_utc: String,
    pub last_updated_at_utc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoicePanelTarget {
    pub channel_id: u64,
    pub message_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveVoiceMember {
    pub user_id: u64,
    pub channel_id: u64,
    pub display_name: String,
    pub username: Option<String>,
    pub is_bot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceMemberForReport {
    pub user_id: u64,
    pub display_name: String,
    pub role_ids: Vec<u64>,
    pub is_bot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderboardEntry {
    pub rank: usize,
    pub user_id: u64,
    pub display_name: String,
    pub total_seconds: i64,
    pub points: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InactiveEntry {
    pub rank: usize,
    pub user_id: u64,
    pub display_name: String,
    pub total_seconds: i64,
    pub required_seconds: i64,
    pub passed: bool,
    pub on_vacation: bool,
}
