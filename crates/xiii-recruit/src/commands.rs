pub const RECRUITS_COMMAND: &str = "/recruits";
pub const RECRUIT_PANEL_COMMAND: &str = "/recruit-panel";

pub fn command_allowed_in_channel(current_channel_id: u64, decision_channel_id: u64) -> bool {
    current_channel_id == decision_channel_id
}
