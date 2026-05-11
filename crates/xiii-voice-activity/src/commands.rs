pub fn inactive_check_allowed(channel_id: u64, allowed_channel_id: u64) -> bool {
    channel_id == allowed_channel_id
}

pub fn member_has_vacation_marker(role_ids: &[u64], vacation_marker_role_id: u64) -> bool {
    role_ids.contains(&vacation_marker_role_id)
}
