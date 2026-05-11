use std::sync::Arc;
use twilight_http::request::AuditLogReason;
use twilight_http::Client as DiscordHttpClient;
use twilight_model::channel::message::component::Component;
use twilight_model::channel::message::embed::Embed;
use twilight_model::channel::message::{AllowedMentions, Message as DiscordMessage, MessageFlags};
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};
use twilight_model::id::marker::{
    ApplicationMarker, ChannelMarker, GuildMarker, InteractionMarker, MessageMarker, RoleMarker,
    UserMarker,
};
use twilight_model::id::Id;

const VACATION_ROLE_AUDIT_REASON: &str = "XIII vacation role update";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacationRoleChange {
    pub user_id: u64,
    pub role_id: u64,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VacationOfficerPing {
    pub content: String,
    pub allowed_role_mentions: Vec<u64>,
}

pub fn approve_role_change(user_id: u64, role_id: u64) -> VacationRoleChange {
    VacationRoleChange {
        user_id,
        role_id,
        reason: "Approved vacation request",
    }
}

pub fn officer_review_ping(role_id: Option<u64>, request_id: i64) -> VacationOfficerPing {
    let _ = request_id;
    match role_id {
        Some(role_id) => VacationOfficerPing {
            content: format!("<@&{role_id}>"),
            allowed_role_mentions: vec![role_id],
        },
        None => VacationOfficerPing {
            content: String::new(),
            allowed_role_mentions: Vec::new(),
        },
    }
}

#[derive(Clone)]
pub struct VacationDiscordHttp {
    client: Arc<DiscordHttpClient>,
}

impl VacationDiscordHttp {
    pub fn new(client: Arc<DiscordHttpClient>) -> Self {
        Self { client }
    }

    pub async fn add_vacation_role(
        &self,
        guild_id: u64,
        user_id: u64,
        role_id: u64,
    ) -> Result<(), String> {
        self.client
            .add_guild_member_role(
                Id::<GuildMarker>::new(guild_id),
                Id::<UserMarker>::new(user_id),
                Id::<RoleMarker>::new(role_id),
            )
            .reason(VACATION_ROLE_AUDIT_REASON)
            .await
            .map(|_| ())
            .map_err(|err| format!("failed to add vacation role {role_id} to {user_id}: {err}"))
    }

    pub async fn remove_vacation_role(
        &self,
        guild_id: u64,
        user_id: u64,
        role_id: u64,
    ) -> Result<(), String> {
        self.client
            .remove_guild_member_role(
                Id::<GuildMarker>::new(guild_id),
                Id::<UserMarker>::new(user_id),
                Id::<RoleMarker>::new(role_id),
            )
            .reason(VACATION_ROLE_AUDIT_REASON)
            .await
            .map(|_| ())
            .map_err(|err| {
                format!("failed to remove vacation role {role_id} from {user_id}: {err}")
            })
    }

    pub async fn send_officer_review(
        &self,
        channel_id: u64,
        ping: VacationOfficerPing,
        embed: Option<Embed>,
    ) -> Result<DiscordMessage, String> {
        let allowed = allowed_mentions_for_roles(&ping.allowed_role_mentions);
        let mut request = self
            .client
            .create_message(Id::<ChannelMarker>::new(channel_id))
            .allowed_mentions(Some(&allowed))
            .content(&ping.content);
        let embeds;
        if let Some(embed) = embed {
            embeds = vec![embed];
            request = request.embeds(&embeds);
        }
        request
            .await
            .map_err(|err| format!("failed to send vacation officer review: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode vacation officer review message: {err}"))
    }

    pub async fn edit_active_panel(
        &self,
        channel_id: u64,
        message_id: u64,
        embeds: &[Embed],
    ) -> Result<DiscordMessage, String> {
        self.client
            .update_message(
                Id::<ChannelMarker>::new(channel_id),
                Id::<MessageMarker>::new(message_id),
            )
            .allowed_mentions(Some(&AllowedMentions::default()))
            .content(None)
            .embeds(Some(embeds))
            .await
            .map_err(|err| format!("failed to edit active vacations panel: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode active vacations panel edit: {err}"))
    }

    pub async fn dm_user(&self, user_id: u64, content: &str) -> Result<Option<u64>, String> {
        self.dm_user_embed(user_id, content, None, &[]).await
    }

    pub async fn dm_user_embed(
        &self,
        user_id: u64,
        content: &str,
        embed: Option<Embed>,
        components: &[Component],
    ) -> Result<Option<u64>, String> {
        let channel = self
            .client
            .create_private_channel(Id::<UserMarker>::new(user_id))
            .await
            .map_err(|err| format!("failed to open vacation DM for user {user_id}: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode vacation DM channel: {err}"))?;

        let allowed_mentions = AllowedMentions::default();
        let mut request = self
            .client
            .create_message(channel.id)
            .allowed_mentions(Some(&allowed_mentions))
            .components(components);
        if !content.is_empty() {
            request = request.content(content);
        }
        let embeds;
        if let Some(embed) = embed {
            embeds = vec![embed];
            request = request.embeds(&embeds);
        }
        let message = request
            .await
            .map_err(|err| format!("failed to send vacation DM to {user_id}: {err}"))?
            .model()
            .await
            .map_err(|err| format!("failed to decode vacation DM response: {err}"))?;
        Ok(Some(message.id.get()))
    }

    pub async fn respond_ephemeral(
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
                flags: Some(MessageFlags::EPHEMERAL),
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
            .map(|_| ())
            .map_err(|err| format!("failed to respond to vacation interaction: {err}"))
    }
}

pub fn allowed_mentions_for_roles(role_ids: &[u64]) -> AllowedMentions {
    AllowedMentions {
        roles: role_ids
            .iter()
            .copied()
            .map(Id::<RoleMarker>::new)
            .collect(),
        ..AllowedMentions::default()
    }
}
