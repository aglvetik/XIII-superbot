#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupVoiceHubValidation {
    Allowed,
    Denied(&'static str),
}

pub fn validate_setup_voice_hub(
    is_owner: bool,
    has_administrator: bool,
    target_is_voice_channel: bool,
) -> SetupVoiceHubValidation {
    if !(is_owner || has_administrator) {
        return SetupVoiceHubValidation::Denied(
            "only server owner or administrators can set the hub",
        );
    }
    if !target_is_voice_channel {
        return SetupVoiceHubValidation::Denied("target channel must be a voice channel");
    }
    SetupVoiceHubValidation::Allowed
}
