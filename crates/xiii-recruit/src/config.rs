#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecruitRuntimeConfig {
    pub recruit_role_id: u64,
    pub member_role_id: u64,
    pub guest_role_id: u64,
    pub next_rank_role_id: u64,
    pub decision_channel_id: u64,
    pub excluded_voice_channel_id: Option<u64>,
}
