use crate::state::{VacationRecord, VacationStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VacationDecision {
    Approve {
        request_id: i64,
        add_role_id: u64,
    },
    Reject {
        request_id: i64,
    },
    EarlyEnd {
        vacation_id: i64,
        remove_role_id: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpiryAction {
    Expire {
        vacation_id: i64,
        remove_role_id: u64,
    },
    Ignore,
}

pub fn approve_request(request_id: i64, vacation_role_id: u64) -> VacationDecision {
    VacationDecision::Approve {
        request_id,
        add_role_id: vacation_role_id,
    }
}

pub fn reject_request(request_id: i64) -> VacationDecision {
    VacationDecision::Reject { request_id }
}

pub fn early_end_vacation(vacation_id: i64, vacation_role_id: u64) -> VacationDecision {
    VacationDecision::EarlyEnd {
        vacation_id,
        remove_role_id: vacation_role_id,
    }
}

pub fn expiry_action(record: &VacationRecord, now_unix: i64) -> ExpiryAction {
    if record.status == VacationStatus::Active && record.expected_end_unix <= now_unix {
        ExpiryAction::Expire {
            vacation_id: record.id,
            remove_role_id: record.role_id,
        }
    } else {
        ExpiryAction::Ignore
    }
}
