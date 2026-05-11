use std::sync::Arc;
use twilight_http::error::{Error as TwilightHttpError, ErrorType as TwilightHttpErrorType};
use twilight_http::request::AuditLogReason;
use twilight_http::Client as DiscordHttpClient;
use twilight_model::channel::message::AllowedMentions;
use twilight_model::channel::{Channel, ChannelType};
use twilight_model::guild::Permissions;
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};
use twilight_model::id::marker::{
    ApplicationMarker, ChannelMarker, GuildMarker, InteractionMarker, UserMarker,
};
use twilight_model::id::Id;

const TEMP_CHANNEL_CREATE_REASON: &str = "Create temporary voice channel";
const TEMP_CHANNEL_MOVE_REASON: &str = "Move member to temporary voice channel";
const TEMP_CHANNEL_DELETE_REASON: &str = "Delete empty temporary voice channel";
const MAX_ROOM_NAME_CHARS: usize = 90;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTempRoomRequest {
    pub guild_id: u64,
    pub parent_or_hub_channel_id: u64,
    pub owner_user_id: u64,
    pub room_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempVoiceMemberContext {
    pub guild_id: u64,
    pub user_id: u64,
    pub display_name: String,
}

pub fn room_name(display_name: &str) -> String {
    let trimmed = display_name.trim();
    let base = if trimmed.is_empty() { "User" } else { trimmed };
    let mut name = format!("{base}'s room");
    if name.chars().count() > MAX_ROOM_NAME_CHARS {
        name = name.chars().take(MAX_ROOM_NAME_CHARS).collect();
    }
    name
}

pub fn is_regular_voice_channel(channel: &Channel) -> bool {
    channel.kind == ChannelType::GuildVoice
}

pub fn user_can_setup_hub(is_guild_owner: bool, permissions: Option<Permissions>) -> bool {
    is_guild_owner
        || permissions
            .map(|permissions| permissions.contains(Permissions::ADMINISTRATOR))
            .unwrap_or(false)
}

#[derive(Clone)]
pub struct TempVoiceDiscordHttp {
    client: Arc<DiscordHttpClient>,
}

impl TempVoiceDiscordHttp {
    pub fn new(client: Arc<DiscordHttpClient>) -> Self {
        Self { client }
    }

    pub async fn get_channel(&self, channel_id: u64) -> Result<Channel, String> {
        self.client
            .channel(Id::<ChannelMarker>::new(channel_id))
            .await
            .map_err(|err| format!("failed to fetch channel {channel_id}: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode channel {channel_id}: {err}"))
    }

    pub async fn get_channel_optional(&self, channel_id: u64) -> Result<Option<Channel>, String> {
        match self
            .client
            .channel(Id::<ChannelMarker>::new(channel_id))
            .await
        {
            Ok(response) => response
                .model()
                .await
                .map(Some)
                .map_err(|err| format!("failed to decode channel {channel_id}: {err}")),
            Err(err) if is_not_found(&err) => Ok(None),
            Err(err) => Err(format!("failed to fetch channel {channel_id}: {err}")),
        }
    }

    pub async fn create_voice_channel_from_hub(
        &self,
        guild_id: u64,
        hub: &Channel,
        name: &str,
    ) -> Result<Channel, String> {
        let guild_id = Id::<GuildMarker>::new(guild_id);
        let overwrites = hub.permission_overwrites.clone().unwrap_or_default();
        let rtc_region = hub.rtc_region.clone();
        let mut request = self
            .client
            .create_guild_channel(guild_id, name)
            .kind(ChannelType::GuildVoice)
            .permission_overwrites(&overwrites)
            .reason(TEMP_CHANNEL_CREATE_REASON);

        if let Some(parent_id) = hub.parent_id {
            request = request.parent_id(parent_id);
        }
        if let Some(bitrate) = hub.bitrate {
            request = request.bitrate(bitrate);
        }
        if let Some(user_limit) = hub.user_limit.and_then(|limit| u16::try_from(limit).ok()) {
            request = request.user_limit(user_limit);
        }
        if let Some(position) = hub
            .position
            .and_then(|position| u64::try_from(position + 1).ok())
        {
            request = request.position(position);
        }
        if let Some(region) = rtc_region.as_deref() {
            request = request.rtc_region(region);
        }

        request
            .await
            .map_err(|err| format!("failed to create temp voice channel: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode created temp voice channel: {err}"))
    }

    pub async fn move_member_to_channel(
        &self,
        guild_id: u64,
        user_id: u64,
        channel_id: u64,
    ) -> Result<(), String> {
        self.client
            .update_guild_member(
                Id::<GuildMarker>::new(guild_id),
                Id::<UserMarker>::new(user_id),
            )
            .channel_id(Some(Id::<ChannelMarker>::new(channel_id)))
            .reason(TEMP_CHANNEL_MOVE_REASON)
            .await
            .map_err(|err| {
                format!("failed to move user {user_id} to channel {channel_id}: {err}")
            })?;
        Ok(())
    }

    pub async fn delete_channel(&self, channel_id: u64) -> Result<(), String> {
        self.client
            .delete_channel(Id::<ChannelMarker>::new(channel_id))
            .reason(TEMP_CHANNEL_DELETE_REASON)
            .await
            .map_err(|err| format!("failed to delete temp voice channel {channel_id}: {err}"))?;
        Ok(())
    }

    pub async fn respond_interaction_ephemeral(
        &self,
        application_id: u64,
        interaction_id: u64,
        token: &str,
        content: &str,
    ) -> Result<(), String> {
        let response = InteractionResponse {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(InteractionResponseData {
                allowed_mentions: Some(AllowedMentions::default()),
                content: Some(content.to_owned()),
                flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
                ..InteractionResponseData::default()
            }),
        };

        self.client
            .interaction(Id::<ApplicationMarker>::new(application_id))
            .create_response(
                Id::<InteractionMarker>::new(interaction_id),
                token,
                &response,
            )
            .await
            .map_err(|err| format!("failed to respond to temp voice interaction: {err}"))?;
        Ok(())
    }
}

fn is_not_found(err: &TwilightHttpError) -> bool {
    matches!(
        err.kind(),
        TwilightHttpErrorType::Response { status, .. } if status.get() == 404
    )
}
