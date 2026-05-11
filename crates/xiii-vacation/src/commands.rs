pub const VACATIONS_DISABLED_RESPONSE: &str =
    "Функционал команды отключён. Список активных отпусков можно посмотреть тут: <#{channel_id}>.";

pub fn validate_vacation_role_split(
    vacation_role_id: u64,
    voice_marker_role_id: u64,
) -> Result<(), String> {
    if vacation_role_id == voice_marker_role_id {
        Err("VACATION_ROLE_ID must not equal VOICE_VACATION_MARKER_ROLE_ID".to_owned())
    } else {
        Ok(())
    }
}
