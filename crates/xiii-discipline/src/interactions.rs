pub const PANEL_ISSUE: &str = "xiii:panel:issue";
pub const PANEL_REMOVE: &str = "xiii:panel:remove";
pub const PANEL_HISTORY: &str = "xiii:panel:history";
pub const BOARD_PREV: &str = "xiii:board:page:prev";
pub const BOARD_NEXT: &str = "xiii:board:page:next";

pub fn issue_member_select_id(session_id: &str) -> String {
    format!("xiii:issue:member:{session_id}")
}

pub fn issue_picker_button_id(session_id: &str, action: &str) -> String {
    format!("xiii:issue:picker:{session_id}:{action}")
}

pub fn issue_id_modal_id(session_id: &str) -> String {
    format!("xiii:issue:idmodal:{session_id}")
}

pub fn issue_type_select_id(issuer_id: u64, target_id: u64) -> String {
    format!("xiii:issue:type:{issuer_id}:{target_id}")
}

pub fn issue_modal_id(issuer_id: u64, target_id: u64, punishment_type: &str) -> String {
    format!("xiii:issue:modal:{issuer_id}:{target_id}:{punishment_type}")
}

pub fn remove_user_select_id(issuer_id: u64) -> String {
    format!("xiii:remove:user:{issuer_id}")
}

pub fn remove_punishment_select_id(issuer_id: u64, target_id: u64) -> String {
    format!("xiii:remove:punishment:{issuer_id}:{target_id}")
}

pub fn remove_modal_id(issuer_id: u64, target_id: u64, punishment_id: i64) -> String {
    format!("xiii:remove:modal:{issuer_id}:{target_id}:{punishment_id}")
}

pub fn history_user_select_id(issuer_id: u64) -> String {
    format!("xiii:history:user:{issuer_id}")
}
