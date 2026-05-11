#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecruitStatus {
    Active,
    Accepted,
    Rejected,
    Extended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recruit {
    pub id: i64,
    pub guild_id: u64,
    pub user_id: u64,
    pub status: RecruitStatus,
    pub due_unix: i64,
    pub last_decision_message_id: Option<u64>,
    pub last_decision_channel_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecruitDecision {
    Accept,
    Reject { reason: String },
    Extend { days: u64, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitVoiceSession {
    pub recruit_id: i64,
    pub user_id: u64,
    pub channel_id: u64,
    pub joined_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitDecisionPanel {
    pub recruit_id: i64,
    pub channel_id: u64,
    pub message_id: u64,
    pub automatic: bool,
}
