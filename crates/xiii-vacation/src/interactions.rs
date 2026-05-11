pub const APPLY_BUTTON_ID: &str = "vacation:apply";
pub const REQUEST_MODAL_ID: &str = "vacation:modal";
pub const END_BUTTON_PREFIX: &str = "vacation:end:";
pub const END_CONFIRM_PREFIX: &str = "vacation:end_confirm:";
pub const END_CANCEL_PREFIX: &str = "vacation:end_cancel:";

pub fn approve_button_id(request_id: i64) -> String {
    format!("vacation:approve:{request_id}")
}

pub fn reject_button_id(request_id: i64) -> String {
    format!("vacation:reject:{request_id}")
}

pub fn end_button_id(vacation_id: i64) -> String {
    format!("{END_BUTTON_PREFIX}{vacation_id}")
}

pub fn end_confirm_button_id(vacation_id: i64) -> String {
    format!("{END_CONFIRM_PREFIX}{vacation_id}")
}

pub fn end_cancel_button_id(vacation_id: i64) -> String {
    format!("{END_CANCEL_PREFIX}{vacation_id}")
}
