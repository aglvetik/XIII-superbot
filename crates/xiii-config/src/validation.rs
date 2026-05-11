use crate::SuperbotConfig;

pub fn vacation_roles_are_distinct(config: &SuperbotConfig) -> bool {
    config.vacation.vacation_role_id != config.voice_activity.vacation_marker_role_id
}
