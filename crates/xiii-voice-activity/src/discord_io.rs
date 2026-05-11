#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceStateSnapshot {
    pub user_id: u64,
    pub channel_id: Option<u64>,
}
