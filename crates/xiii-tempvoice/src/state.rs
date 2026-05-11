#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubSettings {
    pub guild_id: u64,
    pub hub_channel_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempVoiceChannel {
    pub channel_id: u64,
    pub guild_id: u64,
    pub owner_user_id: u64,
    pub member_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TempVoiceAction {
    CreateRoom {
        hub_channel_id: u64,
        owner_user_id: u64,
    },
    DeleteOwnedChannel {
        channel_id: u64,
    },
    ScheduleDeleteOwnedChannel {
        channel_id: u64,
        delay_seconds: u64,
    },
    Ignore,
}

pub fn user_joined_hub(
    hub: &HubSettings,
    user_id: u64,
    channel_id: Option<u64>,
) -> TempVoiceAction {
    if channel_id == Some(hub.hub_channel_id) {
        TempVoiceAction::CreateRoom {
            hub_channel_id: hub.hub_channel_id,
            owner_user_id: user_id,
        }
    } else {
        TempVoiceAction::Ignore
    }
}

pub fn empty_owned_channel_action(
    tracked: &[TempVoiceChannel],
    channel_id: u64,
    delete_after_seconds: u64,
) -> TempVoiceAction {
    let Some(channel) = tracked
        .iter()
        .find(|channel| channel.channel_id == channel_id)
    else {
        return TempVoiceAction::Ignore;
    };
    if channel.member_count != 0 {
        return TempVoiceAction::Ignore;
    }
    if delete_after_seconds == 0 {
        return TempVoiceAction::DeleteOwnedChannel { channel_id };
    }
    TempVoiceAction::ScheduleDeleteOwnedChannel {
        channel_id,
        delay_seconds: delete_after_seconds,
    }
}
