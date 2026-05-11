#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VacationRequestStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VacationStatus {
    Active,
    Ended,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacationRecord {
    pub id: i64,
    pub user_id: u64,
    pub role_id: u64,
    pub status: VacationStatus,
    pub started_unix: i64,
    pub expected_end_unix: i64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacationRequestDraft {
    pub user_id: u64,
    pub start_unix: i64,
    pub expected_end_unix: i64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacationPanelState {
    pub request_panel_message_id: u64,
    pub active_panel_message_id: u64,
}

pub fn has_active_vacation(records: &[VacationRecord], user_id: u64) -> bool {
    records
        .iter()
        .any(|record| record.user_id == user_id && record.status == VacationStatus::Active)
}

pub fn validate_new_request(
    active_records: &[VacationRecord],
    draft: &VacationRequestDraft,
    max_days: u64,
) -> Result<(), String> {
    if has_active_vacation(active_records, draft.user_id) {
        return Err("user already has an active vacation".to_owned());
    }
    if draft.expected_end_unix <= draft.start_unix {
        return Err("expected end must be after start".to_owned());
    }
    let max_seconds = max_days.saturating_mul(86_400) as i64;
    if draft.expected_end_unix - draft.start_unix > max_seconds {
        return Err("requested vacation exceeds maximum duration".to_owned());
    }
    Ok(())
}
