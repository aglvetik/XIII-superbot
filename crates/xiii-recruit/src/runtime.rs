use crate::state::{
    Recruit, RecruitDecision, RecruitDecisionPanel, RecruitStatus, RecruitVoiceSession,
};

pub fn due_recruits(recruits: &[Recruit], now_unix: i64) -> Vec<Recruit> {
    recruits
        .iter()
        .filter(|recruit| recruit.status == RecruitStatus::Active && recruit.due_unix <= now_unix)
        .cloned()
        .collect()
}

pub fn should_ping_decision_roles(is_automatic_due_panel: bool) -> bool {
    is_automatic_due_panel
}

pub fn voice_channel_is_tracked(channel_id: u64, excluded_channel_id: Option<u64>) -> bool {
    Some(channel_id) != excluded_channel_id
}

pub fn next_status(decision: &RecruitDecision) -> RecruitStatus {
    match decision {
        RecruitDecision::Accept => RecruitStatus::Accepted,
        RecruitDecision::Reject { .. } => RecruitStatus::Rejected,
        RecruitDecision::Extend { .. } => RecruitStatus::Extended,
    }
}

pub fn voice_duration_seconds(session: &RecruitVoiceSession, left_unix: i64) -> u64 {
    left_unix.saturating_sub(session.joined_unix).max(0) as u64
}

pub fn should_send_due_panel(
    recruit: &Recruit,
    existing_panels: &[RecruitDecisionPanel],
    now_unix: i64,
) -> bool {
    recruit.status == RecruitStatus::Active
        && recruit.due_unix <= now_unix
        && !existing_panels
            .iter()
            .any(|panel| panel.recruit_id == recruit.id && panel.automatic)
}
